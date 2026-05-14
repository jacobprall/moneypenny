import { Hono } from "hono";
import { streamSSE } from "hono/streaming";
import { z } from "zod";
import { appendEvent, getEvents } from "@mp/db";
import { createRequireDbMiddleware, zodErrorMessage, type HttpVars } from "../middleware.js";
import type { CreateHttpAppOptions } from "../types.js";

const AppendBody = z.object({
  type: z.string().min(1),
  payload: z.record(z.unknown()).default({}),
  turn: z.number().optional(),
});

export function createEventsRouter(getDb: CreateHttpAppOptions["getDb"]): Hono<{ Variables: HttpVars }> {
  const r = new Hono<{ Variables: HttpVars }>();
  r.use("*", createRequireDbMiddleware(getDb));

  r.get("/events", async (c) => {
    const db = c.var.db;
    const follow = c.req.query("follow") === "1";
    const limit = Math.min(500, Math.max(1, parseInt(c.req.query("limit") ?? "100", 10)));
    const type = c.req.query("type");
    const sessionId = c.req.query("sessionId");

    if (follow) {
      return streamSSE(c, async (stream) => {
        let cursor = 0;
        try {
          while (true) {
            const evs = getEvents(db, {
              limit,
              sessionId: sessionId ?? undefined,
              type: type ?? undefined,
              offset: cursor,
            });
            for (const e of evs) {
              await stream.writeSSE({ data: JSON.stringify(e), id: e.id });
              cursor++;
            }
            await new Promise((r) => setTimeout(r, 1500));
          }
        } catch {
          /* client disconnect */
        }
      });
    }

    const events = getEvents(db, {
      limit,
      type: type ?? undefined,
      sessionId: sessionId ?? undefined,
    });
    return c.json({ events });
  });

  r.post("/events", async (c) => {
    let body: unknown;
    try {
      body = await c.req.json();
    } catch {
      return c.json({ error: "Invalid JSON" }, 400);
    }
    const parsed = AppendBody.safeParse(body);
    if (!parsed.success) {
      return c.json({ error: zodErrorMessage(parsed.error) }, 400);
    }
    const ev = appendEvent(c.var.db, {
      type: parsed.data.type,
      payload: parsed.data.payload,
      turn: parsed.data.turn,
    });
    return c.json(ev);
  });

  return r;
}
