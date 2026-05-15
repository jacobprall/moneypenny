# Read/Write Separation for `mp.db`

**Status:** Design  
**Related:** [`parallel-queries.md`](./parallel-queries.md) (readonly `query_db` handle), [`ioc.md`](./ioc.md) (`ToolServices`)

---

## 1. Problem

All reads and writes to `mp.db` go through a single `Database` handle (`AgentDB.db`). This
creates issues in two scenarios:

### 1.1 In-process: parallel tool execution

When `parallelToolExecution` is enabled, `Promise.allSettled` runs multiple tools concurrently.
Tools that read from `agent.db` (search, skills, conversation assembly) can interleave with
the tool executor's post-tool writes (`appendMessage`, `appendEvent`). The `query_db` tool
was moved to a separate readonly handle, but other read paths still share the writer.

### 1.2 Cross-process: multiple consumers of one `mp.db`

Realistic scenarios:
- Two `mp chat` terminals on the same repo
- `mp chat` + MCP sidecar process
- `mp serve` (HTTP + scheduler) + `mp chat`

Each process opens its own `Database` handle. SQLite + WAL serializes writers at the file level
(`SQLITE_BUSY` on commit contention), but Moneypenny has no coordination, retry, or backoff —
a `BUSY` surfaces as an unhandled error.

---

## 2. Goals

1. **In-process correctness:** reads never see partial/interleaved write state; writes are
   ordered and never race each other on the same handle.
2. **Cross-process resilience:** multiple processes sharing `mp.db` do not crash on `BUSY`;
   writes retry with backoff.
3. **Throughput:** writes that the loop does not need to read back immediately can be batched
   or deferred without blocking the next LLM call.
4. **Incremental adoption:** the architecture can ship in phases; each phase is independently
   useful and does not break existing callers.

---

## 3. Write audit: what mutates `mp.db` today

### 3.1 Loop hot path (per LLM iteration)

| Function | Table | Read-your-writes? | Fire-and-forget? |
|----------|-------|--------------------|------------------|
| `appendMessage(assistant)` | `messages` | **Yes** — next `ctx.assemble` reads it back | No |
| `appendMessage(tool result)` | `messages` | **Yes** — next `ctx.assemble` reads it back | No |
| `recordTurnMetrics` | `metrics` | No — cost tracked in-memory via `sessionCostUsd` | **Yes** |
| `appendEvent(cost.recorded)` | `events` | No — observational | **Yes** |
| `appendEvent(tool.called)` | `events` | No — observational | **Yes** |
| `appendEvent(tool.complete)` | `events` | No — observational | **Yes** |
| `appendEvent(tool.error)` | `events` | No — observational | **Yes** |
| `appendEvent(turn.complete)` | `events` | No — observational | **Yes** |
| `appendEvent(turn.paused)` | `events` | No — observational | **Yes** |
| `appendEvent(turn.started)` | `events` | No — observational | **Yes** |

### 3.2 Turn boundaries

| Function | Table | Timing |
|----------|-------|--------|
| `appendMessage(user)` | `messages` | Start of turn, before first LLM call |
| `compactConversation` | `compaction_markers` | Tool-driven, between iterations |

### 3.3 Session lifecycle (between turns / startup)

| Function | Table | Timing |
|----------|-------|--------|
| `createSession` | `sessions` | Startup |
| `setActiveSession` | `sessions` | Startup (`BEGIN IMMEDIATE` + two updates) |
| `labelSession` | `sessions` | After turn (auto-label) |

### 3.4 Config / governance / skills (startup or CLI commands)

| Function | Tables |
|----------|--------|
| `setConfig` | `config` |
| `upsertSkill`, `upsertSkillFile`, `deleteSkillFiles` | `skills`, `skill_files` |
| `upsertSubagentDef` | `subagent_defs` |
| `createPolicy`, `updatePolicy`, `deletePolicy`, `deleteFilePolicies` | `policies` |
| `syncPolicyFiles` | `policies` |
| `scanSkillDirs` | `skills`, `skill_files` |

