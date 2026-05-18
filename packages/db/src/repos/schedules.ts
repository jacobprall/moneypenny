import { Database } from "bun:sqlite";
import { randomUUID } from "node:crypto";

export type Schedule = {
  id: string;
  blueprint: string;
  cron_expr: string;
  enabled: number;
  last_run_at: number | null;
  last_session_id: string | null;
  next_run_at: number;
  updated_at: number;
};

export function listDueSchedules(db: Database, nowUnix: number): Schedule[] {
  return db
    .query<Schedule, [number]>(
      `SELECT * FROM schedules WHERE enabled = 1 AND next_run_at <= ? ORDER BY next_run_at ASC`,
    )
    .all(nowUnix);
}

export function upsertSchedule(
  db: Database,
  input: {
    id?: string;
    blueprint: string;
    cron_expr: string;
    enabled?: number;
    next_run_at: number;
  },
): Schedule {
  const id = input.id ?? randomUUID();
  const en = input.enabled ?? 1;
  db.query<
    unknown,
    [string, string, string, number, number]
  >(
    `INSERT INTO schedules (id, blueprint, cron_expr, enabled, next_run_at)
     VALUES (?, ?, ?, ?, ?)
     ON CONFLICT(id) DO UPDATE SET
       blueprint = excluded.blueprint,
       cron_expr = excluded.cron_expr,
       enabled = excluded.enabled,
       next_run_at = excluded.next_run_at,
       updated_at = unixepoch()`,
  ).run(id, input.blueprint, input.cron_expr, en, input.next_run_at);
  return db.query<Schedule, [string]>(`SELECT * FROM schedules WHERE id = ?`).get(
    id,
  )!;
}

export function recordScheduleRun(
  db: Database,
  id: string,
  input: {
    lastSessionId: string | null;
    nextRunAt: number;
    lastRunAt?: number;
  },
): void {
  const lastAt = input.lastRunAt ?? Math.floor(Date.now() / 1000);
  db.query<unknown, [number, string | null, number, string]>(
    `UPDATE schedules SET last_run_at = ?, last_session_id = ?, next_run_at = ?, updated_at = unixepoch()
     WHERE id = ?`,
  ).run(lastAt, input.lastSessionId, input.nextRunAt, id);
}

export function setScheduleEnabled(db: Database, id: string, enabled: boolean): void {
  db.query<unknown, [number, string]>(
    `UPDATE schedules SET enabled = ?, updated_at = unixepoch() WHERE id = ?`,
  ).run(enabled ? 1 : 0, id);
}

export function disableScheduleByBlueprint(db: Database, blueprint: string): void {
  db.query<unknown, [string]>(
    `UPDATE schedules SET enabled = 0, updated_at = unixepoch() WHERE blueprint = ?`,
  ).run(blueprint);
}

export function purgeMissingBlueprints(db: Database, keep: string[]): void {
  if (keep.length === 0) {
    db.exec(`DELETE FROM schedules`);
    return;
  }
  const ph = keep.map(() => "?").join(",");
  db.query(`DELETE FROM schedules WHERE blueprint NOT IN (${ph})`).run(...keep);
}
