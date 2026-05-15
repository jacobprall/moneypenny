# Sprint 2 — Intelligence Infrastructure

> The sprint that makes the database smart. Read/write separation for
> concurrent tool execution, a unified query engine, computed health and
> suggestion views, conversation compaction, and an autonomous gardener
> agent that keeps the intelligence file clean.

**Prerequisites:** Sprint 1 complete (schema migrations, job system,
blueprint system)

---

## Overview

Sprint 2 addresses five workstreams that transform the SQLite database from
a passive store into an active intelligence substrate. After this sprint,
the agent can run tools in parallel without write contention, query across
all knowledge surfaces through a single interface, get proactive
suggestions from SQL views, compact verbose histories, and rely on a
background gardener to prune stale data.

| # | Workstream | Packages touched |
|---|-----------|-----------------|
| 1 | Read/write separation | `@moneypenny/db` |
| 2 | Unified query engine | `@moneypenny/search`, `@moneypenny/ctx` |
| 3 | Computed intelligence views | `@moneypenny/db` |
| 4 | Conversation compaction | `@moneypenny/loop`, `@moneypenny/ctx` |
| 5 | Gardener agent | `@moneypenny/agents`, built-in blueprint |

---

## 1. Read/Write Separation

> See also: `specs/refactors/read-write.md` for the detailed refactor spec.

### Problem

SQLite is single-writer. Today, the agent loop runs tools sequentially
because parallel writes would cause `SQLITE_BUSY`. This caps throughput: a
session with 5 tool calls that each write audit events or memories takes 5x
longer than it should.

Additionally, `mp serve` needs concurrent read access for the web UI while
an agent session is writing.

### Design

Two components:

1. **DbWriter** — a single writer thread with a serialized write queue
2. **DbReadPool** — a pool of read-only connections

```typescript
// @moneypenny/db

export interface DbWriter {
  /**
   * Enqueue a write operation. Resolves when the write completes.
   * All writes are serialized through a single connection with WAL mode.
   */
  write<T>(fn: (tx: Transaction) => T): Promise<T>;

  /**
   * Defer a non-critical write (audit events, metrics).
   * Returns immediately. Write will be batched.
   */
  defer(fn: (tx: Transaction) => void): void;

  /**
   * Flush all deferred writes. Called on graceful shutdown.
   */
  flush(): Promise<void>;

  close(): Promise<void>;
}

export interface DbReadPool {
  /**
   * Execute a read-only query against any available connection.
   * Multiple reads can execute concurrently.
   */
  read<T>(fn: (db: ReadonlyDatabase) => T): T;

  close(): void;
}
```

### Writer implementation

```typescript
class SqliteWriter implements DbWriter {
  private queue: Array<QueueItem>;
  private processing: boolean;
  private db: Database;           // single writable connection

  constructor(dbPath: string) {
    this.db = new Database(dbPath, { create: true });
    this.db.exec("PRAGMA journal_mode = WAL");
    this.db.exec("PRAGMA busy_timeout = 5000");
    this.db.exec("PRAGMA synchronous = NORMAL");
    this.db.exec("PRAGMA wal_autocheckpoint = 1000");
  }

  async write<T>(fn: (tx: Transaction) => T): Promise<T> {
    return new Promise((resolve, reject) => {
      this.queue.push({ fn, resolve, reject, deferred: false });
      this.processQueue();
    });
  }

  defer(fn: (tx: Transaction) => void): void {
    this.deferredBatch.push(fn);
    this.scheduleDeferredFlush();
  }

  private scheduleDeferredFlush(): void {
    // Flush deferred writes every 100ms or when batch > 50
    if (this.deferredBatch.length >= 50) {
      this.flushDeferred();
    } else if (!this.deferredTimer) {
      this.deferredTimer = setTimeout(() => this.flushDeferred(), 100);
    }
  }

  private flushDeferred(): void {
    const batch = this.deferredBatch.splice(0);
    if (batch.length === 0) return;
    this.write(tx => {
      for (const fn of batch) fn(tx);
    });
  }
}
```

