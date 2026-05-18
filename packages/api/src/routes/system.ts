import { Hono } from "hono";
import { z } from "zod";
import { zValidator } from "@hono/zod-validator";
import * as act from "@moneypenny/core";
import type { ActionContext } from "@moneypenny/core";

const configPatch = z.object({
  kv: z.record(z.string()),
});

export function createSystemRoutes(ctx: ActionContext) {
  return new Hono()
    .get("/health", async (c) => c.json(act.getHealth(ctx)))
    .get("/config", async (c) => c.json(act.getSystemConfig(ctx)))
    .patch("/config", zValidator("json", configPatch), async (c) => {
      act.setSystemConfig(ctx, c.req.valid("json").kv);
      return c.json({ ok: true });
    });
}
