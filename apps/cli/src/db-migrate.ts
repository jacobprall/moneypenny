import type { Database } from "bun:sqlite";
import { mkdir } from "node:fs/promises";
import {
  migrateV1ToV2,
  migrateV2,
  openAiDb,
  openReadDb,
  openWriteDb,
} from "@moneypenny/db";
import type { ResolvedPaths } from "./paths.js";

export type OpenedDatabases = {
  writeDb: Database;
  readDb: Database;
  aiDb: Database;
};

/** Open/write DB path, migrate v1→v2 when needed, run v2 increments, open read replica + AI conn. */
export async function openDatabasesMigrated(paths: ResolvedPaths): Promise<OpenedDatabases> {
  await mkdir(paths.dataDir, { recursive: true });
  await mkdir(paths.extensionsDir, { recursive: true });
  const writeDb = openWriteDb(paths.dbPath);
  const aiDb = openAiDb(paths.dbPath, paths.extensionsDir);
  await migrateV1ToV2(writeDb, paths.v2SqlDir, paths.dbPath);
  await migrateV2(writeDb, paths.v2SqlDir);
  const readDb = openReadDb(paths.dbPath);
  return { writeDb, readDb, aiDb };
}

/** Migrate without opening read replica (offline migrate subcommand). */
export async function migrateOnly(paths: ResolvedPaths): Promise<number> {
  await mkdir(paths.dataDir, { recursive: true });
  await mkdir(paths.extensionsDir, { recursive: true });
  const writeDb = openWriteDb(paths.dbPath);
  await migrateV1ToV2(writeDb, paths.v2SqlDir, paths.dbPath);
  const { applied } = await migrateV2(writeDb, paths.v2SqlDir);
  writeDb.close();
  return applied;
}