### 3.5 Cache

| Function | Tables |
|----------|--------|
| `setCachedResult` | `tool_cache` |
| `evictCache` | `tool_cache` |

### 3.6 Agents / jobs (platform layer, same `mp.db`)

| Module | Tables | Note |
|--------|--------|------|
| `repository.ts` | `agents` | Uses raw `Database`, not `AgentDB` |
| `jobs-repo.ts` | `jobs`, `job_runs` | Uses raw `Database`, not `AgentDB` |
| `loader.ts` | `agents`, `jobs` | Uses raw `Database`, not `AgentDB` |

### 3.7 Workspace (`workspace.sqlite` — separate file)

Out of scope for this spec. Low contention, single process typically owns it. Can be brought
under the same pattern later if needed with minimal effort (same shape, different file).

---

## 4. Read audit: what reads from `mp.db`

### 4.1 Read-your-writes critical (loop hot path)

| Function | Used by |
|----------|---------|
| `getConversation` | `ctx.assemble` — builds LLM prompt from messages + compaction markers |
| `getCurrentTurn` | Loop — determines turn number |
| `getLastEvent` | `resume()` — checks if turn already complete |
| `getSessionMetrics` | Loop startup — seeds `sessionCostUsd` (read once per turn, not per iteration) |

### 4.2 Read-only safe (can tolerate WAL snapshot lag)

| Function | Used by |
|----------|---------|
| `query_db` tool | Already on `queryReadDb` |
| `hybridSearch` | `code_search` tool — reads `code_chunks`, `code_fts` (workspace DB usually) |
| `getExcludePatterns` | `code_search` tool |
| `getSkill`, `getSkillFile`, `listSkillFiles` | `read_skill` tool |
| `getSubagentDef` | `delegate` tool |
| `getConfig` | Various startup reads |
| `listPolicies` | `dbPolicyHook` |
| `getPermissions` | CLI startup |
| `getIndexStatus` | CLI startup |
| `getEvents` | MCP resources, inspect |

---

## 5. Architecture

### 5.1 Core primitives

Two new constructs on `AgentDB`, exposed through `@moneypenny/db`:

**`DbWriter`** — serialized write queue

```typescript
interface DbWriter {
  /** Run fn exclusively on the writer connection. Returns when committed. */
  write<T>(fn: (db: Database) => T): Promise<T>;

  /**
   * Enqueue a write that does not need to be awaited by the caller.
   * Failures are logged, not thrown. Batched with other deferred writes.
   */
  defer(fn: (db: Database) => void): void;

  /** Flush all deferred writes. Called automatically on interval and on close. */
  flush(): Promise<void>;

  close(): void;
}
```

- Backed by an async queue (promise chain or explicit FIFO).
- `write()` is for **read-your-writes** mutations: `appendMessage`, `compactConversation`,
  `createSession`, `setActiveSession`.
- `defer()` is for **fire-and-forget** mutations: `appendEvent`, `recordTurnMetrics`,
  `setCachedResult`, `evictCache`.
- Deferred writes are batched into a single `BEGIN IMMEDIATE` … `COMMIT` on a short timer
  (e.g. 50–100 ms) or when the queue reaches a size cap (e.g. 20 items).
- Cross-process `SQLITE_BUSY`: `write()` and `flush()` retry with exponential backoff
  (e.g. 3 attempts, 10 ms / 50 ms / 200 ms) before surfacing an error.

**`DbReadPool`** — small pool of read-only connections

```typescript
interface DbReadPool {
  /** Run fn on any available read-only connection. */
  read<T>(fn: (db: Database) => T): T;

  close(): void;
}
```

- Pool of 2–4 `new Database(dbPath, { readonly: true })` handles.
- Round-robin or "first idle" selection (in practice, synchronous bun:sqlite calls complete
  before yielding, so a pool of 2 is usually sufficient).
