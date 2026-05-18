import { Database } from "bun:sqlite";
import { readdir } from "node:fs/promises";
import { join } from "node:path";

const V2_BASE = 100;

function parseV2FileVersion(file: string): number | null {
  const m = /^(\d+)_/.exec(file);
  if (!m) return null;
  const ord = parseInt(m[1], 10);
  if (Number.isNaN(ord)) return null;
  return V2_BASE + ord - 1;
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
    if (Number.isNaN(version) || version <= current) continue;

    const sql = await Bun.file(join(sqlDir, file)).text();
    db.transaction(() => {
      db.exec(sql);
      db.query<unknown, [number]>(`INSERT INTO schema_version (version) VALUES (?)`).run(version);
    })();
    applied++;
  }

  return applied;
}

export async function migrateV2(
  writeDb: Database,
  sqlV2Dir: string,
): Promise<{ applied: number; isFreshV2: boolean }> {
  writeDb.exec(`
    CREATE TABLE IF NOT EXISTS schema_version (
      version INTEGER NOT NULL,
      applied_at INTEGER NOT NULL DEFAULT (unixepoch())
    )
  `);

  const row = writeDb
    .query<{ version: number }, []>(
      "SELECT COALESCE(MAX(version), 0) as version FROM schema_version",
    )
    .get();
  const current = row?.version ?? 0;

  if (current > 0 && current < V2_BASE) {
    throw new Error(
      `Schema version ${current} is a legacy v1 version (expected 0 or >= ${V2_BASE}). ` +
      `Run the v1-to-v2 migration first: mp migrate`,
    );
  }

  const isFreshV2 = current === 0;

  let files: string[];
  try {
    const entries = await readdir(sqlV2Dir);
    files = entries.filter((f) => f.endsWith(".sql")).sort();
  } catch {
    files = [];
  }

  let applied = 0;
  for (const file of files) {
    const version = parseV2FileVersion(file);
    if (version === null || version <= current) continue;

    const sql = await Bun.file(join(sqlV2Dir, file)).text();
    writeDb.transaction(() => {
      writeDb.exec(sql);
      writeDb.query<unknown, [number]>(`INSERT INTO schema_version (version) VALUES (?)`).run(version);
    })();
    applied++;
  }

  return { applied, isFreshV2 };
}
