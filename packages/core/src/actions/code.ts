import { hybridSearch } from "@moneypenny/engine";
import type { ActionContext } from "./context.js";
import { readFile } from "node:fs/promises";
import { join, resolve, relative } from "node:path";
import { existsSync } from "node:fs";
import { ErrorCodes, MoneypennyError } from "../errors.js";

function guardPath(cwd: string, rel: string): string {
  const p = resolve(join(cwd, rel));
  const r = relative(resolve(cwd), p);
  if (r.startsWith("..") || r.includes("..")) {
    throw new MoneypennyError(ErrorCodes.PERMISSION_DENIED, "path escape");
  }
  return p;
}

export async function searchCode(ctx: ActionContext, q: string, limit?: number) {
  return hybridSearch(ctx.readDb, q, limit ?? 20);
}

export async function readCodeFile(ctx: ActionContext, cwd: string, rel: string) {
  const path = guardPath(cwd, rel);
  if (!existsSync(path)) return null;
  return readFile(path, "utf8");
}

export function triggerReindex(ctx: ActionContext): { queued: boolean } {
  ctx.writeDb
    .query(`INSERT INTO work_queue (type, payload) VALUES ('reindex_full', NULL)`)
    .run();
  return { queued: true };
}
