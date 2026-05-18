import { readFile, writeFile } from "node:fs/promises";
import { resolve, relative } from "node:path";
import { z } from "zod";
import type { ToolDef } from "../types.js";

function resolveInCwd(cwd: string, p: string): string {
  const abs = resolve(cwd, p);
  const r = relative(resolve(cwd), abs);
  if (r.startsWith("..") || r === "..") throw new Error("path escapes cwd");
  return abs;
}

export const editFileTool: ToolDef<
  { path: string; find: string; replace: string; replaceAll?: boolean },
  string
> = {
  name: "edit_file",
  description: "Apply literal string find/replace to a file (not regex).",
  category: "fs",
  permissions: { filesystem: "readwrite" },
  inputSchema: z.object({
    path: z.string(),
    find: z.string(),
    replace: z.string(),
    replaceAll: z.boolean().optional(),
  }),
  execute: async ({ path: rel, find, replace, replaceAll }, ctx) => {
    const abs = resolveInCwd(ctx.cwd, rel);
    let text = await readFile(abs, "utf-8");
    const count = replaceAll
      ? text.split(find).length - 1
      : text.includes(find)
        ? 1
        : 0;
    text = replaceAll ? text.split(find).join(replace) : text.replace(find, replace);
    await writeFile(abs, text, "utf-8");
    return `ok (${count} replacement(s))`;
  },
};
