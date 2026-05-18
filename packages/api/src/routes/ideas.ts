import { Hono } from "hono";
import { z } from "zod";
import { zValidator } from "@hono/zod-validator";
import * as act from "@moneypenny/core";
import type { ActionContext } from "@moneypenny/core";

const ideaCreate = z.object({
  filename: z.string(),
  body: z.string(),
  frontmatter: z.record(z.unknown()),
});

const ideaPatch = z.object({
  body: z.string().optional(),
  frontmatter: z.record(z.unknown()),
});

export function createIdeasRoutes(ctx: ActionContext) {
  return new Hono()
    .get("/", async (c) => {
      const status = c.req.query("status") ?? undefined;
      const tags = c.req.query("tags") ?? undefined;
      const cwd = c.req.query("cwd") ?? undefined;
      return c.json(act.listIdeas(ctx, { status, tags, cwd }));
    })
    .get("/:filename{.+}", async (c) =>
      c.json(act.getIdea(ctx, c.req.param("filename"))),
    )
    .post("/", zValidator("json", ideaCreate), async (c) =>
      c.json(await act.createIdea(ctx, c.req.valid("json"))),
    )
    .patch("/:filename{.+}", zValidator("json", ideaPatch), async (c) =>
      c.json(
        await act.updateIdea(ctx, c.req.param("filename"), c.req.valid("json")),
      ),
    )
    .delete("/:filename{.+}", async (c) => {
      await act.deleteIdea(ctx, c.req.param("filename"));
      return c.json({ ok: true });
    });
}
