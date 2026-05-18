import { Hono } from "hono";
import * as act from "@moneypenny/core";
import type { ActionContext } from "@moneypenny/core";

export function createEventsRoutes(ctx: ActionContext) {
  return new Hono()
    .get("/", async (c) => {
      const afterIdRaw = c.req.query("cursor");
      const limitRaw = c.req.query("limit");
      const sessionId = c.req.query("sessionId") ?? undefined;
      const type = c.req.query("type") ?? undefined;
      const afterId = afterIdRaw ? Number(afterIdRaw) : null;
      const limit = limitRaw ? Number(limitRaw) : undefined;
      return c.json(
        act.listEvents(ctx, {
          afterId: afterId != null && Number.isFinite(afterId) ? afterId : null,
          limit: limit != null && Number.isFinite(limit) ? Math.min(Math.max(limit, 1), 500) : undefined,
          sessionId,
          type,
        }),
      );
    });
}
