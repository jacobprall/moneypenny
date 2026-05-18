import { z } from "zod";
import type { ToolDef } from "../types.js";

export const requestHumanInputTool: ToolDef<
  { reason: string; options?: string[] },
  string
> = {
  name: "request_human_input",
  description: "Pause for human guidance and surface reason in UI.",
  category: "session",
  permissions: {},
  inputSchema: z.object({
    reason: z.string(),
    options: z.array(z.string()).optional(),
  }),
  execute: async ({ reason, options }, ctx) => {
    ctx.runControl.lastRunPaused = true;
    ctx.events.emit({
      type: "hitl.requested",
      session_id: ctx.sessionId,
      run_id: ctx.runId,
      detail: { reason, options },
    });
    return "requested";
  },
};
