import type { Database } from "bun:sqlite";
import { withBusyRetry, withImmediateTransaction } from "./busy-retry.js";

const MAX_DEFERRED_BATCH = 20;

/**
 * Serialized access to the primary SQLite handle plus deferred (batched) writes.
 * - `exclusive` — read-your-writes path; flushes pending deferred work first (ordering).
 * - `defer` — enqueue work run inside a single transaction on the next flush.
 */
export class DbWriter {
  private readonly deferred: Array<(db: Database) => void> = [];
  private flushTimer: ReturnType<typeof setTimeout> | null = null;
  private readonly flushDelayMs: number;
  private closed = false;
  private exclusiveDepth = 0;

  constructor(
    readonly db: Database,
    opts?: { flushDelayMs?: number },
  ) {
    this.flushDelayMs = opts?.flushDelayMs ?? 50;
  }

  /**
   * Run fn on the writer connection. Flushes deferred queue first so prior deferred
   * events/metrics land before this mutation (timeline ordering).
   */
  exclusive<T>(fn: (db: Database) => T): T {
    if (this.closed) throw new Error("DbWriter is closed");
    if (this.exclusiveDepth === 0) {
      this.cancelScheduledFlush();
      this.flushDeferredSync();
    }
    this.exclusiveDepth++;
    try {
      return withBusyRetry(() => fn(this.db));
    } finally {
      this.exclusiveDepth--;
      if (this.exclusiveDepth === 0 && this.deferred.length > 0) {
        if (this.deferred.length >= MAX_DEFERRED_BATCH) {
          this.flushDeferredSync();
        } else {
          this.scheduleFlush();
        }
      }
    }
  }

  /** Queue a deferred mutation (appendEvent, recordTurnMetrics, cache, …). */
  defer(fn: (db: Database) => void): void {
    if (this.closed) return;
    this.deferred.push(fn);
    if (this.exclusiveDepth > 0) {
      return;
    }
    if (this.deferred.length >= MAX_DEFERRED_BATCH) {
      this.flushDeferredSync();
    } else {
      this.scheduleFlush();
    }
  }

  /** Run all deferred callbacks in one IMMEDIATE transaction. */
  flushDeferredSync(): void {
    if (this.closed || this.deferred.length === 0) return;
    this.cancelScheduledFlush();
    const batch = this.deferred.splice(0);
    try {
      withImmediateTransaction(this.db, () => {
        for (const f of batch) {
          f(this.db);
        }
      });
    } catch (e) {
      console.warn(`[mp] deferred write batch failed: ${e instanceof Error ? e.message : String(e)}`);
    }
  }

  private scheduleFlush(): void {
    if (this.flushTimer != null || this.closed) return;
    this.flushTimer = setTimeout(() => {
      this.flushTimer = null;
      try {
        this.flushDeferredSync();
      } catch {
        /* logged in flush */
      }
    }, this.flushDelayMs);
  }

  private cancelScheduledFlush(): void {
    if (this.flushTimer != null) {
      clearTimeout(this.flushTimer);
      this.flushTimer = null;
    }
  }

  /** Stop timer, flush remaining deferred work. */
  close(): void {
    if (this.closed) return;
    this.closed = true;
    this.cancelScheduledFlush();
    this.flushDeferredSync();
  }
}