- Replaces the current single `queryReadDb` (which becomes the first member of the pool).
- All read-only tool service methods use this pool.

### 5.2 Where they live

```
AgentDB
├── db: Database              ← primary handle (writer uses this)
├── writer: DbWriter          ← serialized write queue over db
├── reads: DbReadPool         ← 2–4 readonly handles
└── dbPath, repoPath, ...     ← unchanged
```

`createAgentDB` initializes `writer` and `reads` lazily or eagerly (TBD per phase).

### 5.3 Wiring into existing code

**Loop / tool-executor** (write path):

```typescript
// read-your-writes: await the write
await db.writer.write((raw) => appendMessageRaw(raw, { turn, role: "assistant", ... }));

// fire-and-forget: deferred, batched
db.writer.defer((raw) => appendEventRaw(raw, { type: "tool.complete", ... }));
db.writer.defer((raw) => recordTurnMetricsRaw(raw, { turn, model, ... }));
```

Current `appendMessage(db: AgentDB, ...)` wrappers become thin delegates to `db.writer.write`
or `db.writer.defer` as appropriate.

**ToolServices** (read path):

```typescript
// createToolServices wires reads through the pool
query: {
  executeReadOnlyQuery(sql, params) {
    return db.reads.read((conn) => {
      // validate, prepare, all — as today
    });
  },
},
search: {
  hybridSearch(query, opts) {
    return db.reads.read((conn) => hybridSearchRaw(conn, query, opts));
  },
},
```

**Agents package** (currently raw `Database`):

Phase 2: accept `DbWriter` (or `AgentDB`) instead of raw `Database`. All agent/job writes go
through the same queue.

### 5.4 Cross-process behavior

- **Writers:** Each process has its own `DbWriter` with its own queue. SQLite WAL serializes
  commits at the file level. `SQLITE_BUSY` is caught and retried inside `write()` / `flush()`.
- **Readers:** Each process has its own `DbReadPool`. WAL readers see a consistent snapshot per
  statement; slight lag behind the writer is acceptable for all read-only paths (see §4.2).
- **No IPC or daemon** required. Each process is self-contained.

---

## 6. Fire-and-forget analysis

### What can be deferred

| Write | Why safe to defer |
|-------|-------------------|
| `appendEvent(*)` | Pure timeline / observability. Nothing in the loop reads events back to decide what to do next (except `resume()` which reads `getLastEvent` at turn start, not mid-iteration). |
| `recordTurnMetrics` | Cost is tracked in-memory (`sessionCostUsd`). Metrics rows are only read by `getSessionMetrics` at turn start and by `query_db`. |
| `setCachedResult` | Cache miss just re-executes the tool. |
| `evictCache` | Runs on a background interval anyway. |

### What must be awaited

| Write | Why |
|-------|-----|
| `appendMessage` | `ctx.assemble` → `getConversation` reads messages to build the next prompt. If the assistant message or tool result is not persisted, the next LLM call gets a stale/incomplete conversation. |
| `compactConversation` | Same reason — markers affect what `getConversation` returns. |
| `createSession` / `setActiveSession` | Subsequent writes scope to `activeSessionId`; must land first. |

### Tradeoff

Deferring `appendEvent` and `recordTurnMetrics` means:
- **Pro:** ~60–70% of per-iteration writes become non-blocking batched inserts.
- **Pro:** Fewer individual transactions = less WAL churn = less `SQLITE_BUSY` cross-process.
- **Con:** If the process crashes mid-turn, deferred events/metrics are lost. Acceptable for
  observability data; the conversation (messages) is always durable.
- **Con:** `resume()` reads `getLastEvent` — if the last `turn.complete` event was deferred and
  not yet flushed, `resume()` might re-run the turn. **Mitigation:** flush before `resume()`
  returns, or flush `turn.complete` synchronously (special-case one event type).

---

## 7. Implementation phases

