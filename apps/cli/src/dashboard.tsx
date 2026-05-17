import { Hono } from "hono";
import type { Database } from "bun:sqlite";
import { getHealth } from "@moneypenny/db";
import { getLlmConfig, configureLlm } from "@moneypenny/engine";

export function createDashboard(db: Database): Hono {
  const app = new Hono();

  // ── Overview ──────────────────────────────────────────────────

  app.get("/", (c) => {
    const health = getHealth(db) as any;
    const cost = db.query<{ total: number; sessions: number; tokens_in: number; tokens_out: number }, []>(
      "SELECT * FROM v_cost_today",
    ).get();

    return c.html(layout("Overview", `
      <div class="grid">
        <div class="card">
          <h3>Sessions</h3>
          <div class="stat">${health?.total_sessions ?? 0}</div>
          <div class="sub">${health?.active_sessions ?? 0} active</div>
        </div>
        <div class="card">
          <h3>Messages</h3>
          <div class="stat">${health?.total_messages ?? 0}</div>
        </div>
        <div class="card">
          <h3>Code Chunks</h3>
          <div class="stat">${health?.total_chunks ?? 0}</div>
          <div class="sub">${health?.total_skills ?? 0} skills</div>
        </div>
        <div class="card">
          <h3>Today's Cost</h3>
          <div class="stat">$${(cost?.total ?? 0).toFixed(4)}</div>
          <div class="sub">${cost?.tokens_in ?? 0} in / ${cost?.tokens_out ?? 0} out</div>
        </div>
        <div class="card">
          <h3>Work Queue</h3>
          <div class="stat">${health?.pending_work ?? 0}</div>
          <div class="sub">${health?.failed_work ?? 0} failed</div>
        </div>
      </div>
    `));
  });

  // ── Sessions ──────────────────────────────────────────────────

  app.get("/sessions", (c) => {
    const q = c.req.query("q");

    const all = db.query<
      { id: string; label: string | null; agent_name: string | null; is_active: number; created_at: number },
      []
    >("SELECT id, label, agent_name, is_active, created_at FROM sessions ORDER BY created_at DESC LIMIT 50").all();

    const rows = all.map((s) => `
      <tr>
        <td><a href="/sessions/${s.id}">${s.id.slice(0, 8)}</a></td>
        <td>${esc(s.label ?? "(unlabeled)")}</td>
        <td>${esc(s.agent_name ?? "")}</td>
        <td>${s.is_active ? '<span class="badge green">active</span>' : '<span class="badge">closed</span>'}</td>
        <td>${new Date(s.created_at * 1000).toISOString().split("T")[0]}</td>
      </tr>
    `).join("");

    return c.html(layout("Sessions", `
      <form method="get" class="search">
        <input name="q" placeholder="Search messages..." value="${esc(q ?? "")}" />
        <button type="submit">Search</button>
      </form>
      <table>
        <thead><tr><th>ID</th><th>Label</th><th>Agent</th><th>Status</th><th>Date</th></tr></thead>
        <tbody>${rows}</tbody>
      </table>
    `));
  });

  app.get("/sessions/:id", (c) => {
    const id = c.req.param("id");
    const session = db.query<
      { id: string; label: string | null; agent_name: string | null; created_at: number },
      [string]
    >("SELECT id, label, agent_name, created_at FROM sessions WHERE id = ?").get(id);

    if (!session) return c.text("Not found", 404);

    const msgs = db.query<
      { role: string; content: string; cost_usd: number | null; created_at: number },
      [string]
    >("SELECT role, content, cost_usd, created_at FROM messages WHERE session_id = ? AND content IS NOT NULL ORDER BY turn ASC").all(id);

    const msgHtml = msgs.map((m) => `
      <div class="msg ${m.role}">
        <div class="role">${m.role}${m.cost_usd ? ` ($${m.cost_usd.toFixed(4)})` : ""}</div>
        <div class="content"><pre>${esc(m.content)}</pre></div>
      </div>
    `).join("");

    return c.html(layout(`Session ${session.id.slice(0, 8)}`, `
      <h3>${esc(session.label ?? "(unlabeled)")}</h3>
      <p>Agent: ${esc(session.agent_name ?? "default")} | Created: ${new Date(session.created_at * 1000).toISOString()}</p>
      <div class="messages">${msgHtml}</div>
    `));
  });

  // ── Agents ────────────────────────────────────────────────────

  app.get("/agents", (c) => {
    const agents = db.query<
      { name: string; model: string | null; trigger_on: string | null; tools: string | null; system_prompt: string | null },
      []
    >("SELECT name, model, trigger_on, tools, system_prompt FROM agent_defs").all();

    const rows = agents.map((a) => {
      const toolList = a.tools ? JSON.parse(a.tools) : [];
      return `
      <tr>
        <td><strong>${esc(a.name)}</strong></td>
        <td><code>${esc(a.model ?? "default")}</code></td>
        <td>${esc(a.trigger_on ?? "manual")}</td>
        <td>${toolList.length > 0 ? toolList.length : "all"}</td>
        <td>${a.system_prompt ? `<span class="badge green">yes</span>` : '<span class="badge">no</span>'}</td>
      </tr>`;
    }).join("");

    return c.html(layout("Agents", `
      <table>
        <thead><tr><th>Name</th><th>Model</th><th>Trigger</th><th>Tools</th><th>System Prompt</th></tr></thead>
        <tbody>${rows}</tbody>
      </table>
    `));
  });

  // ── Events ────────────────────────────────────────────────────

  app.get("/events", (c) => {
    const typeFilter = c.req.query("type");
    const where = typeFilter ? `WHERE type = '${esc(typeFilter)}'` : "";

    const events = db.query<
      { type: string; agent_name: string | null; session_id: string | null; detail: string | null; created_at: number },
      []
    >(`SELECT type, agent_name, session_id, detail, created_at FROM events ${where} ORDER BY created_at DESC LIMIT 100`).all();

    const types = db.query<{ type: string; cnt: number }, []>(
      "SELECT type, COUNT(*) as cnt FROM events GROUP BY type ORDER BY cnt DESC LIMIT 20",
    ).all();

    const filterHtml = types.map((t) =>
      `<a href="/events?type=${encodeURIComponent(t.type)}" class="badge ${typeFilter === t.type ? "green" : ""}">${esc(t.type)} (${t.cnt})</a>`
    ).join(" ");

    const rows = events.map((e) => `
      <tr>
        <td>${new Date(e.created_at * 1000).toISOString().replace("T", " ").slice(0, 19)}</td>
        <td><code>${esc(e.type)}</code></td>
        <td>${esc(e.agent_name ?? "")}</td>
        <td>${e.session_id ? `<a href="/sessions/${e.session_id}">${e.session_id.slice(0, 8)}</a>` : ""}</td>
        <td><code>${esc((e.detail ?? "").slice(0, 120))}</code></td>
      </tr>
    `).join("");

    return c.html(layout("Events", `
      <div style="margin-bottom: 16px">${filterHtml} <a href="/events" class="badge">all</a></div>
      <table>
        <thead><tr><th>Time</th><th>Type</th><th>Agent</th><th>Session</th><th>Detail</th></tr></thead>
        <tbody>${rows}</tbody>
      </table>
    `));
  });

  // ── Costs ─────────────────────────────────────────────────────

  app.get("/costs", (c) => {
    const costs = db.query<
      { day: string; agent_name: string | null; turns: number; total_cost: number },
      []
    >("SELECT * FROM v_cost_summary ORDER BY day DESC LIMIT 30").all();

    const todayCost = db.query<{ total: number; sessions: number; tokens_in: number; tokens_out: number }, []>(
      "SELECT * FROM v_cost_today",
    ).get();

    const rows = costs.map((r) => `
      <tr>
        <td>${r.day}</td>
        <td>${esc(r.agent_name ?? "(default)")}</td>
        <td>${r.turns}</td>
        <td>$${r.total_cost.toFixed(4)}</td>
      </tr>
    `).join("");

    return c.html(layout("Costs", `
      <div class="grid" style="margin-bottom: 24px">
        <div class="card">
          <h3>Today</h3>
          <div class="stat">$${(todayCost?.total ?? 0).toFixed(4)}</div>
          <div class="sub">${todayCost?.sessions ?? 0} sessions, ${todayCost?.tokens_in ?? 0} in / ${todayCost?.tokens_out ?? 0} out</div>
        </div>
      </div>
      <table>
        <thead><tr><th>Day</th><th>Agent</th><th>Turns</th><th>Cost</th></tr></thead>
        <tbody>${rows}</tbody>
      </table>
    `));
  });

  // ── Skills ────────────────────────────────────────────────────

  app.get("/skills", (c) => {
    const skills = db.query<
      { name: string; description: string; instructions: string | null; confidence: number },
      []
    >("SELECT name, description, instructions, confidence FROM skills ORDER BY confidence DESC").all();

    const rows = skills.map((s) => `
      <tr>
        <td>${esc(s.name)}</td>
        <td>${esc(s.description)}</td>
        <td><code>${esc((s.instructions ?? "").slice(0, 80))}</code></td>
        <td>${(s.confidence * 100).toFixed(0)}%</td>
      </tr>
    `).join("");

    return c.html(layout("Skills", `
      <table>
        <thead><tr><th>Name</th><th>Description</th><th>Instructions</th><th>Confidence</th></tr></thead>
        <tbody>${rows}</tbody>
      </table>
      ${skills.length === 0 ? '<p style="color: #666; margin-top: 16px">No skills learned yet. Use <code>mp skills extract &lt;session_id&gt;</code> or the <code>learn_skill</code> tool.</p>' : ""}
    `));
  });

  // ── Conventions ───────────────────────────────────────────────

  app.get("/conventions", (c) => {
    const convs = db.query<
      { name: string; category: string; description: string; confidence: number },
      []
    >("SELECT name, category, description, confidence FROM conventions ORDER BY confidence DESC").all();

    const rows = convs.map((cv) => `
      <tr>
        <td><span class="badge">${esc(cv.category)}</span></td>
        <td>${esc(cv.name)}</td>
        <td>${esc(cv.description)}</td>
        <td>${(cv.confidence * 100).toFixed(0)}%</td>
      </tr>
    `).join("");

    return c.html(layout("Conventions", `
      <table>
        <thead><tr><th>Category</th><th>Name</th><th>Description</th><th>Confidence</th></tr></thead>
        <tbody>${rows}</tbody>
      </table>
      ${convs.length === 0 ? '<p style="color: #666; margin-top: 16px">No conventions detected yet. Run <code>mp detect</code> to analyze your codebase.</p>' : ""}
    `));
  });

  // ── Settings ──────────────────────────────────────────────────

  app.get("/settings", (c) => {
    const llmConfig = getLlmConfig();

    const envKeys = [
      { name: "ANTHROPIC_API_KEY", set: !!process.env.ANTHROPIC_API_KEY },
      { name: "OPENAI_API_KEY", set: !!process.env.OPENAI_API_KEY },
      { name: "GOOGLE_GENERATIVE_AI_API_KEY", set: !!process.env.GOOGLE_GENERATIVE_AI_API_KEY },
    ];

    const envRows = envKeys.map((k) => `
      <tr>
        <td><code>${k.name}</code></td>
        <td>${k.set ? '<span class="badge green">configured</span>' : '<span class="badge red">not set</span>'}</td>
      </tr>
    `).join("");

    const policies = db.query<{ name: string; effect: string; description: string; enabled: number }, []>(
      "SELECT name, effect, description, enabled FROM policies",
    ).all();

    const policyRows = policies.map((p) => `
      <tr>
        <td>${esc(p.name)}</td>
        <td><span class="badge ${p.effect === "deny" ? "red" : ""}">${p.effect}</span></td>
        <td>${esc(p.description)}</td>
        <td>${p.enabled ? '<span class="badge green">on</span>' : '<span class="badge">off</span>'}</td>
      </tr>
    `).join("");

    const configKv = db.query<{ key: string; value: string }, []>(
      "SELECT key, value FROM config",
    ).all();

    const configRows = configKv.map((kv) => `
      <tr>
        <td><code>${esc(kv.key)}</code></td>
        <td><code>${esc(kv.value.slice(0, 200))}</code></td>
      </tr>
    `).join("");

    return c.html(layout("Settings", `
      <div class="settings-section">
        <h3>Model Configuration</h3>
        <table>
          <thead><tr><th>Tier</th><th>Model</th><th>Usage</th></tr></thead>
          <tbody>
            <tr>
              <td><span class="badge green">strong</span></td>
              <td><code>${esc(llmConfig.strong)}</code></td>
              <td>Interactive chat, complex reasoning</td>
            </tr>
            <tr>
              <td><span class="badge">fast</span></td>
              <td><code>${esc(llmConfig.fast)}</code></td>
              <td>Summarization, convention detection, skill extraction</td>
            </tr>
            <tr>
              <td><span class="badge">local</span></td>
              <td><code>${esc(llmConfig.local ?? "(not configured — falls back to fast)")}</code></td>
              <td>Labeling, compaction, pointer generation</td>
            </tr>
          </tbody>
        </table>
        <p class="hint">Configure via <code>moneypenny.toml</code> <code>[models]</code> section or set <code>MP_MODEL</code> env var.<br>
        Local models use <code>ollama:</code> prefix, e.g. <code>ollama:llama3.2</code></p>
      </div>

      <div class="settings-section">
        <h3>API Keys</h3>
        <table>
          <thead><tr><th>Variable</th><th>Status</th></tr></thead>
          <tbody>${envRows}</tbody>
        </table>
        <p class="hint">Set these as environment variables before starting Moneypenny.</p>
      </div>

      <div class="settings-section">
        <h3>Policies</h3>
        <table>
          <thead><tr><th>Name</th><th>Effect</th><th>Description</th><th>Enabled</th></tr></thead>
          <tbody>${policyRows}</tbody>
        </table>
        ${policies.length === 0 ? '<p class="hint">No policies defined. Add them in <code>.moneypenny/policies/</code></p>' : ""}
      </div>

      ${configRows.length > 0 ? `
      <div class="settings-section">
        <h3>Config Store</h3>
        <table>
          <thead><tr><th>Key</th><th>Value</th></tr></thead>
          <tbody>${configRows}</tbody>
        </table>
      </div>
      ` : ""}
    `));
  });

  app.post("/settings/models", async (c) => {
    const body = await c.req.parseBody();
    const update: Record<string, string> = {};
    if (body.strong) update.strong = String(body.strong);
    if (body.fast) update.fast = String(body.fast);
    if (body.local) update.local = String(body.local);
    if (body.ollamaBaseUrl) update.ollamaBaseUrl = String(body.ollamaBaseUrl);
    configureLlm(update);

    db.query(
      "INSERT OR REPLACE INTO config (key, value) VALUES ('llm.config', ?)",
    ).run(JSON.stringify(getLlmConfig()));

    return c.redirect("/settings");
  });

  // ── MCP Servers ───────────────────────────────────────────────

  app.get("/mcps", (c) => {
    const configRow = db.query<{ value: string }, [string]>(
      "SELECT value FROM config WHERE key = ?",
    ).get("mcp.servers");

    let servers: Array<{ name: string; command: string; args?: string[]; env?: Record<string, string> }> = [];
    if (configRow) {
      try { servers = JSON.parse(configRow.value); } catch {}
    }

    const serverRows = servers.map((s, i) => `
      <tr>
        <td><strong>${esc(s.name)}</strong></td>
        <td><code>${esc(s.command)}</code></td>
        <td><code>${esc((s.args ?? []).join(" "))}</code></td>
        <td>${Object.keys(s.env ?? {}).length > 0 ? Object.keys(s.env!).length + " vars" : ""}</td>
        <td>
          <form method="post" action="/mcps/${encodeURIComponent(s.name)}/delete" style="display:inline">
            <button type="submit" class="btn-danger">Remove</button>
          </form>
        </td>
      </tr>
    `).join("");

    return c.html(layout("MCP Servers", `
      <p class="hint">MCP servers that Moneypenny can connect to as a client — consuming tools from other agents, IDEs, or services.</p>

      <table>
        <thead><tr><th>Name</th><th>Command</th><th>Args</th><th>Env</th><th></th></tr></thead>
        <tbody>
          ${serverRows}
          ${servers.length === 0 ? '<tr><td colspan="5" style="color: #666">No MCP servers configured</td></tr>' : ""}
        </tbody>
      </table>

      <div class="settings-section" style="margin-top: 24px">
        <h3>Add MCP Server</h3>
        <form method="post" action="/mcps/add" class="form-grid">
          <div class="form-row">
            <label>Name</label>
            <input name="name" placeholder="e.g. filesystem" required />
          </div>
          <div class="form-row">
            <label>Command</label>
            <input name="command" placeholder="e.g. npx" required />
          </div>
          <div class="form-row">
            <label>Args (space-separated)</label>
            <input name="args" placeholder="e.g. -y @modelcontextprotocol/server-filesystem /tmp" />
          </div>
          <div class="form-row">
            <label>Env (KEY=VALUE, one per line)</label>
            <textarea name="env" rows="3" placeholder="API_KEY=sk-..."></textarea>
          </div>
          <button type="submit">Add Server</button>
        </form>
      </div>
    `));
  });

  app.post("/mcps/add", async (c) => {
    const body = await c.req.parseBody();
    const name = String(body.name ?? "").trim();
    const command = String(body.command ?? "").trim();
    const argsStr = String(body.args ?? "").trim();
    const envStr = String(body.env ?? "").trim();

    if (!name || !command) return c.redirect("/mcps");

    const args = argsStr ? argsStr.split(/\s+/) : [];
    const env: Record<string, string> = {};
    if (envStr) {
      for (const line of envStr.split("\n")) {
        const eq = line.indexOf("=");
        if (eq > 0) env[line.slice(0, eq).trim()] = line.slice(eq + 1).trim();
      }
    }

    const configRow = db.query<{ value: string }, [string]>(
      "SELECT value FROM config WHERE key = ?",
    ).get("mcp.servers");

    let servers: Array<{ name: string; command: string; args?: string[]; env?: Record<string, string> }> = [];
    if (configRow) {
      try { servers = JSON.parse(configRow.value); } catch {}
    }

    servers = servers.filter((s) => s.name !== name);
    servers.push({ name, command, args: args.length > 0 ? args : undefined, env: Object.keys(env).length > 0 ? env : undefined });

    db.query("INSERT OR REPLACE INTO config (key, value) VALUES ('mcp.servers', ?)").run(JSON.stringify(servers));

    return c.redirect("/mcps");
  });

  app.post("/mcps/:name/delete", async (c) => {
    const name = c.req.param("name");

    const configRow = db.query<{ value: string }, [string]>(
      "SELECT value FROM config WHERE key = ?",
    ).get("mcp.servers");

    if (configRow) {
      let servers: Array<{ name: string }> = [];
      try { servers = JSON.parse(configRow.value); } catch {}
      servers = servers.filter((s) => s.name !== name);
      db.query("INSERT OR REPLACE INTO config (key, value) VALUES ('mcp.servers', ?)").run(JSON.stringify(servers));
    }

    return c.redirect("/mcps");
  });

  // ── Work Queue ────────────────────────────────────────────────

  app.get("/work", (c) => {
    const pending = db.query<{ type: string; session_id: string | null; created_at: number; error: string | null }, []>(
      "SELECT type, session_id, created_at, error FROM work_queue WHERE processed_at IS NULL ORDER BY created_at ASC LIMIT 50",
    ).all();

    const recent = db.query<{ type: string; session_id: string | null; created_at: number; processed_at: number; error: string | null }, []>(
      "SELECT type, session_id, created_at, processed_at, error FROM work_queue WHERE processed_at IS NOT NULL ORDER BY processed_at DESC LIMIT 30",
    ).all();

    const pendingRows = pending.map((w) => `
      <tr>
        <td><code>${esc(w.type)}</code></td>
        <td>${w.session_id ? `<a href="/sessions/${w.session_id}">${w.session_id.slice(0, 8)}</a>` : ""}</td>
        <td>${new Date(w.created_at * 1000).toISOString().replace("T", " ").slice(0, 19)}</td>
        <td>${w.error ? `<span class="badge red">error</span>` : '<span class="badge">pending</span>'}</td>
      </tr>
    `).join("");

    const recentRows = recent.map((w) => `
      <tr>
        <td><code>${esc(w.type)}</code></td>
        <td>${w.session_id?.slice(0, 8) ?? ""}</td>
        <td>${new Date((w.processed_at ?? w.created_at) * 1000).toISOString().replace("T", " ").slice(0, 19)}</td>
        <td>${w.error ? `<span class="badge red">failed</span>` : '<span class="badge green">done</span>'}</td>
      </tr>
    `).join("");

    return c.html(layout("Work Queue", `
      <h3>Pending (${pending.length})</h3>
      <table>
        <thead><tr><th>Type</th><th>Session</th><th>Created</th><th>Status</th></tr></thead>
        <tbody>${pendingRows}</tbody>
      </table>
      ${pending.length === 0 ? '<p style="color: #666; margin: 16px 0">No pending work items</p>' : ""}

      <h3 style="margin-top: 32px">Recent (${recent.length})</h3>
      <table>
        <thead><tr><th>Type</th><th>Session</th><th>Processed</th><th>Status</th></tr></thead>
        <tbody>${recentRows}</tbody>
      </table>
    `));
  });

  // ── API ───────────────────────────────────────────────────────

  app.get("/api/health", (c) => c.json(getHealth(db)));

  app.get("/api/config", (c) => {
    const config = db.query<{ key: string; value: string }, []>(
      "SELECT key, value FROM config",
    ).all();
    return c.json(Object.fromEntries(config.map((kv) => [kv.key, kv.value])));
  });

  app.get("/api/models", (c) => c.json(getLlmConfig()));

  return app;
}

