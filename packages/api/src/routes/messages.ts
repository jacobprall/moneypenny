import { Hono } from "hono";
import * as act from "@moneypenny/core";
import type { ActionContext } from "@moneypenny/core";

export function createMessagesRoutes(ctx: ActionContext) {
  return new Hono()
    .get("/", async (c) => {
      const q = c.req.query("q") ?? "";
      return c.json(await act.searchMessages(ctx, q));
    })
    .get("/by-session/:id", async (c) => {
      const cursor = c.req.query("cursor");
      const limitRaw = c.req.query("limit");
      const direction = c.req.query("direction");
      const cursorN = cursor ? Number(cursor) : null;
      const limitN = limitRaw ? Number(limitRaw) : undefined;
      return c.json(
        act.listMessagesBySession(ctx, c.req.param("id"), {
          cursor: cursorN != null && Number.isFinite(cursorN) ? cursorN : null,
          limit: limitN != null && Number.isFinite(limitN) ? Math.min(Math.max(limitN, 1), 500) : undefined,
          direction: direction === "after" ? "after" : "before",
        }),
      );
    });
}
