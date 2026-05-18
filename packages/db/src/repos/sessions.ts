import { Database } from "bun:sqlite";
import { randomUUID } from "node:crypto";

export type Session = {
  id: string;
  label: string | null;
  status: string;
  parent_id: string | null;
  idea_id: string | null;
  config: string;
  config_version: number;
  created_at: number;
  last_active_at: number;
  completed_at: number | null;
  failed_at: number | null;
  archived_at: number | null;
};

export function getSession(db: Database, id: string): Session | null {
  return (
    db
      .query<
        Session,
        [string]
      >(`SELECT * FROM sessions WHERE id = ?`)
      .get(id) ?? null
  );
}

export function listSessions(db: Database): Session[] {
  return db
    .query<Session, []>(
      `SELECT * FROM sessions ORDER BY last_active_at DESC`,
    )
    .all();
}

export function createSession(
  db: Database,
  input: {
    label?: string | null;
    parentId?: string | null;
    ideaId?: string | null;
    config?: string;
  },
): Session {
  const id = randomUUID();
  const config = input.config ?? "{}";
  db.query<
    unknown,
    [string, string | null, string | null, string | null, string]
  >(
    `INSERT INTO sessions (id, label, parent_id, idea_id, config)
     VALUES (?, ?, ?, ?, ?)`,
  ).run(
    id,
    input.label ?? null,
    input.parentId ?? null,
    input.ideaId ?? null,
    config,
  );
  return getSession(db, id)!;
}

export function updateSessionStatus(
  db: Database,
  id: string,
  status: string,
): void {
  db.query<unknown, [string, string]>(
    `UPDATE sessions SET status = ?, last_active_at = unixepoch() WHERE id = ?`,
  ).run(status, id);
}

export function updateSessionConfigOptimistic(
  db: Database,
  id: string,
  config: string,
  expectedVersion: number,
): { ok: boolean; newVersion?: number } {
  db.query<unknown, [string, string, number]>(
    `UPDATE sessions
     SET config = ?, config_version = config_version + 1, last_active_at = unixepoch()
     WHERE id = ? AND config_version = ?`,
  ).run(config, id, expectedVersion);
  const n = db.query<{ n: number }, []>(`SELECT changes() AS n`).get()?.n ?? 0;
  if (n === 0) return { ok: false };
  const row = db
    .query<{ config_version: number }, [string]>(
      `SELECT config_version FROM sessions WHERE id = ?`,
    )
    .get(id);
  return { ok: true, newVersion: row?.config_version };
}

export function touchSession(db: Database, id: string): void {
  db.query<unknown, [string]>(
    `UPDATE sessions SET last_active_at = unixepoch() WHERE id = ?`,
  ).run(id);
}

export function deleteSession(db: Database, id: string): void {
  db.query<unknown, [string]>(`DELETE FROM sessions WHERE id = ?`).run(id);
}

export function searchSessionsByLabel(db: Database, ftsQuery: string): Session[] {
  return db
    .query<Session, [string]>(
      `SELECT s.* FROM sessions s
       JOIN sessions_fts ON sessions_fts.rowid = s.rowid
       WHERE sessions_fts MATCH ?
       ORDER BY s.last_active_at DESC`,
    )
    .all(ftsQuery);
}
