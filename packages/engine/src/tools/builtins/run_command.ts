import { z } from "zod";
import type { ToolDef } from "../types.js";

export const runCommandTool: ToolDef<
  { cmd: string; timeout_ms?: number },
  { exit_code: number; stdout: string; stderr: string }
> = {
  name: "run_command",
  description: "Run a shell command in session cwd (Bun.spawn, abort-aware).",
  category: "shell",
  permissions: { shell: true },
  inputSchema: z.object({
    cmd: z.string(),
    timeout_ms: z.number().optional().default(60_000),
  }),
  execute: async ({ cmd, timeout_ms }, ctx) => {
    const ctl = new AbortController();
    const t = setTimeout(() => ctl.abort(), timeout_ms);
    const merged = AbortSignal.any([ctx.abortSignal, ctl.signal]);
    try {
      const proc = Bun.spawn(["sh", "-c", cmd], {
        cwd: ctx.cwd,
        signal: merged,
        stdout: "pipe",
        stderr: "pipe",
      });
      const [stdout, stderr] = await Promise.all([
        new Response(proc.stdout).text(),
        new Response(proc.stderr).text(),
      ]);
      const exit_code = await proc.exited;
      return {
        exit_code,
        stdout: stdout.slice(0, 50_000),
        stderr: stderr.slice(0, 20_000),
      };
    } finally {
      clearTimeout(t);
    }
  },
};
