import { join, resolve } from "node:path";
import { openDb, migrate, loadExtensions, getHealth } from "@moneypenny/db";
import { processWorkQueue } from "@moneypenny/db";
import { assembleSystemPrompt } from "@moneypenny/db";
import { startStdioServer } from "@moneypenny/mcp";
import {
  runAgentTurn,
  recordTurn,
  checkBudget,
  loadBudgetConfig,
  embedChunks,
  detectConventions,
  extractSkills,
  AgentPool,
  createDefaultHooks,
  runCustodian,
  assembleContextForView,
  credentialRedactor,
  operationLogger,
  configureLlm,
} from "@moneypenny/engine";
import type { BudgetConfig, ContextView } from "@moneypenny/engine";
import { syncAllConfigs } from "./config.js";
import { startWatcher } from "./watcher.js";
import { indexDirectory } from "./indexer.js";
import { scaffoldConfig } from "./scaffold.js";
import { loadRootConfig, scaffoldRootConfig } from "./root-config.js";
import { createDashboard } from "./dashboard.js";
import {
  formatMarkdown,
  handleSlashCommand,
  formatToolProgress,
} from "./repl.js";
import type { Database } from "bun:sqlite";

const DATA_DIR =
  process.env.MP_DATA ??
  join(process.env.HOME ?? "~", ".moneypenny");
const DB_PATH = join(DATA_DIR, "moneypenny.db");
const SQL_DIR = resolve(import.meta.dir, "../../../packages/db/sql");
const EXTENSIONS_DIR = join(DATA_DIR, "extensions");
const DEFAULT_MODEL =
  process.env.MP_MODEL ?? "claude-sonnet-4-20250514";

const DIM = "\x1b[2m";
const RESET = "\x1b[0m";
const BOLD = "\x1b[1m";

interface ResolvedAgent {
  name: string;
  model: string;
  systemPrompt: string | null;
  toolFilter: string[] | null;
}

function resolveAgent(db: Database, nameHint?: string): ResolvedAgent {
  const name = nameHint ?? "Moneypenny";

  const row = db
    .query<
      {
        name: string;
        model: string | null;
        system_prompt: string | null;
        tools: string | null;
      },
      [string]
    >(
      "SELECT name, model, system_prompt, tools FROM agent_defs WHERE name = ?",
    )
    .get(name);

  if (row) {
    return {
      name: row.name,
      model: row.model ?? DEFAULT_MODEL,
      systemPrompt: row.system_prompt,
      toolFilter: row.tools ? JSON.parse(row.tools) : null,
    };
  }

  const fallback = db
    .query<
      {
        name: string;
        model: string | null;
        system_prompt: string | null;
        tools: string | null;
      },
      []
    >("SELECT name, model, system_prompt, tools FROM agent_defs LIMIT 1")
    .get();

  if (fallback) {
    return {
      name: fallback.name,
      model: fallback.model ?? DEFAULT_MODEL,
      systemPrompt: fallback.system_prompt,
      toolFilter: fallback.tools ? JSON.parse(fallback.tools) : null,
    };
  }

  return {
    name,
    model: DEFAULT_MODEL,
    systemPrompt: null,
    toolFilter: null,
  };
}

async function ensureIndexed(db: Database, repoRoot: string): Promise<void> {
  const chunkCount = db
    .query<{ cnt: number }, []>(
      "SELECT COUNT(*) as cnt FROM code_chunks",
    )
    .get()?.cnt ?? 0;

  if (chunkCount === 0) {
    process.stdout.write("  indexing codebase...");
    const indexed = await indexDirectory(db, repoRoot, (n) => {
      process.stdout.write(`\r  indexing codebase... ${n} files`);
    });
    console.log(`\r  indexed ${indexed} files              `);
  }
}

async function initProject(db: Database, repoRoot: string, applied: number): Promise<void> {
  const mpDir = join(repoRoot, ".moneypenny");

  const scaffolded = await scaffoldConfig(repoRoot);
  if (scaffolded) console.log("  created .moneypenny/ config");

  const rootConfigCreated = await scaffoldRootConfig(repoRoot);
  if (rootConfigCreated) console.log("  created moneypenny.toml");

  await syncAllConfigs(db, mpDir);
}

