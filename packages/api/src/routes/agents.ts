import { Hono } from "hono";
import { z } from "zod";
import { zValidator } from "@hono/zod-validator";
import * as act from "@moneypenny/core";
import type { ActionContext } from "@moneypenny/core";

const launch = z.object({
  blueprint: z.string().optional(),
  task: z.string().optional(),
  cwd: z.string().optional(),
  label: z.string().optional(),
  idea_id: z.string().optional(),
});

export function createAgentsRoutes(ctx: ActionContext) {
  return new Hono()
    .post("/launch", zValidator("json", launch), async (c) => {
      const session = await act.launchAgent(ctx, c.req.valid("json"));
      return c.json({ session });
    })
    .get("/status", async (c) => c.json(act.getAgentStatus(ctx)))
    .post("/kill/:sessionId", async (c) => {
      await act.killAgent(ctx, c.req.param("sessionId"));
      return c.json({ ok: true });
    });
}