function esc(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
}

function layout(title: string, body: string): string {
  return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Moneypenny — ${title}</title>
  <style>
    * { margin: 0; padding: 0; box-sizing: border-box; }
    body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui, sans-serif; background: #0a0a0a; color: #e0e0e0; }
    a { color: #60a5fa; text-decoration: none; }
    a:hover { text-decoration: underline; }

    nav { background: #111; border-bottom: 1px solid #222; padding: 12px 24px; display: flex; gap: 20px; align-items: center; flex-wrap: wrap; }
    nav .brand { font-weight: 700; font-size: 18px; color: #fff; margin-right: 8px; }
    nav a { color: #999; font-size: 14px; padding: 4px 0; }
    nav a:hover { color: #fff; text-decoration: none; }

    main { max-width: 1200px; margin: 24px auto; padding: 0 24px; }
    h2 { margin-bottom: 16px; }
    h3 { margin: 16px 0 12px; font-size: 16px; }

    .grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(200px, 1fr)); gap: 16px; margin-bottom: 24px; }
    .card { background: #161616; border: 1px solid #222; border-radius: 8px; padding: 20px; }
    .card h3 { font-size: 13px; color: #888; text-transform: uppercase; letter-spacing: 0.05em; margin: 0 0 8px; }
    .card .stat { font-size: 32px; font-weight: 700; }
    .card .sub { font-size: 13px; color: #666; margin-top: 4px; }

    table { width: 100%; border-collapse: collapse; }
    th, td { text-align: left; padding: 10px 12px; border-bottom: 1px solid #1a1a1a; }
    th { font-size: 12px; color: #666; text-transform: uppercase; letter-spacing: 0.05em; }
    tr:hover { background: #161616; }

    code { background: #1a1a1a; padding: 2px 6px; border-radius: 4px; font-size: 13px; }

    .badge { display: inline-block; font-size: 11px; padding: 2px 8px; border-radius: 4px; background: #222; color: #999; }
    .badge.green { background: #0f3d1a; color: #4ade80; }
    .badge.red { background: #3d0f0f; color: #f87171; }

    .search { margin-bottom: 16px; display: flex; gap: 8px; }
    .search input { flex: 1; padding: 8px 12px; background: #161616; border: 1px solid #333; border-radius: 6px; color: #e0e0e0; font-size: 14px; }
    .search button, button[type="submit"] { padding: 8px 16px; background: #2563eb; border: none; border-radius: 6px; color: white; cursor: pointer; font-size: 14px; }
    .search button:hover, button[type="submit"]:hover { background: #1d4ed8; }
    .btn-danger { background: #7f1d1d !important; }
    .btn-danger:hover { background: #991b1b !important; }

    .messages { display: flex; flex-direction: column; gap: 12px; }
    .msg { padding: 12px; border-radius: 8px; }
    .msg.user { background: #1a2332; border-left: 3px solid #2563eb; }
    .msg.assistant { background: #1a1a1a; border-left: 3px solid #22c55e; }
    .msg .role { font-size: 12px; color: #888; margin-bottom: 4px; text-transform: uppercase; }
    .msg pre { white-space: pre-wrap; word-break: break-word; font-size: 14px; line-height: 1.5; font-family: inherit; }

    .settings-section { margin-bottom: 32px; }
    .settings-section h3 { border-bottom: 1px solid #222; padding-bottom: 8px; }
    .hint { font-size: 13px; color: #666; margin-top: 8px; }

    .form-grid { display: flex; flex-direction: column; gap: 12px; max-width: 600px; }
    .form-row { display: flex; flex-direction: column; gap: 4px; }
    .form-row label { font-size: 13px; color: #888; }
    .form-row input, .form-row textarea { padding: 8px 12px; background: #161616; border: 1px solid #333; border-radius: 6px; color: #e0e0e0; font-size: 14px; font-family: inherit; }
  </style>
</head>
<body>
  <nav>
    <span class="brand">Moneypenny</span>
    <a href="/">Overview</a>
    <a href="/sessions">Sessions</a>
    <a href="/agents">Agents</a>
    <a href="/skills">Skills</a>
    <a href="/conventions">Conventions</a>
    <a href="/events">Events</a>
    <a href="/costs">Costs</a>
    <a href="/work">Work Queue</a>
    <a href="/mcps">MCP Servers</a>
    <a href="/settings">Settings</a>
  </nav>
  <main>
    <h2>${title}</h2>
    ${body}
  </main>
</body>
</html>`;
}
