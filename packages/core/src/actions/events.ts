import type { Event } from "@moneypenny/db";
import type { ActionContext } from "./context.js";

export function listEvents(
  ctx: ActionContext,
  q: {
    afterId?: number | null;
    limit?: number;
    sessionId?: string;
    type?: string;
  },
) {
  const lim = Math.min(q.limit ?? 50, 200);
  let rows: Event[];
  if (q.sessionId && q.afterId != null) {
    rows = ctx.readDb
      .query<Event, [string, number, number]>(
        `SELECT * FROM events WHERE session_id = ? AND id > ? ORDER BY id ASC LIMIT ?`,
      )
      .all(q.sessionId, q.afterId, lim);
  } else if (q.sessionId) {
    rows = ctx.readDb
      .query<Event, [string, number]>(
        `SELECT * FROM events WHERE session_id = ? ORDER BY id DESC LIMIT ?`,
      )
      .all(q.sessionId, lim)
      .reverse();
  } else if (q.type && q.afterId != null) {
    rows = ctx.readDb
      .query<Event, [string, number, number]>(
        `SELECT * FROM events WHERE type = ? AND id > ? ORDER BY id ASC LIMIT ?`,
      )
      .all(q.type, q.afterId, lim);
  } else if (q.afterId != null) {
    rows = ctx.readDb
      .query<Event, [number, number]>(
        `SELECT * FROM events WHERE id > ? ORDER BY id ASC LIMIT ?`,
      )
      .all(q.afterId, lim);
  } else {
    rows = ctx.readDb
      .query<Event, [number]>(`SELECT * FROM events ORDER BY id DESC LIMIT ?`)
      .all(lim)
      .reverse();
  }
  const hasMore = rows.length === lim;
  const nextCursor =
    hasMore && rows.length ? rows[rows.length - 1]!.id : null;
  return { items: rows, nextCursor, hasMore };
}
