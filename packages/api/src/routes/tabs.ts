import { Hono } from "hono";
import { z } from "zod";
import { zValidator } from "@hono/zod-validator";
import * as act from "@moneypenny/core";
import type { ActionContext } from "@moneypenny/core";

const openBody = z.object({
  kind: z.string(),
  sessionId: z.string().nullable().optional(),
  label: z.string().nullable().optional(),
  position: z.number().optional(),
  active: z.boolean().optional(),
});

const patchBody = z.object({
  position: z.number().optional(),
  label: z.string().nullable().optional(),
  active: z.boolean().optional(),
});

export function createTabsRoutes(ctx: ActionContext) {
  return new Hono()
    .get("/", async (c) => c.json(act.listTabs(ctx)))
    .post("/", zValidator("json", openBody), async (c) =>
      c.json(act.openTab(ctx, c.req.valid("json"))),
    )
    .patch("/:id", zValidator("json", patchBody), async (c) => {
      act.patchTab(ctx, { id: c.req.param("id"), ...c.req.valid("json") });
      return c.json({ ok: true });
    })
    .delete("/:id", async (c) => {
      act.closeTab(ctx, c.req.param("id"));
      return c.json({ ok: true });
    });
}
