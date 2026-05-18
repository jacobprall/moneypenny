import { resolve } from "node:path";
import { z } from "zod";
import type { ToolDef } from "../types.js";

export const changeDirectoryTool: ToolDef<{ cwd: string }, string> = {
  name: "change_directory",
  description: "Update session working directory (repo hook + permission re-eval).",
  category: "session",
  permissions: { filesystem: "readwrite" },
  inputSchema: z.object({ cwd: z.string() }),
  execute: async ({ cwd }, ctx) => {
    const next = resolve(ctx.cwd, cwd);
    const fn = ctx.sessionOps?.setCwd;
    if (!fn) throw new Error("sessionOps.setCwd not configured");
    fn(ctx.sessionId, next);
    ctx.runControl.permissionsNeedReeval = true;
    return next;
  },
};