async function main() {
  const args = process.argv.slice(2);
  const command = args[0] ?? "start";

  await Bun.write(join(DATA_DIR, ".keep"), "");

  const db = openDb({ path: DB_PATH, sqlDir: SQL_DIR });

  try {
    loadExtensions(db, EXTENSIONS_DIR);
  } catch {}

  const applied = await migrate(db, SQL_DIR);

  // Load model config from DB if previously saved, or from root config
  const savedLlmConfig = db
    .query<{ value: string }, [string]>("SELECT value FROM config WHERE key = ?")
    .get("llm.config");
  if (savedLlmConfig) {
    try { configureLlm(JSON.parse(savedLlmConfig.value)); } catch {}
  }

  switch (command) {
    case "start": {
      const repoRoot = process.cwd();
      const rootConfig = await loadRootConfig(repoRoot);

      if (rootConfig.models) {
        configureLlm({
          strong: rootConfig.models.strong,
          fast: rootConfig.models.fast,
          local: rootConfig.models.local || undefined,
          ollamaBaseUrl: rootConfig.models.ollama_base_url,
        });
      }

      console.log("moneypenny v0.3.0");
      console.log(`  db: ${DB_PATH}`);
      if (applied > 0) console.log(`  applied ${applied} migration(s)`);

      await initProject(db, repoRoot, applied);

      const { configWatcher, codeWatcher } = startWatcher(
        db,
        repoRoot,
        join(repoRoot, ".moneypenny"),
      );

      console.log(`  watching: ${repoRoot}`);

      const port = parseInt(process.env.MP_PORT ?? "4966", 10);
      const dashboard = createDashboard(db);
      Bun.serve({ port, fetch: dashboard.fetch });
      console.log(`  dashboard: http://localhost:${port}`);

      startWorkLoop(db, rootConfig);

      process.on("SIGINT", () => {
        configWatcher.close();
        codeWatcher.close();
        db.close();
        process.exit(0);
      });
      break;
    }

    case "serve": {
      const repoRoot = process.cwd();

      if (applied > 0) console.error(`  applied ${applied} migration(s)`);

      await initProject(db, repoRoot, applied);
      await ensureIndexed(db, repoRoot);

      await startStdioServer(db);
      break;
    }

    case "chat": {
      const repoRoot = process.cwd();

      console.log("moneypenny v0.3.0");
      if (applied > 0) console.log(`  applied ${applied} migration(s)`);

      await initProject(db, repoRoot, applied);
      await ensureIndexed(db, repoRoot);

      const resumeIdx = args.indexOf("--resume");
      let sessionId: string;
      let messages: Array<{ role: "user" | "assistant"; content: string }> = [];
      let agentNameHint = args.find(
        (a, i) => i > 0 && !a.startsWith("-") && (resumeIdx < 0 || i !== resumeIdx + 1),
      );

      if (resumeIdx >= 0 && args[resumeIdx + 1]) {
        const resumeKey = args[resumeIdx + 1];
        const session = db
          .query<{ id: string; agent_name: string | null }, [string]>(
            "SELECT id, agent_name FROM sessions WHERE id LIKE ? || '%' ORDER BY created_at DESC LIMIT 1",
          )
          .get(resumeKey);

        if (!session) {
          console.error(`  session not found: ${resumeKey}`);
          db.close();
          process.exit(1);
        }

        sessionId = session.id;
        agentNameHint = session.agent_name ?? agentNameHint;

        db.query(
          "UPDATE sessions SET is_active = 1, last_active_at = unixepoch() WHERE id = ?",
        ).run(sessionId);

        const history = db
          .query<{ role: string; content: string }, [string]>(
            "SELECT role, content FROM messages WHERE session_id = ? AND content IS NOT NULL ORDER BY turn ASC",
          )
          .all(sessionId);

        messages = history.map((m) => ({
          role: m.role as "user" | "assistant",
          content: m.content,
        }));

        console.log(`  resumed session: ${sessionId.slice(0, 8)} (${history.length} messages)`);
      } else {
        sessionId = crypto.randomUUID();
      }

      const agent = resolveAgent(db, agentNameHint);
      const budgetConfig = loadBudgetConfig(db);
      const hooks = createDefaultHooks(db);

      console.log(`  agent: ${agent.name}`);
      console.log(`  model: ${agent.model}`);
      if (agent.toolFilter) {
        console.log(`  tools: ${agent.toolFilter.join(", ")}`);
      }
      console.log(`  type /help for commands, "exit" or Ctrl+C to quit\n`);

      if (resumeIdx < 0) {
        db.query(
          `INSERT INTO sessions (id, agent_name, created_at, last_active_at, is_active)
           VALUES (?, ?, unixepoch(), unixepoch(), 1)`,
        ).run(sessionId, agent.name);
      }

      const reader = createLineReader();

      for await (const line of reader) {
        const input = line.trim();
        if (!input) continue;
        if (input === "exit" || input === "quit") break;

        if (input.startsWith("/")) {
          const result = handleSlashCommand(input, db, sessionId, agent.name);
          if (result.output) console.log(result.output);
          if (result.action === "quit") break;
          if (result.action === "clear") {
            messages = [];
            console.log("  conversation cleared\n");
          }
          continue;
        }

        const budgetCheck = checkBudget(db, sessionId, budgetConfig);
        if (budgetCheck) {
          if (budgetCheck.effect === "deny") {
            console.log(`\n  ${BOLD}[budget]${RESET} ${budgetCheck.reason}\n`);
            break;
          }
          if (budgetCheck.effect === "warn") {
            console.log(`  ${DIM}[budget] ${budgetCheck.reason}${RESET}`);
          }
        }

        await hooks.run({
          phase: "pre-turn",
          agentName: agent.name,
          sessionId,
          input,
          messages,
        });

        messages.push({ role: "user", content: input });
        recordTurn(db, sessionId, "user", input);

        try {
          const result = runAgentTurn(
            {
              db,
              model: agent.model,
              agentName: agent.name,
              toolFilter: agent.toolFilter ?? undefined,
            },
            messages,
          );

          let fullText = "";
          for await (const event of result.textStream) {
            process.stdout.write(event);
            fullText += event;
          }
          process.stdout.write("\n\n");

          const formattedText = formatMarkdown(fullText);
          // Re-render with formatting if terminal supports it
          if (process.stdout.isTTY && fullText.includes("```")) {
            process.stdout.write(`\x1b[${fullText.split("\n").length + 1}A\x1b[J`);
            console.log(formattedText);
            console.log();
          }

          const usage = await result.usage;
          messages.push({ role: "assistant", content: fullText });
          recordTurn(
            db,
            sessionId,
            "assistant",
            fullText,
            agent.model,
            {
              promptTokens: usage.promptTokens,
              completionTokens: usage.completionTokens,
            },
          );

          await hooks.run({
            phase: "post-turn",
            agentName: agent.name,
            sessionId,
            output: fullText,
            usage,
            model: agent.model,
          });

          db.query(
            "UPDATE sessions SET last_active_at = unixepoch() WHERE id = ?",
          ).run(sessionId);
        } catch (err) {
          console.error(
            `\nError: ${err instanceof Error ? err.message : String(err)}\n`,
          );
        }
      }

      db.query(
        "UPDATE sessions SET is_active = 0, last_active_at = unixepoch() WHERE id = ?",
      ).run(sessionId);
      db.close();
      break;
    }

    case "index": {
      const force = args.includes("--force");
      const repoRoot =
        args.find((a, i) => i > 0 && !a.startsWith("-")) ?? process.cwd();
      console.log("moneypenny v0.3.0");
      if (applied > 0) console.log(`  applied ${applied} migration(s)`);

      await initProject(db, repoRoot, applied);

      if (force) {
        const deleted = db
          .query<{ cnt: number }, []>(
            "SELECT COUNT(*) as cnt FROM code_chunks",
          )
          .get()?.cnt ?? 0;
        db.exec("DELETE FROM code_chunks");
        db.exec("DELETE FROM file_tree");
        console.log(`  cleared ${deleted} existing chunks`);
      }

      process.stdout.write(`  indexing ${repoRoot}...`);
      const indexed = await indexDirectory(db, repoRoot, (n) => {
        process.stdout.write(`\r  indexing ${repoRoot}... ${n} files`);
      });
      console.log(`\r  indexed ${indexed} files from ${repoRoot}              `);

      const chunkCount = db
        .query<{ cnt: number }, []>(
          "SELECT COUNT(*) as cnt FROM code_chunks",
        )
        .get()?.cnt ?? 0;
      console.log(`  total chunks: ${chunkCount}`);
      db.close();
      break;
    }

    case "embed": {
      console.log("moneypenny v0.3.0");
      if (applied > 0) console.log(`  applied ${applied} migration(s)`);

      const batchSize = parseInt(args[1] ?? "50", 10);
      const unembedded = db
        .query<{ cnt: number }, []>(
          "SELECT COUNT(*) as cnt FROM code_chunks WHERE embedding IS NULL",
        )
        .get()?.cnt ?? 0;

      if (unembedded === 0) {
        console.log("  all chunks already have embeddings");
        db.close();
        break;
      }

      console.log(`  ${unembedded} chunks need embeddings`);

      let totalEmbedded = 0;
      while (true) {
        const embedded = await embedChunks(db, batchSize);
        if (embedded === 0) break;
        totalEmbedded += embedded;
        process.stdout.write(`\r  embedded ${totalEmbedded}/${unembedded} chunks`);
      }
      console.log(`\r  embedded ${totalEmbedded} chunks              `);
      db.close();
      break;
    }

    case "detect": {
      console.log("moneypenny v0.3.0");
      if (applied > 0) console.log(`  applied ${applied} migration(s)`);

      console.log("  analyzing code patterns...");
      const detected = await detectConventions(db, DEFAULT_MODEL);
      console.log(`  detected ${detected} new convention(s)`);

      const all = db
        .query<{ name: string; category: string; description: string; confidence: number }, []>(
          "SELECT name, category, description, confidence FROM conventions ORDER BY confidence DESC",
        )
        .all();
      for (const c of all) {
        console.log(
          `  ${DIM}[${c.category}]${RESET} ${c.name}: ${c.description} ${DIM}(${(c.confidence * 100).toFixed(0)}%)${RESET}`,
        );
      }
      db.close();
      break;
    }

    case "skills": {
      const sub = args[1];
      if (sub === "extract" && args[2]) {
        const sid = args[2];
        const session = db
          .query<{ id: string }, [string]>(
            "SELECT id FROM sessions WHERE id LIKE ? || '%'",
          )
          .get(sid);
        if (!session) {
          console.log(`Session not found: ${sid}`);
          db.close();
          break;
        }
        console.log("  extracting skills...");
        const extracted = await extractSkills(db, session.id, DEFAULT_MODEL);
        console.log(`  extracted ${extracted} skill(s)`);
      }

      const skills = db
        .query<
          { name: string; description: string; instructions: string | null; confidence: number },
          []
        >(
          "SELECT name, description, instructions, confidence FROM skills WHERE confidence > 0.2 ORDER BY confidence DESC",
        )
        .all();

      if (skills.length === 0) {
        console.log("  no skills learned yet");
      } else {
        for (const s of skills) {
          console.log(
            `  ${BOLD}${s.name}${RESET} ${DIM}(${(s.confidence * 100).toFixed(0)}%)${RESET}: ${s.description}`,
          );
          if (s.instructions) {
            console.log(`    ${DIM}${s.instructions}${RESET}`);
          }
        }
      }
      db.close();
      break;
    }

    case "pool": {
      const sub = args[1];
      const pool = new AgentPool({ db, defaultModel: DEFAULT_MODEL });

      if (sub === "run" && args[2]) {
        const agentName = args[2];
        const task = args.slice(3).join(" ");
        if (!task) {
          console.log("Usage: mp pool run <agent> <task>");
          db.close();
          break;
        }

        console.log(`  submitting to ${agentName}...`);
        const jobId = await pool.submit(agentName, task);
        console.log(`  job: ${jobId}`);

        // Wait for completion
        const start = performance.now();
        while (!pool.getResult(jobId)) {
          await new Promise((r) => setTimeout(r, 1000));
          const elapsed = ((performance.now() - start) / 1000).toFixed(0);
          process.stdout.write(`\r  waiting... ${elapsed}s`);
        }

        const result = pool.getResult(jobId)!;
        console.log(`\r  completed in ${(result.durationMs / 1000).toFixed(1)}s  $${result.costUsd.toFixed(4)}              `);
        if (result.error) {
          console.error(`  error: ${result.error}`);
        } else {
          console.log(`\n${formatMarkdown(result.response)}`);
        }
      } else if (sub === "schedule") {
        const processed = await pool.processScheduledJobs();
        console.log(`  ran ${processed} scheduled job(s)`);
      } else {
        console.log("Usage:");
        console.log('  mp pool run <agent> <task>   Run agent with a task');
        console.log("  mp pool schedule             Process scheduled jobs");
      }
      db.close();
      break;
    }

    case "status": {
      const health = getHealth(db);
      console.log(JSON.stringify(health, null, 2));
      db.close();
      break;
    }

    case "context": {
      const agentName = args[1] ?? "Moneypenny";
      const prompt = assembleSystemPrompt(db, agentName);
      console.log(prompt);
      db.close();
      break;
    }

    case "work": {
      if (applied > 0) console.log(`  applied ${applied} migration(s)`);
      const processed = await processWorkQueue({
        db,
        model: DEFAULT_MODEL,
        pointerCap: 20,
        batchSize: 10,
        embedFn: embedChunks,
        detectConventionsFn: detectConventions,
        extractSkillsFn: extractSkills,
      });
      console.log(`  processed ${processed} work item(s)`);
      db.close();
      break;
    }

    case "agents": {
      const defs = db
        .query<
          {
            name: string;
            model: string | null;
            trigger_on: string | null;
            tools: string | null;
          },
          []
        >("SELECT name, model, trigger_on, tools FROM agent_defs")
        .all();
      if (defs.length === 0) {
        console.log(
          'No agents defined. Run "mp start" or "mp chat" to scaffold .moneypenny/agents/',
        );
      } else {
        for (const d of defs) {
          const tools = d.tools ? JSON.parse(d.tools).length : "all";
          console.log(
            `  ${d.name}  model=${d.model ?? "default"}  trigger=${d.trigger_on ?? "manual"}  tools=${tools}`,
          );
        }
      }
      db.close();
      break;
    }

    case "policies": {
      const policies = db
        .query<
          {
            name: string;
            effect: string;
            description: string;
            conditions: string | null;
          },
          []
        >(
          "SELECT name, effect, description, conditions FROM policies WHERE enabled = 1",
        )
        .all();
      if (policies.length === 0) {
        console.log("No policies defined.");
      } else {
        for (const p of policies) {
          console.log(`  [${p.effect}] ${p.name}: ${p.description}`);
          if (p.conditions) {
            const conds = JSON.parse(p.conditions);
            for (const [k, v] of Object.entries(conds)) {
              console.log(`    ${k}: ${v}`);
            }
          }
        }
      }
      db.close();
      break;
    }

    case "sessions": {
      const sub = args[1];
      if (sub === "search") {
        const query = args.slice(2).join(" ");
        if (!query) {
          console.log("Usage: mp sessions search <query>");
          break;
        }
        const results = db
          .query<{ session_id: string; content: string }, [string]>(
            `SELECT m.session_id, m.content FROM messages_fts fts
             JOIN messages m ON m.rowid = fts.rowid
             WHERE messages_fts MATCH ? ORDER BY rank LIMIT 20`,
          )
          .all(query);
        for (const r of results) {
          console.log(
            `[${r.session_id.slice(0, 8)}] ${r.content.slice(0, 120)}`,
          );
        }
      } else if (sub && sub !== "list") {
        const sessionId = sub;
        const session = db
          .query<
            {
              id: string;
              label: string | null;
              agent_name: string | null;
              created_at: number;
            },
            [string]
          >(
            "SELECT id, label, agent_name, created_at FROM sessions WHERE id LIKE ? || '%'",
          )
          .get(sessionId);
        if (!session) {
          console.log(`Session not found: ${sessionId}`);
          break;
        }
        console.log(`Session: ${session.id}`);
        console.log(`  Label: ${session.label ?? "(none)"}`);
        console.log(`  Agent: ${session.agent_name ?? "(default)"}`);
        console.log(
          `  Created: ${new Date(session.created_at * 1000).toISOString()}`,
        );
        console.log("");
        const msgs = db
          .query<
            { role: string; content: string; cost_usd: number | null },
            [string]
          >(
            "SELECT role, content, cost_usd FROM messages WHERE session_id = ? AND content IS NOT NULL ORDER BY turn ASC",
          )
          .all(session.id);
        for (const m of msgs) {
          const prefix = m.role === "user" ? ">>> " : "    ";
          const cost =
            m.cost_usd != null ? ` ($${m.cost_usd.toFixed(4)})` : "";
          console.log(`${prefix}${m.content.slice(0, 200)}${cost}`);
          console.log("");
        }
      } else {
        const sessions = db
          .query<
            {
              id: string;
              label: string | null;
              agent_name: string | null;
              is_active: number;
              created_at: number;
            },
            []
          >(
            "SELECT id, label, agent_name, is_active, created_at FROM sessions ORDER BY created_at DESC LIMIT 20",
          )
          .all();
        if (sessions.length === 0) {
          console.log("No sessions yet.");
          break;
        }
        for (const s of sessions) {
          const active = s.is_active ? " [active]" : "";
          const date = new Date(s.created_at * 1000)
            .toISOString()
            .split("T")[0];
          console.log(
            `  ${s.id.slice(0, 8)}  ${date}  ${s.label ?? "(unlabeled)"}  ${s.agent_name ?? ""}${active}`,
          );
        }
      }
      db.close();
      break;
    }

    case "costs": {
      const sub = args[1];
      if (sub === "today") {
        const row = db
          .query<
            {
              total: number;
              sessions: number;
              tokens_in: number;
              tokens_out: number;
            },
            []
          >("SELECT * FROM v_cost_today")
          .get();
        if (row) {
          console.log(
            `  today: $${row.total.toFixed(4)}  sessions=${row.sessions}  tokens_in=${row.tokens_in}  tokens_out=${row.tokens_out}`,
          );
        }
      } else {
        const rows = db
          .query<
            {
              day: string;
              agent_name: string | null;
              turns: number;
              total_cost: number;
            },
            []
          >("SELECT * FROM v_cost_summary ORDER BY day DESC LIMIT 30")
          .all();
        if (rows.length === 0) {
          console.log("No cost data yet.");
          break;
        }
        for (const r of rows) {
          console.log(
            `  ${r.day}  ${r.agent_name ?? "(default)"}  turns=${r.turns}  $${r.total_cost.toFixed(4)}`,
          );
        }
      }
      db.close();
      break;
    }

    case "custodian": {
      console.log("moneypenny v0.3.0 — custodian run");
      if (applied > 0) console.log(`  applied ${applied} migration(s)`);

      const repoRoot = process.cwd();
      const rootConfig = await loadRootConfig(repoRoot);

      console.log("  running custodian pipeline...");
      const result = await runCustodian(db, {
        model: rootConfig.agent.model,
        pointerCap: rootConfig.pointers.cap,
        compactAfterTurns: rootConfig.custodian.compact_after_turns,
        archiveAfterDays: rootConfig.custodian.archive_after_days,
        purgeAfterDays: rootConfig.custodian.purge_after_days,
        chunkPruneAfterDays: rootConfig.custodian.chunk_prune_after_days,
      });
      console.log(`  labeled: ${result.labeled}`);
      console.log(`  compacted: ${result.compacted}`);
      console.log(`  archived: ${result.archived}`);
      console.log(`  purged: ${result.purged}`);
      console.log(`  summarized: ${result.summarized}`);
      console.log(`  consolidated: ${result.consolidated}`);
      console.log(`  stale chunks pruned: ${result.chunksP}`);
      console.log(`  duration: ${(result.durationMs / 1000).toFixed(1)}s`);
      db.close();
      break;
    }

    case "dashboard": {
      console.log("moneypenny v0.3.0 — dashboard");
      if (applied > 0) console.log(`  applied ${applied} migration(s)`);

      const port = parseInt(process.env.MP_PORT ?? "4966", 10);
      const dash = createDashboard(db);
      Bun.serve({ port, fetch: dash.fetch });
      console.log(`  http://localhost:${port}`);

      process.on("SIGINT", () => {
        db.close();
        process.exit(0);
      });
      break;
    }

    default:
      printUsage();
      db.close();
  }
}

