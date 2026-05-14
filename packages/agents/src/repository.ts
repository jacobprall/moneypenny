/**
 * Agents repository — CRUD against the `agents` table.
 */

import type { Database } from "bun:sqlite";

export interface AgentRow {
  id: string;
  dirPath: string;
  agentMdPath: string;
  checksum: string;
  name: string;
  description: string | null;
  schedule: string | null;
  timezone: string | null;
  enabled: number;
  status: string;
  validationErrors: string | null;
  configJson: string;
  prompt: string;
  jobId: string | null;
  lastLoadedAt: number;
  createdAt: number;
  updatedAt: number;
}

export interface UpsertInput {
  id: string;
  dirPath: string;
  agentMdPath: string;
  checksum: string;
  name: string;
  description?: string | null;
  schedule?: string | null;
  timezone?: string | null;
  enabled: number;
  status: string;
  validationErrors?: string | null;
  configJson: string;
  prompt: string;
  jobId?: string | null;
}

const COLS = `id, dir_path as dirPath, agent_md_path as agentMdPath, checksum,
              name, description, schedule, timezone, enabled, status,
              validation_errors as validationErrors, config_json as configJson, prompt,
              job_id as jobId, last_loaded_at as lastLoadedAt,
              created_at as createdAt, updated_at as updatedAt`;

export function getById(db: Database, id: string): AgentRow | null {
  const row = db.query(`SELECT ${COLS} FROM agents WHERE id = ?`).get(id) as AgentRow | undefined;
  return row ?? null;
}

export function list(db: Database): AgentRow[] {
  return db.query(`SELECT ${COLS} FROM agents ORDER BY id ASC`).all() as AgentRow[];
}

export function listActive(db: Database): AgentRow[] {
  return db
    .query(`SELECT ${COLS} FROM agents WHERE status != 'deleted' ORDER BY id ASC`)
    .all() as AgentRow[];
}

export function upsert(db: Database, input: UpsertInput): void {
  const now = Date.now();
  const existing = getById(db, input.id);

  if (!existing) {
    db.run(
      `INSERT INTO agents (
         id, dir_path, agent_md_path, checksum, name, description, schedule, timezone,
         enabled, status, validation_errors, config_json, prompt, job_id,
         last_loaded_at, created_at, updated_at
       ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
      [
        input.id,
        input.dirPath,
        input.agentMdPath,
        input.checksum,
        input.name,
        input.description ?? null,
        input.schedule ?? null,
        input.timezone ?? null,
        input.enabled,
        input.status,
        input.validationErrors ?? null,
        input.configJson,
        input.prompt,
        input.jobId ?? null,
        now,
        now,
        now,
      ],
    );
    return;
  }

  db.run(
    `UPDATE agents SET
       dir_path = ?, agent_md_path = ?, checksum = ?, name = ?, description = ?,
       schedule = ?, timezone = ?, enabled = ?, status = ?, validation_errors = ?,
       config_json = ?, prompt = ?, job_id = ?, last_loaded_at = ?, updated_at = ?
     WHERE id = ?`,
    [
      input.dirPath,
      input.agentMdPath,
      input.checksum,
      input.name,
      input.description ?? null,
      input.schedule ?? null,
      input.timezone ?? null,
      input.enabled,
      input.status,
      input.validationErrors ?? null,
      input.configJson,
      input.prompt,
      input.jobId ?? existing.jobId ?? null,
      now,
      now,
      input.id,
    ],
  );
}

export function setJobId(db: Database, id: string, jobId: string | null): void {
  db.run("UPDATE agents SET job_id = ?, updated_at = ? WHERE id = ?", [jobId, Date.now(), id]);
}

export function setEnabled(db: Database, id: string, enabled: number): void {
  db.run("UPDATE agents SET enabled = ?, updated_at = ? WHERE id = ?", [enabled, Date.now(), id]);
}

export function markDeleted(db: Database, id: string): void {
  db.run("UPDATE agents SET status = 'deleted', enabled = 0, updated_at = ? WHERE id = ?", [Date.now(), id]);
}

export function allKnownIds(db: Database): string[] {
  return (db.query("SELECT id FROM agents WHERE status != 'deleted'").all() as Array<{ id: string }>).map((r) => r.id);
}
