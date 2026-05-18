import { Hono } from "hono";
import * as act from "@moneypenny/core";
import type { ActionContext } from "@moneypenny/core";

export function createRunsRoutes(ctx: ActionContext) {
  return new Hono()
    .get("/by-session/:id", async (c) =>
      c.json(act.listRunsBySession(ctx, c.req.param("id"))),
    )
    .get("/:id", async (c) => c.json(act.getRunDetail(ctx, c.req.param("id"))));
}
