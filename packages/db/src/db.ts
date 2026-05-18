import { Database } from "bun:sqlite";
import { join } from "node:path";

export interface DbConfig {
  path: string;
  sqlDir: string;
  extensionsDir?: string;
}

export function openDb(config: DbConfig): Database {
  const db = new Database(config.path, { create: true });
  db.exec("PRAGMA journal_mode = WAL");
  db.exec("PRAGMA foreign_keys = ON");
  db.exec("PRAGMA busy_timeout = 5000");
  return db;
}

/**
 * Opens a dedicated SQLite connection for AI inference (sqlite-ai).
 * Isolating this connection keeps slow local generations off the main
 * write path. The same physical DB file is used; WAL gives readers
 * snapshot isolation and writers serialize harmlessly.
 */
export function openAiDb(path: string, extensionsDir?: string): Database {
  const db = new Database(path, { create: true });
  db.exec("PRAGMA journal_mode = WAL");
  db.exec("PRAGMA busy_timeout = 5000");
  if (extensionsDir) {
    try { loadExtensions(db, extensionsDir); } catch {}
  }
  return db;
}

export function loadExtensions(db: Database, dir: string): void {
  const extensions = ["sqlite_ai", "sqlite_vector"];
  for (const ext of extensions) {
    const path = join(dir, ext);
    try {
      db.loadExtension(path);
    } catch {}
  }
}

export function getContext(db: Database): Record<string, unknown> | null {
  try {
    const row = db
      .query<{ context: string }, []>("SELECT context FROM v_agent_context")
      .get();
    return row ? JSON.parse(row.context) : null;
  } catch {
    return null;
  }
}

export function getHealth(db: Database): Record<string, unknown> | null {
  const row = db
    .query<{ health: string }, []>("SELECT health FROM v_health")
    .get();
  return row ? JSON.parse(row.health) : null;
}