### Read pool implementation

```typescript
class SqliteReadPool implements DbReadPool {
  private connections: Database[];
  private available: Database[];
  private poolSize: number;

  constructor(dbPath: string, poolSize = 4) {
    this.poolSize = poolSize;
    this.connections = Array.from({ length: poolSize }, () => {
      const db = new Database(dbPath, { readonly: true });
      db.exec("PRAGMA journal_mode = WAL");
      return db;
    });
    this.available = [...this.connections];
  }

  read<T>(fn: (db: ReadonlyDatabase) => T): T {
    const conn = this.available.pop() ?? this.connections[0];
    try {
      return fn(conn);
    } finally {
      this.available.push(conn);
    }
  }
}
```

### Migration path

The refactor changes the `AgentDB` constructor surface:

```typescript
// Before
const db = new AgentDB("/path/.mp/mp.db");

// After
const { writer, readers } = createDatabase("/path/.mp/mp.db", {
  readPoolSize: 4,
  deferredFlushIntervalMs: 100,
  deferredBatchSize: 50,
});
const db = new AgentDB(writer, readers);
```

All existing `db.*` calls are categorized:

| Category | Current calls | New path |
|----------|--------------|----------|
| Critical writes | `insertMessage`, `insertSession`, `insertToolCall` | `writer.write(tx => ...)` |
| Deferred writes | `insertGovEvent`, `updateCostMetrics`, `updateLastActivity` | `writer.defer(fn)` |
| Reads | `getSession`, `listMessages`, `search*` | `readers.read(db => ...)` |

### Parallel tool execution

With read/write separation in place, the loop can fan out tool calls:

```typescript
// @moneypenny/loop
if (parallelToolCalls && toolCalls.length > 1) {
  const results = await Promise.all(
    toolCalls.map(tc => executeTool(tc, { writer, readers }))
  );
} else {
  // sequential (current behavior)
  for (const tc of toolCalls) {
    await executeTool(tc, { writer, readers });
  }
}
```

Tool executors that need to write (e.g., `memory_add`) use `writer.write()`.
Tools that only read (e.g., `code_search`) use `readers.read()`. Audit
events for all tools use `writer.defer()`.

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 1.1 | `DbWriter` class with queue, defer, flush | 2 days |
| 1.2 | `DbReadPool` class | 1 day |
| 1.3 | Refactor `AgentDB` to accept writer + readers | 2 days |
| 1.4 | Categorize all existing db calls (write vs defer vs read) | 1 day |
| 1.5 | Parallel tool execution in the loop | 1 day |
| 1.6 | Cross-process resilience testing | 1 day |

---

## 2. Unified Query Engine

### Problem

Today, searching across moneypenny's knowledge surfaces requires calling
different functions in different packages:

- `@moneypenny/search` for code chunks (BM25 + vector)
- `@moneypenny/db` for messages, sessions, skills
- `@moneypenny/ctx` for memory retrieval

The agent's `code_search` tool only reaches code chunks. A solo developer
asking "what do I know about authentication?" should get results from code,
memories, skills, sessions, and docs — all from one query.

### Design

A `UnifiedQuery` engine that fans out a query across all surfaces,
normalizes results into a common format, and ranks them.

```typescript
// @moneypenny/search (extended)

export interface QueryResult {
  surface: "code" | "memory" | "skill" | "session" | "doc";
  id: string;
  title: string;
  content: string;
  score: number;              // normalized 0..1
  metadata: Record<string, unknown>;
}

export interface UnifiedQueryOptions {
  query: string;
  surfaces?: Array<"code" | "memory" | "skill" | "session" | "doc">;
  limit?: number;             // default 20
  minScore?: number;          // default 0.1
  hybridWeights?: {
    bm25: number;             // default 0.4
    vector: number;           // default 0.6
  };
}

export class UnifiedQuery {
  constructor(
    private readers: DbReadPool,
    private workspaceReaders: DbReadPool,
  ) {}

  async search(opts: UnifiedQueryOptions): Promise<QueryResult[]> {
    const surfaces = opts.surfaces ?? ["code", "memory", "skill", "session"];
    const results = await Promise.all(
      surfaces.map(s => this.searchSurface(s, opts))
    );
    return results
      .flat()
      .sort((a, b) => b.score - a.score)
      .slice(0, opts.limit ?? 20);
  }
}
```

