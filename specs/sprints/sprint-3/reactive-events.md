# Reactive Event Layer

### Problem

Today, side-effects are imperative: after an agent writes a memory, the
code explicitly calls the indexer. After a job fails, nothing happens.
The moneypenny-rs spec (sprint-4/self-aware-db.md) envisions a reactive
layer where database writes automatically trigger downstream effects.

### DbWriter integration challenge

The existing `DbWriter` has **synchronous** `exclusive()` and deferred
`defer()` methods. There is no async `write()` that returns a Promise.
The reactive layer must work with this design, not against it.

**Solution:** Hook into `flushDeferredSync()` post-commit. After the
IMMEDIATE transaction succeeds, emit events for the batch:

```typescript
// Extended DbWriter (minimal change)
export class DbWriter {
  private eventCallback: ((events: WriteEvent[]) => void) | null = null;

  /** Register a callback invoked after each successful deferred flush. */
  onFlush(cb: (events: WriteEvent[]) => void): void {
    this.eventCallback = cb;
  }

  flushDeferredSync(): void {
    if (this.closed || this.deferred.length === 0) return;
    this.cancelScheduledFlush();
    const batch = this.deferred.splice(0);

    // Collect write metadata during the transaction
    const writeEvents: WriteEvent[] = [];
    try {
      withImmediateTransaction(this.db, () => {
        for (const f of batch) {
          f(this.db);
        }
      });
    } catch (e) {
      console.warn(`[mp] deferred write batch failed: ${e}`);
      return; // no events on failure
    }

    // Post-commit: emit events
    if (this.eventCallback) {
      this.eventCallback(writeEvents);
    }
  }
}
```

For `exclusive()`, a similar post-return hook:

```typescript
exclusive<T>(fn: (db: Database) => T): T {
  // ... existing logic ...
  try {
    const result = withBusyRetry(() => fn(this.db));
    return result;
  } finally {
    this.exclusiveDepth--;
    // ... existing flush logic ...
    // Post-return: collect changes_count from db.changes()
    // and emit via eventCallback
  }
}
```

### WAL visibility timing

With WAL mode, after a write commits on the writer connection, readers
on separate connections see the data immediately (WAL reads check the
WAL file before the main database file). This means:

- Events emitted after `flushDeferredSync()` returns are safe: any
  reactive handler that reads back the row via `DbReadPool` will see it.
- No checkpoint synchronization is needed.

This is verified by SQLite's WAL documentation: "A read transaction that
is started after a write transaction completes will be able to see the
changes made by the write transaction."

### Event types

```typescript
// @moneypenny/events

export type IntelligenceEvent =
  | { type: "memory_added"; memoryId: string; context: string }
  | { type: "session_completed"; sessionId: string; costUsd: number }
  | { type: "job_completed"; jobId: string; status: "completed" | "failed" }
  | { type: "cost_threshold_crossed"; currentUsd: number; thresholdUsd: number }
  | { type: "skill_discovered"; skillName: string }
  | { type: "index_stale"; staleFileCount: number }
  | { type: "compaction_needed"; sessionId: string; messageCount: number }
  | { type: "governance_violation"; effect: string; toolName: string; policyName: string };
```

### EventBus

```typescript
export class EventBus {
  private listeners = new Map<string, Set<EventHandler>>();
  private inflightHandlers: Promise<void>[] = [];

  on<T extends IntelligenceEvent["type"]>(
    type: T,
    handler: EventHandler<T>,
    opts?: { critical?: boolean; maxRetries?: number },
  ): () => void {
    // Register handler with metadata
    const entry = { handler, critical: opts?.critical ?? false, maxRetries: opts?.maxRetries ?? 0 };
    // ...
    return () => { /* unsubscribe */ };
  }

  emit(event: IntelligenceEvent): void {
    const handlers = this.listeners.get(event.type);
    if (!handlers) return;

    for (const entry of handlers) {
      const promise = this.runHandler(entry, event);
      this.inflightHandlers.push(promise);
      promise.finally(() => {
        const idx = this.inflightHandlers.indexOf(promise);
        if (idx >= 0) this.inflightHandlers.splice(idx, 1);
      });
    }
  }

  /** Wait for all in-flight handlers. Used during graceful shutdown. */
  async drain(timeoutMs = 5000): Promise<void> {
    await Promise.race([
      Promise.allSettled(this.inflightHandlers),
      Bun.sleep(timeoutMs),
    ]);
  }
}
```

