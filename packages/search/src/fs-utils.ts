import { statSync, type Stats } from "node:fs";
import type { FileEntry } from "@mp/db/types";

export function tryStat(fullPath: string): Stats | null {
  try {
    return statSync(fullPath);
  } catch {
    return null;
  }
}

export interface FileRow {
  path: string;
  hash: string;
  size: number | null;
  modified_at: number | null;
  language: string | null;
  indexed_at: number | null;
}

export function mapFileRow(r: FileRow): FileEntry {
  return {
    path: r.path,
    hash: r.hash,
    size: r.size,
    modifiedAt: r.modified_at,
    language: r.language,
    indexedAt: r.indexed_at,
  };
}