### Surface search implementations

| Surface | Source table | Search method |
|---------|------------|---------------|
| `code` | `code_chunks` (workspace DB) | BM25 + vector hybrid (existing) |
| `memory` | `messages` + `knowledge` | FTS5 on content + vector on embedding |
| `skill` | `skills` + `skill_files` | FTS5 on description + content |
| `session` | `sessions` + `compaction_markers` | FTS5 on label + compacted summaries |
| `doc` | `docs` (new table, optional) | FTS5 + vector |

### Score normalization

Each surface returns raw scores in different ranges. The engine normalizes
using per-surface min-max scaling within each query, then applies hybrid
weights:

```
final_score = (bm25_weight * bm25_normalized) + (vector_weight * vector_normalized)
```

If a surface only supports one method (e.g., sessions → FTS only), the
single score is used directly.

### Tool integration

The existing `code_search` tool is extended with an optional `surfaces`
parameter. Default behavior (code only) is preserved for backward
compatibility:

```typescript
const codeSearchTool = defineTool({
  name: "code_search",
  parameters: z.object({
    query: z.string(),
    surfaces: z.array(z.enum(["code", "memory", "skill", "session", "doc"]))
      .optional()
      .describe("Search across multiple knowledge surfaces. Default: code only."),
    limit: z.number().optional(),
  }),
});
```

### Context assembly

The `@moneypenny/ctx` context assembler uses `UnifiedQuery` to build
context blocks for the system prompt. The existing flow:

```
user query → code_search → rank → assemble system prompt
```

Becomes:

```
user query → unified_search(code + memory + skill) → rank → assemble system prompt
```

The context assembler respects token budgets per surface:

```typescript
export interface ContextBudget {
  totalTokens: number;         // overall limit
  codeTokens: number;          // max for code chunks
  memoryTokens: number;        // max for memories
  skillTokens: number;         // max for skills
  reservedTokens: number;      // for system prompt + history
}
```

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 2.1 | `UnifiedQuery` class with surface fan-out and score normalization | 2 days |
| 2.2 | FTS5 indexes on messages, skills, sessions | 1 day |
| 2.3 | Vector embeddings for memories (extend existing embed pipeline) | 1.5 days |
| 2.4 | Extend `code_search` tool with `surfaces` parameter | 0.5 days |
| 2.5 | Context assembler integration with per-surface budgets | 1.5 days |

---

## 3. Computed Intelligence Views

### Problem

The agent and UI have no way to get a quick health check of the
intelligence file without running ad-hoc queries. The moneypenny-rs spec
(sprint-4/self-aware-db.md) introduces SQL views that compute health
metrics and actionable suggestions. These are cheap to query and provide
a foundation for the gardener agent and the Observe UI page.

### Design

Create SQL views (computed on read) that summarize system health and
generate actionable suggestions.

### `mp_health` view

