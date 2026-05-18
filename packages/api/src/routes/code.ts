import { Hono } from "hono";
import * as act from "@moneypenny/core";
import type { ActionContext } from "@moneypenny/core";

export function createCodeRoutes(ctx: ActionContext) {
  return new Hono()
    .get("/search", async (c) => {
      const q = c.req.query("q") ?? "";
      const limitRaw = c.req.query("limit");
      const limit = limitRaw ? Number(limitRaw) : undefined;
      return c.json(
        await act.searchCode(ctx, q, limit != null && Number.isFinite(limit) ? Math.min(limit, 200) : undefined),
      );
    })
    .get("/file", async (c) => {
      const path = c.req.query("path") ?? "";
      const text = await act.readCodeFile(ctx, process.cwd(), path);
      if (text == null) return c.json({ error: { code: "FILE_NOT_FOUND", message: path } }, 404);
      return c.json({ path, content: text });
    })
    .post("/index", async (c) => c.json(act.triggerReindex(ctx)));
}
