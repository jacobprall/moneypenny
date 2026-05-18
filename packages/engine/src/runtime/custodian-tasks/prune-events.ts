import type { Database } from "bun:sqlite";
import { pruneEventsOlderThan } from "@moneypenny/db";

export function pruneEventsTask(
  db: Database,
  retentionDays: number,
  nowUnix: number,
): number {
  const before = nowUnix - retentionDays * 86_400;
  return pruneEventsOlderThan(db, before);
}
