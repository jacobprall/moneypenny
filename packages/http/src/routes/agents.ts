import { Hono } from "hono";
import { z } from "zod";
import {
  listAgentRows,
  getAgentRow,
  runAgent,
  scan,
} from "@moneypenny/agents";
import { createRequireDbMiddleware, zodErrorMessage, type HttpVars } from "../middleware.js";
import type { CreateHttpAppOptions } from "../types.js";

const RunBody = z.object({
  model: z.string().optional(),
});

export function createAgentsRouter(opts: CreateHttpAppOptions): Hono<{ Variables: HttpVars }> {
  const r = new Hono<{ Variables: HttpVars }>();
  r.use("*", createRequireDbMiddleware(opts.getDb));

  r.get("/agents", (c) => {
    const rows = listAgentRows(c.var.db.db).filter((a) => a.status !== "deleted");
    return c.json({ agents: rows });
  });

  r.get("/agents/:id", (c) => {
    const row = getAgentRow(c.var.db.db, c.req.param("id"));
    if (!row || row.status === "deleted") {
      return c.json({ error: "not found" }, 404);
    }
    let config: unknown;
    try {
      config = JSON.parse(row.configJson);
    } catch {
      config = null;
    }
    return c.json({ agent: row, config });
  });

  r.post("/agents/:id/run", async (c) => {
    const id = c.req.param("id");
    const row = getAgentRow(c.var.db.db, id);
    if (!row || row.status === "deleted") {
      return c.json({ error: "not found" }, 404);
    }
    if (!row.enabled) {
      return c.json({ error: "agent disabled" }, 409);
    }
    let body: unknown = {};
    try {
      body = await c.req.json();
    } catch {
      /* */
    }
    const parsed = RunBody.safeParse(body);
    if (!parsed.success) {
      return c.json({ error: zodErrorMessage(parsed.error) }, 400);
    }
    const apiKey = opts.getApiKey?.() ?? process.env.ANTHROPIC_API_KEY;
    if (!apiKey) {
      return c.json({ error: "No API key configured (set ANTHROPIC_API_KEY or pass getApiKey)" }, 501);
    }
    try {
      const result = await runAgent({
        agentDb: c.var.db,
        agentId: id,
        apiKey,
        model: parsed.data.model,
      });
      return c.json(result);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      return c.json({ error: msg }, 500);
    }
  });

  r.post("/agents/reload", (c) => {
    const dir = opts.blueprintsDir;
    if (!dir) {
      return c.json({ error: "agents directory not configured" }, 501);
    }
    const out = scan({ agentDb: c.var.db, blueprintsDir: dir });
    return c.json(out);
  });

  return r;
}