### Handler failure isolation

Handlers are classified as **critical** or **non-critical**:

| Handler | Critical? | Retry? | Failure behavior |
|---------|-----------|--------|-----------------|
| Embed new memory | Yes | 2 retries, 1s backoff | Log warning, memory is saved but unsearchable by vector |
| Compaction check | No | No | Skip, next session completion will re-trigger |
| Webhook notifier | No | 3 retries, exponential | Log warning after final failure |
| Cost alert | No | No | Log warning |
| Skill indexer | No | 1 retry | Log warning, skill is saved but uncataloged |

**Critical handler execution:**

```typescript
private async runHandler(entry: HandlerEntry, event: IntelligenceEvent): Promise<void> {
  for (let attempt = 0; attempt <= entry.maxRetries; attempt++) {
    try {
      await Promise.race([
        entry.handler(event),
        Bun.sleep(entry.critical ? 10_000 : 5_000).then(() => {
          throw new Error("handler timeout");
        }),
      ]);
      return;
    } catch (err) {
      if (attempt < entry.maxRetries) {
        await Bun.sleep(1000 * 2 ** attempt);
        continue;
      }
      console.warn(
        `[mp] ${entry.critical ? "CRITICAL" : "non-critical"} handler failed for ${event.type}: ${err}`
      );
    }
  }
}
```

Non-critical handler failures are logged but never block the caller.
Critical handler failures are logged with a `CRITICAL` prefix so they
appear in `mp doctor` output.

### Event routing from DB writes

Rather than using SQLite's raw `update_hook` (which fires per-row and
doesn't carry enough context), we use **explicit event emission** at the
call site. This is more reliable and type-safe:

```typescript
// In knowledge write path:
function addMemory(db: AgentDB, memory: NewMemory): Memory {
  const result = db.writer.exclusive((raw) => {
    // insert into knowledge...
    return row;
  });
  db.eventBus?.emit({ type: "memory_added", memoryId: result.id, context: result.context });
  return result;
}

// In job_runs write path:
function updateJobRun(db: AgentDB, runId: string, status: string): void {
  db.writer.exclusive((raw) => {
    // update job_runs set status = ...
  });
  if (status === "completed" || status === "failed") {
    db.eventBus?.emit({ type: "job_completed", jobId, status });
  }
}
```

**Why explicit over SQLite hooks:**

| Approach | Pros | Cons |
|----------|------|------|
| SQLite `update_hook` | Automatic, catches all writes | No context (only rowid + table), requires read-back, fires during transaction |
| Explicit emission | Type-safe, carries full context, fires post-commit | Must be added at each call site |

We choose explicit emission because:
1. The event carries domain context (not just a rowid)
2. Emission happens post-commit (readers can see the data)
3. No risk of handlers running inside a transaction
4. Type-safe — the compiler catches missing event fields

### Custom handlers via YAML

Users can register custom handlers via `.mp/events/` YAML:

```yaml
# .mp/events/notify-on-failure.yaml
name: notify-on-failure
event: job_completed
condition:
  status: failed
action:
  type: webhook
  url: "${SLACK_WEBHOOK_URL}"
  template: |
    Job "{{jobId}}" failed at {{timestamp}}.
```

Custom handlers are always non-critical with no retries.

### Acceptance criteria

- [ ] `EventBus` emits events after successful DB writes (not during transaction)
- [ ] Critical handlers retry on failure with backoff
- [ ] Non-critical handler failures don't block the write path
- [ ] `drain()` waits for in-flight handlers during shutdown
- [ ] Readers can see committed data when handler runs (WAL visibility)
- [ ] Custom YAML handlers load from `.mp/events/` and fire correctly
- [ ] Memory addition triggers auto-embedding via `memory_added` event
- [ ] Job failure triggers webhook notification via `job_completed` event
- [ ] `mp doctor` reports failed critical handler events

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 2.1 | `EventBus` class with typed listeners, critical/non-critical classification, retry, drain | 1.5 days |
| 2.2 | `DbWriter.onFlush()` hook for post-commit event emission | 1 day |
| 2.3 | Explicit event emission at call sites (knowledge, job_runs, sessions, skills) | 1.5 days |
| 2.4 | Built-in reactive handlers (embed, compaction check, cost alert, webhook) | 2 days |
| 2.5 | Custom handler loading from `.mp/events/*.yaml` | 1 day |
| 2.6 | Integration tests: event ordering, handler isolation, drain | 1 day |
