import { readdir, stat, readFile as fsRead } from "node:fs/promises";
import { join, resolve, relative, basename } from "node:path";
import { existsSync } from "node:fs";
import type { Database } from "bun:sqlite";
import { ErrorCodes, MoneypennyError } from "../errors.js";
import type { ActionContext } from "./context.js";

const BINARY_EXT =
  /\.(png|jpe?g|gif|webp|ico|pdf|zip|gz|tar|wasm|so|dylib|dll|exe|mp3|mp4)$/i;

function guardPath(cwd: string, rel: string): string {
  const path = resolve(join(cwd, rel));
  const r = relative(resolve(cwd), path);
  if (r.startsWith("..") || r.includes("..")) {
    throw new MoneypennyError(ErrorCodes.PERMISSION_DENIED, "path escape");
  }
  return path;
}

type FileTreeRow = { path: string; is_dir: number };

function listFromFileTree(
  db: Database,
  dirPath: string,
): { name: string; isDir: boolean }[] | null {
  const prefix = dirPath.endsWith("/") ? dirPath : `${dirPath}/`;
  const rows = db
    .query<FileTreeRow, [string, string]>(
      `SELECT path, is_dir FROM file_tree
       WHERE path LIKE ? AND path NOT LIKE ?
       ORDER BY is_dir DESC, path ASC`,
    )
    .all(`${prefix}%`, `${prefix}%/%`);
  if (rows.length === 0) return null;
  return rows.map((r) => ({
    name: basename(r.path),
    isDir: r.is_dir === 1,
  }));
}

export async function listDirectory(ctx: ActionContext, cwd: string, rel: string) {
  const path = guardPath(cwd, rel);
  const indexed = listFromFileTree(ctx.readDb, path);
  if (indexed) return indexed;
  const entries = await readdir(path, { withFileTypes: true });
  return entries.map((e) => ({ name: e.name, isDir: e.isDirectory() }));
}

export async function statFile(ctx: ActionContext, cwd: string, rel: string) {
  const path = guardPath(cwd, rel);
  if (!existsSync(path))
    throw new MoneypennyError(ErrorCodes.FILE_NOT_FOUND, rel);
  const s = await stat(path);
  return {
    path: rel,
    size: s.size,
    isDir: s.isDirectory(),
    mtimeMs: s.mtimeMs,
  };
}

export async function readFileText(ctx: ActionContext, cwd: string, rel: string) {
  if (BINARY_EXT.test(rel)) {
    throw new MoneypennyError(ErrorCodes.BINARY_FILE_REJECTED, rel);
  }
  const path = guardPath(cwd, rel);
  if (!existsSync(path))
    throw new MoneypennyError(ErrorCodes.FILE_NOT_FOUND, rel);
  return fsRead(path, "utf8");
}
