import { Hono } from "hono";
import * as act from "@moneypenny/core";
import type { ActionContext } from "@moneypenny/core";

export function createFilesRoutes(ctx: ActionContext) {
  return new Hono()
    .get("/list", async (c) => {
      const path = c.req.query("path") ?? ".";
      const cwd = c.req.query("cwd") ?? process.cwd();
      return c.json(await act.listDirectory(ctx, cwd, path));
    })
    .get("/stat", async (c) => {
      const path = c.req.query("path") ?? "";
      const cwd = c.req.query("cwd") ?? process.cwd();
      return c.json(await act.statFile(ctx, cwd, path));
    })
    .get("/read", async (c) => {
      const path = c.req.query("path") ?? "";
      const cwd = c.req.query("cwd") ?? process.cwd();
      return c.json({ content: await act.readFileText(ctx, cwd, path) });
    });
}
