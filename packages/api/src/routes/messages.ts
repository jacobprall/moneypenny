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
      const limit = c.req.query("limit");
      const direction = c.req.query("direction");
      return c.json(
        act.listMessagesBySession(ctx, c.req.param("id"), {
          cursor: cursor ? Number(cursor) : null,
          limit: limit ? Number(limit) : undefined,
          direction: direction === "after" ? "after" : "before",
        }),
      );
    });
}
