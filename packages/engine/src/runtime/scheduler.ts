import CronExpressionParser from "cron-parser";
import { getSession, listDueSchedules, recordScheduleRun } from "@moneypenny/db";
import type { SchedulerDeps } from "./types.js";

function nextCronUnix(cronExpr: string, fromUnix: number): number {
  const expr = CronExpressionParser.parse(cronExpr, {
    currentDate: new Date(fromUnix * 1000),
    tz: "UTC",
  });
  const n = expr.next();
  return Math.floor(n.getTime() / 1000);
}

export class Scheduler {
  private handle: ReturnType<typeof setInterval> | undefined;

  constructor(private readonly deps: SchedulerDeps) {}

  start(): void {
    if (this.handle) return;
    this.handle = setInterval(() => void this.tick(), 30_000);
  }

  stop(): void {
    if (this.handle) clearInterval(this.handle);
    this.handle = undefined;
  }

  private async tick(): Promise<void> {
    const now = Math.floor(Date.now() / 1000);
    const due = listDueSchedules(this.deps.readDb, now);
    for (const row of due) {
      if (row.last_session_id) {
        const prev = getSession(this.deps.readDb, row.last_session_id);
        if (prev?.status === "running") {
          this.deps.events.emit({
            type: "schedule.skipped",
            detail: { blueprint: row.blueprint, reason: "prior_session_running" },
          });
          const nxt = nextCronUnix(row.cron_expr, now);
          recordScheduleRun(this.deps.writeDb, row.id, {
            lastSessionId: row.last_session_id,
            nextRunAt: nxt,
            lastRunAt: row.last_run_at ?? now,
          });
          continue;
        }
      }
      let sessionId: string;
      try {
        const launched = await this.deps.launchScheduledAgent({
          blueprint: row.blueprint,
          task: "Scheduled run",
          cwd: this.deps.repoRoot,
          label: row.blueprint,
        });
        sessionId = launched.sessionId;
      } catch (e) {
        this.deps.events.emit({
          type: "schedule.skipped",
          detail: {
            blueprint: row.blueprint,
            reason: String(e),
          },
        });
        continue;
      }
      const nxt = nextCronUnix(row.cron_expr, now);
      recordScheduleRun(this.deps.writeDb, row.id, {
        lastSessionId: sessionId,
        nextRunAt: nxt,
        lastRunAt: now,
      });
      this.deps.events.emit({
        type: "schedule.fired",
        session_id: sessionId,
        detail: { blueprint: row.blueprint, session_id: sessionId },
      });
    }
  }
}
