import type { Database } from "bun:sqlite";
import { autoArchiveStaleSessions } from "./custodian-tasks/auto-archive.js";
import { maybeCompactRunningSession } from "./custodian-tasks/compact-session.js";
import { detectConventionsTask } from "./custodian-tasks/detect-conventions.js";
import { embedPendingTask } from "./custodian-tasks/embed-pending.js";
import { pruneEventsTask } from "./custodian-tasks/prune-events.js";
import { pruneWorkQueueTask } from "./custodian-tasks/prune-work-queue.js";
import type { CustodianDeps } from "./types.js";

export class Custodian {
  private handle: ReturnType<typeof setInterval> | undefined;
  private lastHourly = 0;
  private lastDaily = 0;
  private last5 = 0;
  private lastConventionDay = -1;

  constructor(private readonly deps: CustodianDeps) {}

  queueExtract(sessionId: string): void {
    this.deps.writeDb
      .query(`INSERT INTO work_queue (type, session_id) VALUES ('extract', ?)`)
      .run(sessionId);
  }

  start(): void {
    if (this.handle) return;
    this.handle = setInterval(() => void this.tick(), 60_000);
  }

  stop(): void {
    if (this.handle) clearInterval(this.handle);
    this.handle = undefined;
  }

  private async tick(): Promise<void> {
    const now = Math.floor(Date.now() / 1000);
    const dayKey = Math.floor(now / 86_400);
    if (dayKey !== this.lastDaily) {
      this.lastDaily = dayKey;
      this.safe(() => pruneEventsTask(this.deps.writeDb, this.deps.eventRetentionDays, now));
      this.safe(() => pruneWorkQueueTask(this.deps.writeDb, now));
    }

    const hourKey = Math.floor(now / 3600);
    if (hourKey !== this.lastHourly) {
      this.lastHourly = hourKey;
      this.safe(() =>
        autoArchiveStaleSessions(this.deps.writeDb, this.deps.events, this.deps.archiveAfterDays * 86_400, now),
      );
    }

    const fiveKey = Math.floor(now / 300);
    if (fiveKey !== this.last5) {
      this.last5 = fiveKey;
      await this.safeAsync(() => embedPendingTask(this.deps.writeDb, 80));
    }

    const d = new Date(now * 1000);
    const day = Math.floor(now / 86_400);
    if (d.getUTCHours() === 3 && day !== this.lastConventionDay) {
      this.lastConventionDay = day;
      await this.safeAsync(() => detectConventionsTask(this.deps.writeDb, this.deps.events));
    }

    const running = this.deps.readDb
      .query<{ id: string }, []>(
        `SELECT id FROM sessions WHERE status = 'running' LIMIT 20`,
      )
      .all();
    for (const r of running) {
      await this.safeAsync(() =>
        maybeCompactRunningSession(this.deps.writeDb, r.id, this.deps.compactMessageThreshold),
      );
    }
  }

  private safe(fn: () => void): void {
    try { fn(); } catch (err) {
      console.error("[custodian] task error:", err instanceof Error ? err.message : err);
    }
  }

  private async safeAsync(fn: () => Promise<unknown>): Promise<void> {
    try { await fn(); } catch (err) {
      console.error("[custodian] task error:", err instanceof Error ? err.message : err);
    }
  }
}