### Phase 1: `DbWriter` with sync fast-path

- Implement `DbWriter` in `packages/db/src/writer.ts`.
- For `write()`: initially just run synchronously on `agent.db` under a promise-chain mutex
  (no behavior change, just centralized).
- For `defer()`: buffer and flush on a 50 ms timer or 20-item cap; flush wraps all deferred
  callbacks in one `BEGIN IMMEDIATE` … `COMMIT`.
- Add `SQLITE_BUSY` retry (3 attempts, backoff) inside `flush()` and `write()`.
- Wire `appendEvent` and `recordTurnMetrics` through `defer()`.
- Wire `appendMessage` through `write()`.
- Add `writer` to `AgentDB`; update `closeAgentDB` to call `writer.close()` (flushes + stops
  timer).

**Validates:** deferred batching, `BUSY` retry, no behavior change for critical writes.

### Phase 2: `DbReadPool`

- Implement `DbReadPool` in `packages/db/src/read-pool.ts`.
- Migrate `queryReadDb` into the pool (pool of 1 initially, then 2).
- Wire all read-only `ToolServices` methods through `db.reads.read(...)`.
- Update `createToolServices` to use pool instead of `ensureAgentQueryReadDb` directly.
- Update `closeAgentDB` to close the pool.

**Validates:** reads fully isolated from writer; no interleaving on `agent.db` from tools.

### Phase 3: Agents package unification

- Change `repository.ts`, `jobs-repo.ts`, `loader.ts` to accept `AgentDB` (or `DbWriter`)
  instead of raw `Database`.
- All agent/job writes go through `db.writer`.
- Scheduler reads (e.g. `findDue`) go through `db.reads`.

**Validates:** single write discipline for the entire `mp.db` surface.

### Phase 4: Loop integration

- Make `appendMessage` in the loop `await db.writer.write(...)`.
- Make `appendEvent` / `recordTurnMetrics` use `db.writer.defer(...)`.
- Ensure `flush()` is called before `resume()` reads `getLastEvent`.
- Ensure `flush()` is called at turn end (or on `closeAgentDB`).

**Validates:** full fire-and-forget batching in the hot path.

### Phase 5 (optional): Adaptive read pool sizing

- Monitor read contention; grow pool to 3–4 if needed.
- Add pool metrics (wait time, idle connections) behind a debug flag.

---

## 8. Open questions

1. **`appendMessage` latency:** Converting from sync to `await db.writer.write(...)` adds a
   microtask hop. In practice this is ~0 ms because the queue is usually empty (the loop is
   sequential between LLM calls). Measure to confirm.

2. **`turn.complete` event and `resume()`:** Special-case `turn.complete` to flush immediately,
   or always flush before reading `getLastEvent`?

3. **`setActiveSession` uses `BEGIN IMMEDIATE` today.** Should this become a `write()` call
   (which already serializes), or keep the explicit transaction inside the `write` callback?
   Recommendation: keep the explicit transaction inside the callback — `write()` serializes
   callers, the callback owns its own transaction strategy.

4. **Pool size:** Start with 2 readonly handles. If bun:sqlite has per-connection memory
   overhead that matters, start with 1 (the existing `queryReadDb`) and measure.

5. **Timer-based flush interval:** 50 ms is aggressive enough to keep events near-real-time
   for `query_db` / inspect, but lazy enough to batch 3–5 events per transaction in the
   common case. Tune based on profiling.

---

## 9. Non-goals

- **Replacing SQLite** with a client/server DB. SQLite + WAL + `BUSY` retry is sufficient for
  the foreseeable process model (1–3 processes per repo).
- **Cross-process write ordering guarantees** beyond SQLite's own serialization. If two
  processes write events for different sessions, interleaving is fine.
- **Workspace DB (`workspace.sqlite`) separation.** Same pattern applies if needed later.
- **IPC daemon** for write coordination. Each process is self-contained with its own writer
  queue and `BUSY` retry.