function printUsage() {
  console.log("moneypenny v0.3.0 — local-first coding agent platform\n");
  console.log("Usage: mp <command>\n");
  console.log(`${BOLD}Core:${RESET}`);
  console.log("  start              Start watcher + dashboard + work loop");
  console.log("  serve              Start MCP server (stdio)");
  console.log("  chat [agent]       Interactive chat REPL");
  console.log("  chat --resume <id> Resume a previous session");
  console.log("  dashboard          Start web dashboard only");
  console.log("");
  console.log(`${BOLD}Intelligence:${RESET}`);
  console.log("  index [path]       Index codebase (--force to re-index)");
  console.log("  embed [batch]      Generate embeddings for code chunks");
  console.log("  detect             Detect project conventions from code");
  console.log("  skills             List learned skills");
  console.log("  skills extract <s> Extract skills from a session");
  console.log("  work               Process pending work queue");
  console.log("  custodian          Run custodian pipeline");
  console.log("");
  console.log(`${BOLD}Agent Pool:${RESET}`);
  console.log('  pool run <a> <t>   Run agent <a> with task <t>');
  console.log("  pool schedule      Process scheduled agent jobs");
  console.log("");
  console.log(`${BOLD}Inspect:${RESET}`);
  console.log("  status             Show database health");
  console.log("  context [agent]    Print assembled system prompt");
  console.log("  agents             List defined agents");
  console.log("  policies           List active policies");
  console.log("  sessions           List recent sessions");
  console.log("  sessions <id>      View session messages");
  console.log("  sessions search q  Search message history");
  console.log("  costs              Cost summary by day");
  console.log("  costs today        Today's spend");
}

function startWorkLoop(db: Database, rootConfig?: { pointers: { cap: number }; worker: { interval_ms: number; batch_size: number } }) {
  const interval = rootConfig?.worker.interval_ms ??
    parseInt(process.env.MP_WORK_INTERVAL ?? "30000", 10);
  const batchSize = rootConfig?.worker.batch_size ?? 5;
  const pointerCap = rootConfig?.pointers.cap ?? 20;

  setInterval(async () => {
    try {
      await processWorkQueue({
        db,
        model: DEFAULT_MODEL,
        pointerCap,
        batchSize,
        embedFn: embedChunks,
        detectConventionsFn: detectConventions,
        extractSkillsFn: extractSkills,
      });
    } catch {}
  }, interval);
}

async function* createLineReader(): AsyncGenerator<string> {
  process.stdout.write("you> ");
  const decoder = new TextDecoder();
  for await (const chunk of Bun.stdin.stream()) {
    const text = decoder.decode(chunk);
    const lines = text.split("\n");
    for (const line of lines) {
      if (line.length > 0) {
        yield line;
        process.stdout.write("you> ");
      }
    }
  }
}

main().catch((err) => {
  console.error("Fatal:", err);
  process.exit(1);
});