```sql
CREATE VIEW mp_health AS
SELECT
  -- Sessions
  (SELECT COUNT(*) FROM sessions) AS total_sessions,
  (SELECT COUNT(*) FROM sessions
   WHERE last_activity_at > unixepoch() - 86400) AS sessions_today,
  (SELECT COALESCE(SUM(json_extract(cost, '$.totalCost')), 0) FROM sessions
   WHERE last_activity_at > unixepoch() - 86400) AS cost_today_usd,
  (SELECT COALESCE(SUM(json_extract(cost, '$.totalCost')), 0) FROM sessions
   WHERE last_activity_at > unixepoch() - 604800) AS cost_week_usd,

  -- Knowledge
  (SELECT COUNT(*) FROM messages WHERE role = 'assistant') AS total_responses,
  (SELECT COUNT(*) FROM skills) AS total_skills,
  (SELECT COUNT(*) FROM knowledge) AS total_knowledge_entries,

  -- Index (workspace DB stats — requires cross-db query or separate view)
  -- Populated by watcher stats endpoint instead

  -- Jobs
  (SELECT COUNT(*) FROM jobs WHERE enabled = 1) AS active_jobs,
  (SELECT COUNT(*) FROM job_runs
   WHERE status = 'failed'
   AND created_at > unixepoch() - 86400) AS failed_jobs_today,

  -- Governance
  (SELECT COUNT(*) FROM gov_events
   WHERE created_at > unixepoch() - 86400) AS gov_events_today,
  (SELECT COUNT(*) FROM gov_events
   WHERE effect = 'deny'
   AND created_at > unixepoch() - 86400) AS denied_today,

  -- Activity
  (SELECT MAX(last_activity_at) FROM sessions) AS last_session_at,
  (SELECT MAX(created_at) FROM job_runs) AS last_job_run_at;
```

### `mp_suggestions` view

```sql
CREATE VIEW mp_suggestions AS
-- Stale sessions that could be compacted
SELECT
  'compact_session' AS suggestion_type,
  s.id AS target_id,
  s.label AS target_label,
  'Session has ' || mc.msg_count || ' messages and no compaction marker' AS reason,
  mc.msg_count AS priority_score
FROM sessions s
JOIN (SELECT session_id, COUNT(*) AS msg_count FROM messages GROUP BY session_id) mc
  ON mc.session_id = s.id
LEFT JOIN compaction_markers cm ON cm.session_id = s.id
WHERE mc.msg_count > 50 AND cm.id IS NULL

UNION ALL

-- Unused skills (not referenced in any session in 30 days)
SELECT
  'review_skill' AS suggestion_type,
  sk.name AS target_id,
  sk.name AS target_label,
  'Skill not referenced in any session for 30+ days' AS reason,
  30 AS priority_score
FROM skills sk
WHERE sk.name NOT IN (
  SELECT DISTINCT json_extract(value, '$.skill')
  FROM messages, json_each(messages.content)
  WHERE messages.created_at > unixepoch() - 2592000
    AND json_valid(value)
    AND json_extract(value, '$.skill') IS NOT NULL
)

UNION ALL

-- High-cost agents (daily spend anomalies)
SELECT
  'review_cost' AS suggestion_type,
  a.name AS target_id,
  a.name AS target_label,
  'Agent spent $' || ROUND(daily_cost, 4) || ' today (>' || ROUND(avg_cost * 2, 4) || ' 2x avg)' AS reason,
  CAST(daily_cost * 1000 AS INTEGER) AS priority_score
FROM agents a
JOIN (
  SELECT
    s.agent_name,
    SUM(CASE WHEN s.last_activity_at > unixepoch() - 86400
        THEN COALESCE(json_extract(s.cost, '$.totalCost'), 0) ELSE 0 END) AS daily_cost,
    AVG(COALESCE(json_extract(s.cost, '$.totalCost'), 0)) AS avg_cost
  FROM sessions s
  WHERE s.last_activity_at > unixepoch() - 604800
  GROUP BY s.agent_name
) costs ON costs.agent_name = a.name
WHERE daily_cost > avg_cost * 2 AND daily_cost > 0.01

UNION ALL

-- Failed jobs needing attention
SELECT
  'fix_job' AS suggestion_type,
  j.id AS target_id,
  j.name AS target_label,
  'Job failed ' || fail_count || ' times in last 24h' AS reason,
  fail_count * 10 AS priority_score
FROM jobs j
JOIN (
  SELECT job_id, COUNT(*) AS fail_count
  FROM job_runs
  WHERE status = 'failed' AND created_at > unixepoch() - 86400
  GROUP BY job_id
) fr ON fr.job_id = j.id
WHERE fail_count >= 2

ORDER BY priority_score DESC;
```

### API exposure

