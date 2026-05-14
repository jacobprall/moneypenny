import { sqlError } from "@mp/db/errors";
import type { AgentDB, FileEntry } from "@mp/db/types";
import { getWorkspaceHandle } from "@mp/db/workspace";
import { mapFileRow, type FileRow } from "./fs-utils";

export function getFileTree(db: AgentDB): FileEntry[] {
  const wsHandle = getWorkspaceHandle(db);
  try {
    const rows = wsHandle
      .prepare(`SELECT path, hash, size, modified_at, language, indexed_at FROM file_tree ORDER BY path`)
      .all() as FileRow[];
    return rows.map(mapFileRow);
  } catch (e) {
    throw sqlError("getFileTree", e);
  }
}

/**
 * Read exclude patterns from a raw database handle.
 * Works with both workspace and session databases.
 */
export function getExcludePatternsFromDb(database: import("bun:sqlite").Database): string[] {
  try {
    const rows = database
      .prepare(`SELECT pattern FROM exclude_patterns ORDER BY pattern`)
      .all() as { pattern: string }[];
    return rows.map((r) => r.pattern);
  } catch {
    return [];
  }
}

/**
 * Read exclude patterns. Checks both the workspace DB (if available) and
 * the session DB, deduplicating results.
 */
export function getExcludePatterns(db: AgentDB): string[] {
  try {
    const wsHandle = getWorkspaceHandle(db);
    const patterns = new Set(getExcludePatternsFromDb(wsHandle));

    if (db.workspace) {
      for (const p of getExcludePatternsFromDb(db.db)) {
        patterns.add(p);
      }
    }

    return [...patterns].sort();
  } catch (e) {
    throw sqlError("getExcludePatterns", e);
  }
}
