import { listMessages, searchMessagesFts } from "@moneypenny/db";
import { getSessionRecord } from "./sessions.js";
import type { ActionContext } from "./context.js";

export function searchMessages(ctx: ActionContext, q: string) {
  return searchMessagesFts(ctx.readDb, q);
}

export function listMessagesBySession(
  ctx: ActionContext,
  sessionId: string,
  q: { cursor?: number | null; limit?: number; direction?: "before" | "after" },
) {
  getSessionRecord(ctx, sessionId);
  const lim = Math.min(q.limit ?? 50, 200);
  const items = listMessages(ctx.readDb, {
    sessionId,
    cursorSeq: q.cursor ?? null,
    direction: q.direction ?? "before",
    limit: lim,
  });
  const hasMore = items.length === lim;
  const nextCursor =
    hasMore && items.length ? items[items.length - 1]!.seq : null;
  return { items, nextCursor, hasMore };
}
