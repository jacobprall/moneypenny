import { Database } from "bun:sqlite";
import { openAiDb as openAiDbFromDb } from "./db.js";

export function openWriteDb(path: string): Database {
  const db = new Database(path, { create: true });
  db.exec("PRAGMA journal_mode = WAL");
  db.exec("PRAGMA foreign_keys = ON");
  db.exec("PRAGMA busy_timeout = 5000");
  db.exec("PRAGMA synchronous = NORMAL");
  return db;
}

export function openReadDb(path: string): Database {
  const db = new Database(path, { readonly: true });
  db.exec("PRAGMA journal_mode = WAL");
  db.exec("PRAGMA foreign_keys = ON");
  db.exec("PRAGMA busy_timeout = 5000");
  return db;
}

export function openAiDb(path: string, extensionsDir?: string): Database {
  return openAiDbFromDb(path, extensionsDir);
}
