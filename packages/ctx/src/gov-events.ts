import type { Database, SQLQueryBindings } from "bun:sqlite";

export interface NewEvent {
  id: string;
  operation: string;
  actor: string;
  sessionId?: string;
  input: unknown;
  output?: unknown;
  error?: string;
  durationMs?: number;
  createdAt: number;
}

export function append(db: Database, event: NewEvent): void {
  db.run(
    `INSERT INTO gov_events (id, operation, actor, session_id, input, output, error, duration_ms, created_at)
     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)`,
    [
      event.id,
      event.operation,
      event.actor,
      event.sessionId ?? null,
      JSON.stringify(event.input),
      event.output !== undefined ? JSON.stringify(event.output) : null,
      event.error ?? null,
      event.durationMs ?? null,
      event.createdAt,
    ]
  );
}

export interface EventFilters {
  operation?: string;
  actor?: string;
  sessionId?: string;
  limit?: number;
}

export interface Event {
  id: string;
  operation: string;
  actor: string;
  sessionId: string | null;
  input: string;
  output: string | null;
  error: string | null;
  durationMs: number | null;
  createdAt: number;
}

export function query(db: Database, filters: EventFilters = {}): Event[] {
  const conditions: string[] = [];
  const params: unknown[] = [];

  if (filters.operation) {
    conditions.push("operation = ?");
    params.push(filters.operation);
  }
  if (filters.actor) {
    conditions.push("actor = ?");
    params.push(filters.actor);
  }
  if (filters.sessionId) {
    conditions.push("session_id = ?");
    params.push(filters.sessionId);
  }

  const where = conditions.length > 0 ? "WHERE " + conditions.join(" AND ") : "";
  const limit = filters.limit ?? 100;
  params.push(limit);
  const sql = `SELECT id, operation, actor, session_id as sessionId, input, output, error, duration_ms as durationMs, created_at as createdAt
               FROM gov_events ${where} ORDER BY created_at DESC LIMIT ?`;
  const rows = db.query(sql).all(...(params as SQLQueryBindings[])) as Event[];
  return rows;
}
