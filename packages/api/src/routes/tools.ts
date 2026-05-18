import { Hono } from "hono";
import * as act from "@moneypenny/core";
import type { ActionContext } from "@moneypenny/core";

export function createToolsRoutes(ctx: ActionContext) {
  return new Hono()
    .get("/", async (c) => c.json(act.listTools(ctx)));
}
