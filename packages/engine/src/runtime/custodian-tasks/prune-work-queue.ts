import type { Database } from "bun:sqlite";

export function pruneWorkQueueTask(db: Database, nowUnix: number): number {
  const before = nowUnix - 7 * 86_400;
  db.query<unknown, [number]>(
    `DELETE FROM work_queue WHERE processed_at IS NOT NULL AND processed_at < ?`,
  ).run(before);
  return db.query<{ n: number }, []>(`SELECT changes() AS n`).get()?.n ?? 0;
}
