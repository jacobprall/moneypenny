import { Hono } from "hono";
import * as act from "@moneypenny/core";
import type { ActionContext } from "@moneypenny/core";

export function createEventsRoutes(ctx: ActionContext) {
  return new Hono()
    .get("/", async (c) => {
      const afterId = c.req.query("cursor");
      const limit = c.req.query("limit");
      const sessionId = c.req.query("sessionId") ?? undefined;
      const type = c.req.query("type") ?? undefined;
      return c.json(
        act.listEvents(ctx, {
          afterId: afterId ? Number(afterId) : null,
          limit: limit ? Number(limit) : undefined,
          sessionId,
          type,
        }),
      );
    });
}
