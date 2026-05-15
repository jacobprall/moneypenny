import type { Database, SQLQueryBindings } from "bun:sqlite";

export interface Job {
  id: string;
  name: string;
  description: string | null;
  schedule: string;
  operation: string;
  payload: string | null;
  nextRunAt: number | null;
  lastRunAt: number | null;
  overlapPolicy: string;
  maxRetries: number;
  timeoutMs: number;
  status: string;
  enabled: number;
  createdAt: number;
  updatedAt: number;
}

export interface NewJob {
  id: string;
  name: string;
  description?: string | null;
  schedule: string;
  operation: string;
  payload?: string | null;
  nextRunAt?: number | null;
  overlapPolicy?: string;
  maxRetries?: number;
  timeoutMs?: number;
  status?: string;
  enabled?: number;
  createdAt: number;
  updatedAt: number;
}

export function insert(db: Database, job: NewJob): void {
  db.run(
    `INSERT INTO jobs (id, name, description, schedule, operation, payload, next_run_at, last_run_at, overlap_policy, max_retries, timeout_ms, status, enabled, created_at, updated_at)
     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
    [
      job.id,
      job.name,
      job.description ?? null,
      job.schedule,
      job.operation,
      job.payload ?? null,
      job.nextRunAt ?? null,
      null,
      job.overlapPolicy ?? "skip",
      job.maxRetries ?? 3,
      job.timeoutMs ?? 30000,
      job.status ?? "active",
      job.enabled ?? 1,
      job.createdAt,
      job.updatedAt,
    ],
  );
}

export function findDue(db: Database, now: number): Job[] {
  return db
    .query(
      `SELECT id, name, description, schedule, operation, payload, next_run_at as nextRunAt, last_run_at as lastRunAt,
              overlap_policy as overlapPolicy, max_retries as maxRetries, timeout_ms as timeoutMs, status, enabled, created_at as createdAt, updated_at as updatedAt
       FROM jobs WHERE enabled = 1 AND status = 'active' AND next_run_at IS NOT NULL AND next_run_at <= ? ORDER BY next_run_at ASC`,
    )
    .all(now) as Job[];
}

export function getById(db: Database, id: string): Job | null {
  const row = db
    .query(
      `SELECT id, name, description, schedule, operation, payload, next_run_at as nextRunAt, last_run_at as lastRunAt,
              overlap_policy as overlapPolicy, max_retries as maxRetries, timeout_ms as timeoutMs, status, enabled, created_at as createdAt, updated_at as updatedAt
       FROM jobs WHERE id = ?`,
    )
    .get(id) as Job | undefined;
  return row ?? null;
}

export function getByName(db: Database, name: string): Job | null {
  const row = db
    .query(
      `SELECT id, name, description, schedule, operation, payload, next_run_at as nextRunAt, last_run_at as lastRunAt,
              overlap_policy as overlapPolicy, max_retries as maxRetries, timeout_ms as timeoutMs, status, enabled, created_at as createdAt, updated_at as updatedAt
       FROM jobs WHERE name = ?`,
    )
    .get(name) as Job | undefined;
  return row ?? null;
}

export function listAll(db: Database, operationFilter?: string): Job[] {
  if (operationFilter) {
    return db
      .query(
        `SELECT id, name, description, schedule, operation, payload, next_run_at as nextRunAt, last_run_at as lastRunAt,
                overlap_policy as overlapPolicy, max_retries as maxRetries, timeout_ms as timeoutMs, status, enabled, created_at as createdAt, updated_at as updatedAt
         FROM jobs WHERE operation = ? ORDER BY name ASC`,
      )
      .all(operationFilter) as Job[];
  }
  return db
    .query(
      `SELECT id, name, description, schedule, operation, payload, next_run_at as nextRunAt, last_run_at as lastRunAt,
              overlap_policy as overlapPolicy, max_retries as maxRetries, timeout_ms as timeoutMs, status, enabled, created_at as createdAt, updated_at as updatedAt
       FROM jobs ORDER BY name ASC`,
    )
    .all() as Job[];
}

export function listJobsWithMpFileSource(db: Database): Job[] {
  return db
    .query(
      `SELECT id, name, description, schedule, operation, payload, next_run_at as nextRunAt, last_run_at as lastRunAt,
              overlap_policy as overlapPolicy, max_retries as maxRetries, timeout_ms as timeoutMs, status, enabled, created_at as createdAt, updated_at as updatedAt
       FROM jobs WHERE payload LIKE '%__mp_job_file%' ORDER BY name ASC`,
    )
    .all() as Job[];
}

export function updateJob(
  db: Database,
  id: string,
  patch: {
    name?: string;
    description?: string | null;
    schedule?: string;
    operation?: string;
    payload?: string | null;
    nextRunAt?: number | null;
    overlapPolicy?: string;
    maxRetries?: number;
    timeoutMs?: number;
    status?: string;
    enabled?: number;
  },
): void {
  const sets: string[] = ["updated_at = ?"];
  const vals: unknown[] = [Date.now()];
  if (patch.name !== undefined) {
    sets.push("name = ?");
    vals.push(patch.name);
  }
  if (patch.description !== undefined) {
    sets.push("description = ?");
    vals.push(patch.description);
  }
  if (patch.schedule !== undefined) {
    sets.push("schedule = ?");
    vals.push(patch.schedule);
  }
  if (patch.operation !== undefined) {
    sets.push("operation = ?");
    vals.push(patch.operation);
  }
  if (patch.payload !== undefined) {
    sets.push("payload = ?");
    vals.push(patch.payload);
  }
  if (patch.nextRunAt !== undefined) {
    sets.push("next_run_at = ?");
    vals.push(patch.nextRunAt);
  }
  if (patch.overlapPolicy !== undefined) {
    sets.push("overlap_policy = ?");
    vals.push(patch.overlapPolicy);
  }
  if (patch.maxRetries !== undefined) {
    sets.push("max_retries = ?");
    vals.push(patch.maxRetries);
  }
  if (patch.timeoutMs !== undefined) {
    sets.push("timeout_ms = ?");
    vals.push(patch.timeoutMs);
  }
  if (patch.status !== undefined) {
    sets.push("status = ?");
    vals.push(patch.status);
  }
  if (patch.enabled !== undefined) {
    sets.push("enabled = ?");
    vals.push(patch.enabled);
  }
  vals.push(id);
  db.run(`UPDATE jobs SET ${sets.join(", ")} WHERE id = ?`, vals as SQLQueryBindings[]);
}

export function updateNextRun(db: Database, id: string, nextRunAt: number): void {
  db.run("UPDATE jobs SET next_run_at = ?, updated_at = ? WHERE id = ?", [nextRunAt, Date.now(), id]);
}

export function updateLastRun(db: Database, id: string, lastRunAt: number): void {
  db.run("UPDATE jobs SET last_run_at = ?, updated_at = ? WHERE id = ?", [lastRunAt, Date.now(), id]);
}

export interface JobRun {
  id: string;
  jobId: string;
  startedAt: number;
  endedAt: number | null;
  status: string;
  result: string | null;
  error: string | null;
  retryCount: number;
  createdAt: number;
}

export function insertRun(db: Database, run: JobRun): void {
  db.run(
    `INSERT INTO job_runs (id, job_id, started_at, ended_at, status, result, error, retry_count, created_at)
     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)`,
    [
      run.id,
      run.jobId,
      run.startedAt,
      run.endedAt ?? null,
      run.status,
      run.result ?? null,
      run.error ?? null,
      run.retryCount ?? 0,
      run.createdAt,
    ],
  );
}

export function updateRun(db: Database, id: string, updates: Partial<JobRun>): void {
  const sets: string[] = [];
  const vals: unknown[] = [];
  if (updates.endedAt !== undefined) {
    sets.push("ended_at = ?");
    vals.push(updates.endedAt);
  }
  if (updates.status !== undefined) {
    sets.push("status = ?");
    vals.push(updates.status);
  }
  if (updates.result !== undefined) {
    sets.push("result = ?");
    vals.push(updates.result);
  }
  if (updates.error !== undefined) {
    sets.push("error = ?");
    vals.push(updates.error);
  }
  if (sets.length === 0) return;
  vals.push(id);
  db.run(`UPDATE job_runs SET ${sets.join(", ")} WHERE id = ?`, vals as SQLQueryBindings[]);
}

export function listRunsForJob(db: Database, jobId: string, limit = 100): JobRun[] {
  return db
    .query(
      `SELECT id, job_id as jobId, started_at as startedAt, ended_at as endedAt, status, result, error, retry_count as retryCount, created_at as createdAt
       FROM job_runs WHERE job_id = ? ORDER BY created_at DESC LIMIT ?`,
    )
    .all(jobId, limit) as JobRun[];
}