```
GET /api/v1/observe/health      → SELECT * FROM mp_health
GET /api/v1/observe/suggestions  → SELECT * FROM mp_suggestions
```

The Observe page in the web UI renders these:
- Health as a dashboard card grid (sessions today, cost today, failed jobs, etc.)
- Suggestions as an actionable list with "Fix" buttons

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 3.1 | `mp_health` view + API endpoint | 1 day |
| 3.2 | `mp_suggestions` view (compaction, skills, costs, jobs) | 2 days |
| 3.3 | Web UI: health dashboard cards + suggestion list | 1.5 days |
| 3.4 | CLI: `mp status` command displaying health + top suggestions | 0.5 days |

---

## 4. Conversation Compaction

### Problem

Long sessions accumulate hundreds of messages. Loading them into context
windows wastes tokens on verbose tool call/result pairs. The Rust version's
compaction system generates concise summaries while preserving semantic
anchors. Solo developers running long coding sessions benefit the most.

### Design

```typescript
// @moneypenny/ctx

export interface CompactionConfig {
  triggerThreshold: number;     // messages before compaction triggers (default 40)
  keepRecent: number;           // messages to keep uncompacted (default 10)
  model: string;                // model for summary generation
  maxSummaryTokens: number;     // token budget for summary (default 2000)
}

export interface CompactionResult {
  sessionId: string;
  upToTurn: number;
  summary: string;
  originalMessages: number;
  summaryTokens: number;
  savedTokens: number;
}
```

### Compaction flow

```
Session has 60 messages (turns 1..60)
  │
  ├── keepRecent = 10 → keep messages 51..60 intact
  │
  ├── Compact messages 1..50 into summary
  │   1. Group by turns (user + assistant + tool calls)
  │   2. Extract key decisions, code changes, errors, resolutions
  │   3. LLM call: "Summarize this conversation preserving decisions and outcomes"
  │   4. Store summary in compaction_markers table
  │
  └── When loading session for context:
      1. Load compaction_markers for session
      2. Load messages after latest marker's up_to_turn
      3. Assemble: [system prompt] + [compacted summaries] + [recent messages]
```

### Trigger conditions

Compaction triggers when:
- Message count exceeds `triggerThreshold` AND no existing marker covers them
- Manually via `context_curate` tool's `summarize_session` action
- By the gardener agent during maintenance runs

### Summary generation prompt

```
You are summarizing a coding conversation. Preserve:
- All decisions made and their rationale
- Files created, modified, or deleted
- Errors encountered and how they were resolved
- Key code patterns or approaches chosen
- Any commitments or TODOs mentioned

Be concise. Use structured format:
## Decisions
- ...
## Changes
- ...
## Issues & Resolutions
- ...
## Open Items
- ...
```

### Integration with context assembly

```typescript
// @moneypenny/ctx assembler

function assembleHistory(sessionId: string, readers: DbReadPool): Message[] {
  const markers = readers.read(db =>
    db.prepare("SELECT * FROM compaction_markers WHERE session_id = ? ORDER BY up_to_turn ASC")
      .all(sessionId)
  );

  const latestMarkerTurn = markers.length > 0
    ? markers[markers.length - 1].up_to_turn
    : 0;

  const recentMessages = readers.read(db =>
    db.prepare("SELECT * FROM messages WHERE session_id = ? AND turn > ? ORDER BY turn ASC")
      .all(sessionId, latestMarkerTurn)
  );

  const compactedHistory: Message[] = markers.map(m => ({
    role: "user" as const,
    content: `[Previous conversation summary]\n\n${m.summary}`,
  }));

  return [...compactedHistory, ...recentMessages];
}
```

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 4.1 | `CompactionConfig` types, `compaction_markers` schema | 0.5 days |
| 4.2 | Summary generation with LLM (prompt, extraction, storage) | 2 days |
| 4.3 | Automatic trigger in the loop (threshold check post-turn) | 1 day |
| 4.4 | Context assembly integration (load markers + recent messages) | 1 day |
| 4.5 | Manual trigger via `context_curate.summarize_session` | 0.5 days |

