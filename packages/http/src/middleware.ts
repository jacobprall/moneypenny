import type { Context, Next, MiddlewareHandler } from "hono";
import type { ZodError } from "zod";
import type { AgentDB } from "@swe/db";

export type HttpVars = {
  db: AgentDB;
};

export function createRequireDbMiddleware(
  getDb: () => AgentDB | null,
): MiddlewareHandler<{ Variables: HttpVars }> {
  return async (c: Context<{ Variables: HttpVars }>, next: Next) => {
    const db = getDb();
    if (!db) {
      return c.json({ error: "Database not ready" }, 503);
    }
    c.set("db", db);
    await next();
  };
}

export function createTokenAuthMiddleware(
  getApiKey: () => string | undefined,
): MiddlewareHandler {
  return async (c: Context, next: Next) => {
    const expected = getApiKey();
    if (!expected) {
      await next();
      return;
    }
    const header = c.req.header("authorization");
    const token = header?.startsWith("Bearer ") ? header.slice(7) : null;
    if (token !== expected) {
      return c.json({ error: "Unauthorized" }, 401);
    }
    await next();
  };
}

export function zodErrorMessage(err: ZodError): string {
  const issues = err.issues;
  if (!issues.length) return "Invalid request body";
  return issues
    .map((i) => {
      const path = Array.isArray(i.path)
        ? i.path.map((p) => (typeof p === "symbol" ? String(p) : String(p))).join(".")
        : "";
      return `${path || "body"}: ${i.message}`;
    })
    .join("; ");
}
