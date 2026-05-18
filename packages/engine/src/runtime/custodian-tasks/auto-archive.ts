import type { Database } from "bun:sqlite";
import { listSessions, updateSessionStatus } from "@moneypenny/db";
import type { EventBus } from "../../events/index.js";

export function autoArchiveStaleSessions(
  db: Database,
  events: EventBus,
  afterSeconds: number,
  nowUnix: number,
): void {
  const cutoff = nowUnix - afterSeconds;
  const stmt = db.query<unknown, [string]>(
    `INSERT INTO work_queue (type, session_id, payload) VALUES ('extract', ?, NULL)`,
  );
  for (const s of listSessions(db)) {
    if (s.status !== "completed") continue;
    const at = s.completed_at;
    if (at == null || at > cutoff) continue;
    updateSessionStatus(db, s.id, "archived");
    db.query<unknown, [number, string]>(
      `UPDATE sessions SET archived_at = ? WHERE id = ?`,
    ).run(nowUnix, s.id);
    events.emit({
      type: "session.status_changed",
      session_id: s.id,
      detail: { from: "completed", to: "archived", reason: "auto_archive" },
    });
    stmt.run(s.id);
  }
}
