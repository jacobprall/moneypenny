import { Database } from "bun:sqlite";
import { readdir } from "node:fs/promises";
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

export async function migrate(db: Database, sqlDir: string): Promise<number> {
  db.exec(`
    CREATE TABLE IF NOT EXISTS schema_version (
      version INTEGER NOT NULL,
      applied_at INTEGER NOT NULL DEFAULT (unixepoch())
    )
  `);

  const row = db
    .query<{ version: number }, []>(
      "SELECT COALESCE(MAX(version), 0) as version FROM schema_version",
    )
    .get();
  const current = row?.version ?? 0;

  let sqlFiles: string[];
  try {
    const entries = await readdir(sqlDir);
    sqlFiles = entries.filter((f) => f.endsWith(".sql")).sort();
  } catch {
    sqlFiles = [];
  }

  let applied = 0;
  for (const file of sqlFiles) {
    const version = parseInt(file.split("_")[0], 10);
    if (isNaN(version) || version <= current) continue;

    const sql = await Bun.file(join(sqlDir, file)).text();
    db.transaction(() => {
      db.exec(sql);
      db.exec(`INSERT INTO schema_version (version) VALUES (${version})`);
    })();
    applied++;
  }

  return applied;
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
  const row = db
    .query<{ context: string }, []>("SELECT context FROM v_agent_context")
    .get();
  return row ? JSON.parse(row.context) : null;
}

export function getHealth(db: Database): Record<string, unknown> | null {
  const row = db
    .query<{ health: string }, []>("SELECT health FROM v_health")
    .get();
  return row ? JSON.parse(row.health) : null;
}
