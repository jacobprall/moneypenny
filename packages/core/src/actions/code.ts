import { hybridSearch } from "@moneypenny/engine";
import type { ActionContext } from "./context.js";
import { readFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { existsSync } from "node:fs";

export async function searchCode(ctx: ActionContext, q: string, limit?: number) {
  return hybridSearch(ctx.readDb, q, limit ?? 20);
}

export async function readCodeFile(ctx: ActionContext, cwd: string, rel: string) {
  const path = resolve(join(cwd, rel));
  if (!existsSync(path)) return null;
  return readFile(path, "utf8");
}

export function triggerReindex(ctx: ActionContext): { queued: boolean } {
  ctx.writeDb
    .query(`INSERT INTO work_queue (type, payload) VALUES ('reindex_full', NULL)`)
    .run();
  return { queued: true };
}
