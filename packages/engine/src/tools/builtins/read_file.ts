import { readFile } from "node:fs/promises";
import { resolve, relative } from "node:path";
import { z } from "zod";
import type { ToolDef } from "../types.js";

function resolveInCwd(cwd: string, p: string): string {
  const abs = resolve(cwd, p);
  const r = relative(resolve(cwd), abs);
  if (r.startsWith("..") || r === "..") throw new Error("path escapes cwd");
  return abs;
}

export const readFileTool: ToolDef<
  { path: string; start_line?: number; end_line?: number },
  string
> = {
  name: "read_file",
  description: "Read a text file relative to session cwd.",
  category: "fs",
  permissions: { filesystem: "read" },
  inputSchema: z.object({
    path: z.string(),
    start_line: z.number().optional(),
    end_line: z.number().optional(),
  }),
  execute: async ({ path: rel, start_line, end_line }, ctx) => {
    const abs = resolveInCwd(ctx.cwd, rel);
    const raw = await readFile(abs, "utf-8");
    const lines = raw.split("\n");
    const s = (start_line ?? 1) - 1;
    const e = end_line ?? lines.length;
    return lines
      .slice(s, e)
      .map((l, i) => `${s + i + 1}|${l}`)
      .join("\n");
  },
};
