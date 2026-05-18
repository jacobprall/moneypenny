import { Hono } from "hono";
import * as act from "@moneypenny/core";
import type { ActionContext } from "@moneypenny/core";

export function createFilesRoutes(ctx: ActionContext) {
  return new Hono()
    .get("/list", async (c) => {
      const path = c.req.query("path") ?? ".";
      return c.json(await act.listDirectory(ctx, process.cwd(), path));
    })
    .get("/stat", async (c) => {
      const path = c.req.query("path") ?? "";
      return c.json(await act.statFile(ctx, process.cwd(), path));
    })
    .get("/read", async (c) => {
      const path = c.req.query("path") ?? "";
      return c.json({ content: await act.readFileText(ctx, process.cwd(), path) });
    });
}
