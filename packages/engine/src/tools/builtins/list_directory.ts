import { readdir, stat } from "node:fs/promises";
import { join, resolve, relative } from "node:path";
import { z } from "zod";
import type { ToolDef } from "../types.js";

function resolveInCwd(cwd: string, p: string): string {
  const abs = resolve(cwd, p);
  const r = relative(resolve(cwd), abs);
  if (r.startsWith("..") || r === "..") throw new Error("path escapes cwd");
  return abs;
}

export const listDirectoryTool: ToolDef<
  { path?: string },
  { entries: Array<{ name: string; type: "file" | "dir"; size: number }> }
> = {
  name: "list_directory",
  description: "List directory entries with type and size.",
  category: "fs",
  permissions: { filesystem: "read" },
  inputSchema: z.object({ path: z.string().optional().default(".") }),
  execute: async ({ path: rel }, ctx) => {
    const abs = resolveInCwd(ctx.cwd, rel);
    const names = await readdir(abs);
    const entries: Array<{ name: string; type: "file" | "dir"; size: number }> = [];
    for (const n of names) {
      const st = await stat(join(abs, n));
      entries.push({
        name: n,
        type: st.isDirectory() ? "dir" : "file",
        size: st.isFile() ? st.size : 0,
      });
    }
    return { entries };
  },
};
