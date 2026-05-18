import { Database } from "bun:sqlite";
import { randomUUID } from "node:crypto";

export type Run = {
  id: string;
  session_id: string;
  status: string;
  model: string | null;
  blueprint: string | null;
  started_at: number;
  finished_at: number | null;
  tokens_in: number | null;
  tokens_out: number | null;
  cost_usd: number | null;
  error: string | null;
};

export function startRun(
  db: Database,
  input: { sessionId: string; model?: string | null; blueprint?: string | null },
): Run {
  const id = randomUUID();
  db.query<unknown, [string, string, string | null, string | null]>(
    `INSERT INTO runs (id, session_id, status, model, blueprint)
     VALUES (?, ?, 'running', ?, ?)`,
  ).run(id, input.sessionId, input.model ?? null, input.blueprint ?? null);
  return getRun(db, id)!;
}

export function finishRun(
  db: Database,
  id: string,
  totals: {
    tokensIn?: number | null;
    tokensOut?: number | null;
    costUsd?: number | null;
  },
): void {
  db.query<unknown, [number | null, number | null, number | null, string]>(
    `UPDATE runs SET status = 'complete', finished_at = unixepoch(),
         tokens_in = ?, tokens_out = ?, cost_usd = ?
     WHERE id = ?`,
  ).run(
    totals.tokensIn ?? null,
    totals.tokensOut ?? null,
    totals.costUsd ?? null,
    id,
  );
}

export function failRun(db: Database, id: string, error: string): void {
  db.query<unknown, [string, string]>(
    `UPDATE runs SET status = 'failed', finished_at = unixepoch(), error = ? WHERE id = ?`,
  ).run(error, id);
}

export function abortRun(db: Database, id: string, reason?: string): void {
  db.query<unknown, [string | null, string]>(
    `UPDATE runs SET status = 'aborted', finished_at = unixepoch(), error = ? WHERE id = ?`,
  ).run(reason ?? null, id);
}

export function listRuns(db: Database, sessionId: string): Run[] {
  return db
    .query<Run, [string]>(
      `SELECT * FROM runs WHERE session_id = ? ORDER BY started_at DESC`,
    )
    .all(sessionId);
}

export function getRun(db: Database, id: string): Run | null {
  return db.query<Run, [string]>(`SELECT * FROM runs WHERE id = ?`).get(id) ?? null;
}
