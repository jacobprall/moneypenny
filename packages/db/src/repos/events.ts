import { Database } from "bun:sqlite";

export type Event = {
  id: number;
  type: string;
  session_id: string | null;
  run_id: string | null;
  blueprint: string | null;
  detail: string | null;
  created_at: number;
};

export function insertEvent(
  db: Database,
  input: Omit<Event, "id" | "created_at">,
): Event {
  db.query<
    unknown,
    [string, string | null, string | null, string | null, string | null]
  >(
    `INSERT INTO events (type, session_id, run_id, blueprint, detail)
     VALUES (?, ?, ?, ?, ?)`,
  ).run(
    input.type,
    input.session_id,
    input.run_id,
    input.blueprint,
    input.detail,
  );
  const row = db
    .query<{ id: number }, []>(`SELECT last_insert_rowid() AS id`)
    .get();
  const id = Number(row?.id ?? 0);
  return db.query<Event, [number]>(`SELECT * FROM events WHERE id = ?`).get(id)!;
}

export function listEvents(
  db: Database,
  input: { afterId?: number | null; limit: number },
): Event[] {
  const lim = Math.min(Math.max(input.limit, 1), 1000);
  if (input.afterId == null) {
    return db
      .query<Event, [number]>(`SELECT * FROM events ORDER BY id DESC LIMIT ?`)
      .all(lim)
      .reverse();
  }
  return db
    .query<Event, [number, number]>(
      `SELECT * FROM events WHERE id > ? ORDER BY id ASC LIMIT ?`,
    )
    .all(input.afterId, lim);
}

export function eventsForSession(
  db: Database,
  sessionId: string,
  limit = 200,
): Event[] {
  const lim = Math.min(Math.max(limit, 1), 1000);
  return db
    .query<Event, [string, number]>(
      `SELECT * FROM events WHERE session_id = ? ORDER BY id DESC LIMIT ?`,
    )
    .all(sessionId, lim)
    .reverse();
}

export function pruneEventsOlderThan(db: Database, beforeUnix: number): number {
  db.query<unknown, [number]>(`DELETE FROM events WHERE created_at < ?`).run(
    beforeUnix,
  );
  return db.query<{ n: number }, []>(`SELECT changes() AS n`).get()?.n ?? 0;
}
