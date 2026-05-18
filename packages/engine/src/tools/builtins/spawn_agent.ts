import { z } from "zod";
import type { ToolDef } from "../types.js";

export const spawnAgentTool: ToolDef<
  { blueprint: string; task: string; label?: string; cwd?: string },
  { sessionId: string }
> = {
  name: "spawn_agent",
  description: "Launch a child session from a blueprint via SessionRunner.",
  category: "meta",
  permissions: {},
  inputSchema: z.object({
    blueprint: z.string(),
    task: z.string(),
    label: z.string().optional(),
    cwd: z.string().optional(),
  }),
  execute: (args, ctx) =>
    ctx.runner.launchChild({
      blueprint: args.blueprint,
      task: args.task,
      label: args.label,
      cwd: args.cwd ?? ctx.cwd,
    }),
};
