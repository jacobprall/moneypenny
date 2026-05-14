import type { Database } from "bun:sqlite";

export const TEAM_SCHEMA_SQL = `
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS team_members (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  email TEXT,
  role TEXT NOT NULL DEFAULT 'member',
  joined_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS session_summaries (
  id TEXT PRIMARY KEY,
  member_id TEXT REFERENCES team_members(id),
  title TEXT,
  duration_ms INTEGER,
  cost_usd REAL,
  files_modified INTEGER,
  model TEXT,
  created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS cost_budgets (
  id TEXT PRIMARY KEY,
  scope TEXT NOT NULL,
  limit_usd REAL NOT NULL,
  period TEXT NOT NULL DEFAULT 'weekly',
  current_usd REAL NOT NULL DEFAULT 0,
  reset_at INTEGER NOT NULL,
  created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS audit_log (
  id TEXT PRIMARY KEY,
  member_id TEXT,
  action TEXT NOT NULL,
  details TEXT,
  created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_summaries_member ON session_summaries(member_id);
CREATE INDEX IF NOT EXISTS idx_summaries_created ON session_summaries(created_at);
CREATE INDEX IF NOT EXISTS idx_audit_created ON audit_log(created_at);
`;

export function initTeamDb(db: Database): void {
  db.exec(TEAM_SCHEMA_SQL);
}
