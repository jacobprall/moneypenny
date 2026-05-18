import type { Context } from "hono";
import { streamSSE } from "hono/streaming";
import type { ActionContext } from "@moneypenny/core";
import type { Event } from "@moneypenny/db";

function replaySession(
  ctx: ActionContext,
  sessionId: string,
  sinceId: number,
): Event[] {
  return ctx.readDb
    .query<Event, [string, number]>(
      `SELECT * FROM events WHERE session_id = ? AND id > ? ORDER BY id ASC`,
    )
    .all(sessionId, sinceId);
}

function replayGlobal(ctx: ActionContext, sinceId: number): Event[] {
  return ctx.readDb
    .query<Event, [number]>(
      `SELECT * FROM events WHERE id > ? ORDER BY id ASC`,
    )
    .all(sinceId);
}

function serializeEvent(e: Event): { id: string; event: string; data: string } {
  let detail: unknown = undefined;
  if (e.detail) {
    try {
      detail = JSON.parse(e.detail);
    } catch {
      detail = e.detail;
    }
  }
  const payload = {
    id: e.id,
    type: e.type,
    session_id: e.session_id,
    run_id: e.run_id,
    blueprint: e.blueprint,
    detail,
    created_at: e.created_at,
  };
  return {
    id: String(e.id),
    event: e.type,
    data: JSON.stringify(payload),
  };
}

export function streamSessionEvents(c: Context, ctx: ActionContext): Response {
  const sessionId = c.req.param("id");
  if (!sessionId) return c.text("missing id", 400);
  const last = c.req.header("Last-Event-ID");
  const sinceId = last ? Number.parseInt(last, 10) || 0 : 0;

  return streamSSE(c, async (stream) => {
    if (sinceId > 0) {
      for (const row of replaySession(ctx, sessionId, sinceId)) {
        const s = serializeEvent(row);
        await stream.writeSSE(s);
      }
    }
    const sub = ctx.events.subscribe({ sessionId });
    try {
      for await (const ev of sub) {
        if (ev.id < 0) continue;
        await stream.writeSSE(serializeEvent(ev as Event));
      }
    } finally {
      sub.close();
    }
  });
}

export function streamGlobalEvents(c: Context, ctx: ActionContext): Response {
  const last = c.req.header("Last-Event-ID");
  const sinceId = last ? Number.parseInt(last, 10) || 0 : 0;

  return streamSSE(c, async (stream) => {
    if (sinceId > 0) {
      for (const row of replayGlobal(ctx, sinceId)) {
        await stream.writeSSE(serializeEvent(row));
      }
    }
    const sub = ctx.events.subscribe();
    try {
      for await (const ev of sub) {
        if (ev.id < 0) continue;
        await stream.writeSSE(serializeEvent(ev as Event));
      }
    } finally {
      sub.close();
    }
  });
}
