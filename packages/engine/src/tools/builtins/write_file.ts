import { mkdir, writeFile } from "node:fs/promises";
import { dirname, resolve, relative } from "node:path";
import { z } from "zod";
import type { ToolDef } from "../types.js";

function resolveInCwd(cwd: string, p: string): string {
  const abs = resolve(cwd, p);
  const r = relative(resolve(cwd), abs);
  if (r.startsWith("..") || r === "..") throw new Error("path escapes cwd");
  return abs;
}

export const writeFileTool: ToolDef<{ path: string; content: string }, string> = {
  name: "write_file",
  description: "Create or overwrite a file (mkdir -p parents). Cwd-relative.",
  category: "fs",
  permissions: { filesystem: "readwrite" },
  inputSchema: z.object({ path: z.string(), content: z.string() }),
  execute: async ({ path: rel, content }, ctx) => {
    const abs = resolveInCwd(ctx.cwd, rel);
    await mkdir(dirname(abs), { recursive: true });
    await writeFile(abs, content, "utf-8");
    return `wrote ${rel}`;
  },
};