---

## 5. Gardener Agent

### Problem

The intelligence file accumulates stale data: orphaned code chunks for
deleted files, verbose sessions that should be compacted, unused skills,
redundant memories. A solo developer shouldn't have to manually curate
their agent's knowledge. The moneypenny-rs spec envisions an autonomous
gardener that runs as a scheduled job.

### Design

The gardener is a built-in blueprint (`_gardener`) that runs as a scheduled
job. It uses the `context_curate` tool to inspect and maintain the
intelligence file.

### Blueprint

```yaml
---
name: _gardener
description: Autonomous maintenance agent. Prunes stale data, compacts verbose sessions, reviews cost anomalies.
model: claude-3-5-haiku-20241022
tools:
  - context_curate
max_turns: 20
guardrails:
  max_cost_usd: 0.05
  filesystem_sandbox: []       # no filesystem access
schedule:
  cron: "0 3 * * *"            # 3 AM daily
  trigger: cron
  enabled: true
strategy: standard
---

You are the gardener agent for a developer's coding assistant. Your job is
to maintain the intelligence file — the database that stores sessions,
memories, skills, code index, and agent configuration.

## Routine

1. **Check health:** Use `context_curate` with `action: review_costs` to
   check for cost anomalies.

2. **Prune stale chunks:** Use `context_curate` with `action: index_status`
   to check for stale code chunks. If stale chunks exist, use
   `action: prune_stale_chunks` to remove them.

3. **Compact sessions:** Use `context_curate` with `action: list_sessions`
   to find sessions with many messages. For sessions with > 50 messages
   and no compaction markers, use `action: summarize_session` to compact
   them.

4. **Review skills:** Use `context_curate` with `action: list_skills` to
   check for unused or stale skills. Report any anomalies but do not
   delete them without explicit user confirmation in a prior session.

5. **Report:** Summarize what you did and any issues found.

## Rules

- Never delete skills without prior user confirmation
- Never delete sessions — only compact them
- Prune stale chunks (for deleted files) freely
- Keep total cost under $0.05 per run
```

### Job registration

The gardener is registered as an `agent_run` job during `mp init` or
on first `mp serve`. It appears in the Jobs page alongside user-defined
jobs. Users can disable it, change its schedule, or trigger it manually.

### Gardener run tracking

Each gardener run creates a session (agent: `_gardener`) and a `job_run`
entry. Results are visible in:
- Jobs page → gardener → run history
- Sessions page (filtered to `_gardener` agent)
- `mp status` shows "last gardener run: 3h ago, pruned 42 chunks"

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 5.1 | Built-in `_gardener` blueprint + registration on init/serve | 1 day |
| 5.2 | End-to-end test: gardener runs, prunes, compacts, reports | 1.5 days |
| 5.3 | `mp status` integration with gardener stats | 0.5 days |

---

## Implementation Order

```
Phase 1: Read/write separation (§1)
  │       ↑ unblocks everything else
  │
  ├── Phase 2: Unified query engine (§2)
  │   depends on read pool for concurrent search
  │
  ├── Phase 3: Computed views (§3) [independent of §2]
  │   uses reads only, no write path
  │
  ├── Phase 4: Compaction (§4) [independent of §2, §3]
  │   needs writer for storing markers
  │
  └── Phase 5: Gardener agent (§5)
      depends on §4 (compaction) and sprint 1 §6 (job system)
      depends on sprint 1 §7 (context_curate tool)
```

Read/write separation is the critical path. Once it lands, phases 2–4 can
proceed in parallel. Phase 5 (gardener) is the capstone that ties
everything together.

---

## What we deliberately skip

- **Reactive triggers (SQLite write hooks → event bus)** — deferred to
  sprint 3.
- **Self-evolving prompts** — deferred to sprint 3 (requires reactive
  layer).
- **Embeddable SQL extensions** — deferred to sprint 3.
- **Cross-workspace federation** — out of scope for solo dev use case.
