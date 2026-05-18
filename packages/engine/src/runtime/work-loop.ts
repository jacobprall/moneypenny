import type { Database } from "bun:sqlite";
import type { EventBus } from "../events/index.js";
import { extractOnArchive } from "./custodian-tasks/extract-on-archive.js";
import { embedPendingTask } from "./custodian-tasks/embed-pending.js";
import type { WorkLoopDeps } from "./types.js";

export class WorkLoop {
  private handle: ReturnType<typeof setInterval> | undefined;
  private ticking = false;

  constructor(private readonly deps: WorkLoopDeps) {}

  start(intervalMs = 15_000): void {
    if (this.handle) return;
    this.handle = setInterval(() => void this.tick(), intervalMs);
  }

  stop(): void {
    if (this.handle) clearInterval(this.handle);
    this.handle = undefined;
  }

  private async tick(): Promise<void> {
    if (this.ticking) return;
    this.ticking = true;
    try {
      await this.drainBatch(this.deps.writeDb, this.deps.events);
    } finally {
      this.ticking = false;
    }
  }

  private async drainBatch(db: Database, events: EventBus): Promise<void> {
    const batch = db
      .query<
        { id: number; type: string; session_id: string | null; payload: string | null },
        [number]
      >(
        `UPDATE work_queue SET processed_at = -1
         WHERE id IN (SELECT id FROM work_queue WHERE processed_at IS NULL ORDER BY id ASC LIMIT ?)
         RETURNING id, type, session_id, payload`,
      )
      .all(this.deps.batchSize);

    for (const row of batch) {
      try {
        await this.processRow(db, events, row);
        db.query<unknown, [number]>(
          `UPDATE work_queue SET processed_at = unixepoch() WHERE id = ?`,
        ).run(row.id);
      } catch (e) {
        db.query<unknown, [string, number]>(
          `UPDATE work_queue SET error = ?, processed_at = unixepoch() WHERE id = ?`,
        ).run(String(e), row.id);
      }
      await new Promise((r) => setTimeout(r, 0));
    }
  }

  private async processRow(
    db: Database,
    events: EventBus,
    row: { id: number; type: string; session_id: string | null; payload: string | null },
  ): Promise<void> {
    if (row.type === "extract" && row.session_id) {
      await extractOnArchive(db, row.session_id, events);
      return;
    }
    if (row.type === "embed") {
      await embedPendingTask(db, 40);
      return;
    }
    if (row.type === "label" || row.type === "summarize") {
      if (row.session_id) await extractOnArchive(db, row.session_id, events);
      return;
    }
    if (row.type === "detect") {
      const { detectConventionsTask } = await import(
        "./custodian-tasks/detect-conventions.js"
      );
      await detectConventionsTask(db, events);
      return;
    }
    if (row.type === "reindex_full") {
      if (this.deps.onFullReindex) await this.deps.onFullReindex();
      events.emit({
        type: "index.completed",
        detail: { files: 0, chunks: 0, duration_ms: 0 },
      });
    }
  }
}
