import type { Database } from "bun:sqlite";

const BUSY_RE = /SQLITE_BUSY|database is locked|locked/i;

function isBusyError(e: unknown): boolean {
  const msg = e instanceof Error ? e.message : String(e);
  return BUSY_RE.test(msg);
}

function sleepMs(ms: number): void {
  try {
    Bun.sleepSync(ms);
  } catch {
    const end = Date.now() + ms;
    while (Date.now() < end) {
      /* spin */
    }
  }
}

/** Run sync fn with up to 3 attempts on SQLITE_BUSY / locked. */
export function withBusyRetry<T>(fn: () => T): T {
  const delays = [10, 50, 200];
  let last: unknown;
  for (let i = 0; i <= delays.length; i++) {
    try {
      return fn();
    } catch (e) {
      last = e;
      if (i === delays.length || !isBusyError(e)) throw e;
      sleepMs(delays[i] ?? 200);
    }
  }
  throw last;
}

/** Run fn in a single transaction with busy retry. */
export function withImmediateTransaction<T>(db: Database, fn: () => T): T {
  return withBusyRetry(() => {
    db.exec("BEGIN IMMEDIATE");
    try {
      const out = fn();
      db.exec("COMMIT");
      return out;
    } catch (e) {
      try {
        db.exec("ROLLBACK");
      } catch {
        /* best effort */
      }
      throw e;
    }
  });
}
