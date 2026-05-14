import { Hono } from "hono";
import { listSessions } from "@mp/db";
import { createRequireDbMiddleware, type HttpVars } from "../middleware.js";
import type { CreateHttpAppOptions } from "../types.js";

export function createSessionsRouter(
  getDb: CreateHttpAppOptions["getDb"],
): Hono<{ Variables: HttpVars }> {
  const r = new Hono<{ Variables: HttpVars }>();
  r.use("*", createRequireDbMiddleware(getDb));

  r.get("/sessions", (c) => {
    const sessions = listSessions(c.var.db);
    return c.json({ sessions });
  });

  r.get("/sessions/:id", (c) => {
    const id = c.req.param("id");
    const row = c.var.db.db
      .query(
        `SELECT id, label, created_at as createdAt, last_active_at as lastActiveAt, is_active as isActive
         FROM sessions WHERE id = ?`,
     )
      .get(id) as
      | {
          id: string;
          label: string | null;
          createdAt: number;
          lastActiveAt: number;
          isActive: number;
        }
      | undefined;
    if (!row) {
      return c.json({ error: "not found" }, 404);
    }
    const turns = c.var.db.db
      .query(`SELECT COUNT(DISTINCT turn) as c FROM messages WHERE session_id = ?`)
      .get(id) as { c: number } | undefined;
    return c.json({ session: row, turns: Number(turns?.c ?? 0) });
  });

  return r;
}
