/// <reference types="bun-types" />

import { existsSync } from "node:fs";
import { join } from "node:path";
import { Hono } from "hono";
import { z } from "zod";
import { createPolicy, listPolicies, syncPolicyFiles } from "@swe/db";
import { getIndexStatus, hybridSearch } from "@swe/search";
import { evaluatePolicy } from "@swe/ctx";
import { getSyncStatus, initSyncTables } from "@swe/cloud";
import { createRequireDbMiddleware, createTokenAuthMiddleware, zodErrorMessage, type HttpVars } from "./middleware.js";
import type { CreateHttpAppOptions } from "./types.js";
import { createAgentsRouter } from "./routes/agents.js";
import { createSessionsRouter } from "./routes/sessions.js";
import { createEventsRouter } from "./routes/events.js";

const SearchBody = z.object({
  query: z.string().min(1),
  limit: z.number().int().positive().max(500).optional(),
  languages: z.array(z.string()).optional(),
  paths: z.array(z.string()).optional(),
});

const PolicyEvaluateBody = z.object({
  actor: z.string().min(1),
  action: z.string().min(1),
  resource: z.string(),
  denyByDefault: z.boolean().optional(),
  sessionId: z.string().optional(),
});

const PolicyPostBody = z.object({
  name: z.string().min(1),
  effect: z.enum(["allow", "deny", "audit", "confirm"]),
  priority: z.number().optional(),
  toolPattern: z.string().nullable().optional(),
  pathPattern: z.string().nullable().optional(),
  costCondition: z.string().nullable().optional(),
  argsPattern: z.string().nullable().optional(),
  actorPattern: z.string().nullable().optional(),
  message: z.string().nullable().optional(),
});

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export function createHttpApp(opts: CreateHttpAppOptions): Hono<any, any, any> {
  const app = new Hono<any, any, any>();

  app.get("/health", (c) => {
    return c.json({ status: "ok", service: "swe", version: "0.1.0" });
  });

  app.get("/", (c) => {
    return c.json({
      name: "swe",
      service: "http",
      endpoints: ["/health", "/api/events", "/api/sessions", "/api/agents", "/api/policies", "/api/status", "/api/search", "/api/policy/evaluate"],
    });
  });

  const api = new Hono<any, any, any>();
  if (opts.getApiKey) {
    api.use("*", createTokenAuthMiddleware(opts.getApiKey));
  }
  api.use("*", createRequireDbMiddleware(opts.getDb));

  api.get("/status", (c) => {
    const db = c.var.db;
    const index = getIndexStatus(db);
    let sync = null as ReturnType<typeof getSyncStatus> | null;
    try {
      sync = getSyncStatus(db.db);
    } catch {
      sync = null;
    }
    return c.json({ index, sync });
  });

  api.post("/search", async (c) => {
    let body: unknown;
    try {
      body = await c.req.json();
    } catch {
      return c.json({ error: "Invalid JSON" }, 400);
    }
    const parsed = SearchBody.safeParse(body);
    if (!parsed.success) {
      return c.json({ error: zodErrorMessage(parsed.error) }, 400);
    }
    const results = hybridSearch(c.var.db, parsed.data.query, {
      limit: parsed.data.limit,
      languages: parsed.data.languages,
      paths: parsed.data.paths,
    });
    return c.json({ results });
  });

  api.post("/policy/evaluate", async (c) => {
    let body: unknown;
    try {
      body = await c.req.json();
    } catch {
      return c.json({ error: "Invalid JSON" }, 400);
    }
    const parsed = PolicyEvaluateBody.safeParse(body);
    if (!parsed.success) {
      return c.json({ error: zodErrorMessage(parsed.error) }, 400);
    }
    const b = parsed.data;
    const decision = evaluatePolicy(c.var.db.db, {
      actor: b.actor,
      toolName: b.action,
      path: b.resource,
    });
    const effect = decision.effect;
    return c.json({
      effect,
      matchedPolicy: decision.matchedPolicy
        ? { id: decision.matchedPolicy.id, name: decision.matchedPolicy.name }
        : null,
      reason: decision.reason,
    });
  });

  api.get("/policies", (c) => {
    return c.json({ policies: listPolicies(c.var.db) });
  });

  api.post("/policies", async (c) => {
    let body: unknown;
    try {
      body = await c.req.json();
    } catch {
      return c.json({ error: "Invalid JSON" }, 400);
    }
    const parsed = PolicyPostBody.safeParse(body);
    if (!parsed.success) {
      return c.json({ error: zodErrorMessage(parsed.error) }, 400);
    }
    const p = parsed.data;
    const created = createPolicy(c.var.db, {
      name: p.name,
      effect: p.effect,
      priority: p.priority ?? 0,
      toolPattern: p.toolPattern ?? null,
      pathPattern: p.pathPattern ?? null,
      costCondition: p.costCondition ?? null,
      argsPattern: p.argsPattern ?? null,
      actorPattern: p.actorPattern ?? null,
      message: p.message ?? null,
      enabled: 1,
    });
    return c.json({ policy: created });
  });

  api.post("/policies/reload", (c) => {
    const dir = opts.policiesDir;
    if (!dir) {
      return c.json({ error: "policiesDir not configured" }, 501);
    }
    const out = syncPolicyFiles(c.var.db, dir);
    return c.json(out);
  });

  api.post("/sync/init", (c) => {
    const n = initSyncTables(c.var.db.db);
    return c.json({ tablesInitialized: n });
  });

  app.route("/api", api);
  app.route("/api", createAgentsRouter(opts));
  app.route("/api", createSessionsRouter(opts.getDb));
  app.route("/api", createEventsRouter(opts.getDb));

  const uiDir = opts.uiDistPath;
  const uiAvailable = uiDir && existsSync(uiDir);

  if (uiAvailable && uiDir) {
    app.get("/ui", (c) => c.redirect("/ui/"));

    app.get("/ui/*", async (c) => {
      const url = new URL(c.req.url);
      let relPath = url.pathname.replace(/^\/ui\/?/, "");
      if (!relPath || relPath.endsWith("/")) relPath = "index.html";

      const filePath = join(uiDir, relPath);
      const file = Bun.file(filePath);
      if (!(await file.exists())) {
        const fallback = Bun.file(join(uiDir, "index.html"));
        if (await fallback.exists()) {
          return new Response(fallback, {
            headers: { "Content-Type": "text/html; charset=utf-8" },
          });
        }
        return c.json({ error: "UI not found" }, 404);
      }
      return new Response(file);
    });
  }

  return app;
}

export function serveHttp(port: number, opts: CreateHttpAppOptions): { server: ReturnType<typeof Bun.serve>; port: number } {
  const app = createHttpApp(opts);
  const server = Bun.serve({
    fetch: app.fetch,
    port,
    hostname: "127.0.0.1",
  });
  return { server, port: server.port ?? port };
}
