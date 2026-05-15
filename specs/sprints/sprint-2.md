# Sprint 2 — Intelligence Infrastructure

> The sprint that makes the database smart. Parallel tool execution,
> a unified query engine, computed health views, conversation compaction,
> embedding pipeline, and an autonomous gardener agent.

**Prerequisites:** Sprint 1 complete (schema v10, job system, blueprints,
`context_curate` tool)

---

## Existing foundations (already implemented)

| Component | Location | Status |
|-----------|----------|--------|
| `DbWriter` (exclusive + defer) | `@moneypenny/db/writer.ts` | Production. 107 lines. |
| `DbReadPool` (round-robin readers) | `@moneypenny/db/read-pool.ts` | Production. 70 lines. |
| `withBusyRetry` (cross-process) | `@moneypenny/db/busy-retry.ts` | Production. |
| Scheduler uses `agent.reads.read()` / `agent.writer.exclusive()` | `@moneypenny/agents/scheduler.ts` | Production. |
| BM25 + vector hybrid search (vector leg is extension-gated) | `@moneypenny/search/search.ts` | Production. RRF fusion. |
| Embeddings inserted as NULL by indexer | `@moneypenny/search/indexer.ts` | **Gap.** Vector leg dead without embeddings. |

**Key insight:** Read/write separation is built. What's missing is:
(a) wiring parallel tool execution in the loop, (b) actually populating
embeddings, and (c) building the higher-level intelligence features on
top of the existing infrastructure.

---

## Overview

| # | Workstream | Packages touched |
|---|-----------|-----------------|
| 1 | Embedding pipeline | `@moneypenny/search`, `@moneypenny/db` |
| 2 | Parallel tool execution | `@moneypenny/loop` |
| 3 | Unified query engine | `@moneypenny/search`, `@moneypenny/ctx` |
| 4 | Computed intelligence views | `@moneypenny/db` |
| 5 | Conversation compaction | `@moneypenny/loop`, `@moneypenny/ctx` |
| 6 | Gardener agent | `@moneypenny/agents`, built-in blueprint |

---

## 1. Embedding Pipeline

### Problem

The indexer writes `embedding` as NULL for every code chunk. The search
module has a vector leg (`hybridSearch` calls `llm_embed_generate` at
query time) but with no stored embeddings, vector search returns nothing
and the entire hybrid search degrades to BM25-only. The unified query
engine (§3), reactive auto-embed (sprint 3), and context quality eval
(sprint 4) all depend on working embeddings.

### Design

Use the `@sqliteai/sqlite-ai` extension's `llm_embed_generate()` function,
which is already loaded in `database.ts` / `workspace.ts` on a best-effort
basis.

```typescript
// @moneypenny/search

export interface EmbedConfig {
  model: string;          // default: "text-embedding-3-small" (via sqlite-ai)
  batchSize: number;      // default: 50 chunks per transaction
  enabled: boolean;       // default: true if extension loads
}

export function embedChunks(
  db: Database,
  chunks: Array<{ rowid: number; content: string }>,
  config: EmbedConfig,
): { embedded: number; failed: number; durationMs: number };
```

### Integration with indexer

After `chunkFileContent` writes chunks with NULL embeddings, a second pass
calls `embedChunks` on the new/modified chunks:

```typescript
// In indexer.ts, after chunk insertion:
if (embedConfig.enabled) {
  const nullChunks = db.prepare(
    "SELECT rowid, content FROM code_chunks WHERE embedding IS NULL LIMIT ?"
  ).all(embedConfig.batchSize);

  embedChunks(db, nullChunks, embedConfig);
}
```

### Backfill command

`mp index --embed` re-embeds all chunks with NULL embeddings. This is
idempotent and can be interrupted and resumed.

### Graceful degradation

If the sqlite-ai extension fails to load (missing native binary, unsupported
platform), embeddings remain NULL and search falls back to BM25-only.
The `mp doctor` command reports embedding status:

```
Embeddings: ✗ sqlite-ai extension not available
  Vector search will be disabled. BM25 full-text search still works.
  Install: https://github.com/nickhudkins/moneypenny/wiki/sqlite-ai
```

### Acceptance criteria

- [ ] `mp index` populates embeddings for all code chunks when extension is available
- [ ] `mp index --embed` backfills NULL embeddings on existing databases
- [ ] `hybridSearch` returns vector results when embeddings are populated
- [ ] Search works (BM25-only) when extension is unavailable
- [ ] `mp doctor` reports embedding status accurately
- [ ] Embedding errors for individual chunks don't fail the entire index operation

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 1.1 | `embedChunks` function with batch processing and error handling | 1.5 days |
| 1.2 | Wire into indexer post-chunk-insertion pass | 0.5 days |
| 1.3 | `mp index --embed` backfill command | 0.5 days |
| 1.4 | `mp doctor` embedding status check | 0.5 days |

---

## 2. Parallel Tool Execution

### Problem

The loop already supports `parallelToolExecution` as a config flag, and
the existing code uses `Promise.allSettled` for parallel tools. But the
implementation is incomplete: audit events, cost metrics, and governance
decisions all write to the DB, and without proper routing through the
writer, parallel writes can interleave.

### What exists

The loop's tool executor already fans out via `Promise.allSettled` when
`parallelToolExecution` is true. The `DbWriter.exclusive()` serializes
writes. What's missing is categorizing every write in the tool execution
path and ensuring non-critical writes use `defer()`.

### Remaining work

Categorize all writes in the tool execution path:

| Write | Current path | Should be |
|-------|-------------|-----------|
| `appendMessage` (tool result) | `writer.exclusive()` | `writer.exclusive()` (read-your-writes needed) |
| `appendEvent` (tool.called, tool.complete) | `writer.exclusive()` | `writer.defer()` (not read back in loop) |
| `recordTurnMetrics` | `writer.exclusive()` | `writer.defer()` (metrics, not read in loop) |
| `insertGovEvent` | `writer.exclusive()` | `writer.defer()` (audit trail) |
| `updateLastActivity` | `writer.exclusive()` | `writer.defer()` (timestamp) |
| `tool_cache` writes | `writer.exclusive()` | `writer.defer()` (cache, not critical) |
| Search queries (code_search tool) | `writer.exclusive()` on same handle | `readers.read()` (read-only) |
| `memory_add` | `writer.exclusive()` | `writer.exclusive()` (must persist) |

After categorization, parallel tool execution becomes safe: critical
writes go through `exclusive()` (serialized), non-critical writes go
through `defer()` (batched), and reads go through `readers.read()`
(concurrent).

### Acceptance criteria

- [ ] 3 independent read-only tool calls (e.g., 3x `code_search`) run concurrently
- [ ] Tool results arrive in correct order in the message history
- [ ] Governance events are recorded for all parallel tool calls
- [ ] Cost metrics are accurate after parallel execution
- [ ] No `SQLITE_BUSY` errors during parallel tool execution
- [ ] Deferred writes flush within 100ms of batch threshold

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 2.1 | Audit all tool executor writes, categorize as exclusive/defer/read | 1 day |
| 2.2 | Refactor tool executor to use correct write path per category | 1.5 days |
| 2.3 | Route search/read-only tool queries through `readers.read()` | 0.5 days |
| 2.4 | Integration tests: parallel tools, write ordering, busy resilience | 1 day |

---

## 3. Unified Query Engine

### Problem

Searching across moneypenny's knowledge surfaces requires calling
different functions in different packages. The agent's `code_search`
tool only reaches code chunks. A developer asking "what do I know about
authentication?" should get results from code, memories, skills, and
sessions.

### Two-database challenge

Moneypenny has two databases:

- **Session DB** (`mp.db`): sessions, messages, agents, skills, knowledge,
  policies, gov_events, config
- **Workspace DB** (`workspace.db`): code_chunks, code_fts, file_tree

The unified query engine must search across both. Both are opened by
`AgentDB` — the workspace DB via `getWorkspaceHandle()`. The engine
receives both read pools:

```typescript
export class UnifiedQuery {
  constructor(
    private sessionReaders: DbReadPool,   // mp.db readers
    private workspaceReaders: DbReadPool | null,  // workspace.db readers (null if not indexed)
  ) {}
}
```

If the workspace DB doesn't exist (user hasn't run `mp index`), the
`code` surface is silently skipped and results come from session DB only.

### Score normalization: Reciprocal Rank Fusion

Min-max normalization within a query makes cross-surface ranking
meaningless for small result sets (the worst result in each surface
always gets score 0). Instead, use **Reciprocal Rank Fusion (RRF)** —
the same method already used in `hybridSearch`:

```typescript
// For each surface, rank results by their native score
// Then compute RRF score: 1 / (k + rank)
// k = 60 (standard RRF constant, matches existing search.ts)

function rrf(rank: number, k = 60): number {
  return 1 / (k + rank);
}
```

Each surface returns results ranked by its native scoring. The engine
assigns RRF scores based on rank position, then merges and sorts by
RRF score. This produces meaningful cross-surface rankings without
requiring score normalization.

### Surface implementations

| Surface | Source | DB | Search method | Notes |
|---------|--------|----|---------------|-------|
| `code` | `code_chunks` + `code_fts` | workspace | BM25 + vector hybrid (existing `hybridSearch`) | Requires workspace DB |
| `memory` | `knowledge` table | session | FTS5 on content + vector (if embedded) | New FTS index needed |
| `skill` | `skills` + `skill_files` | session | FTS5 on description + instructions | New FTS index needed |
| `session` | `sessions` + `compaction_markers` | session | FTS5 on label + compacted summaries | Only useful after compaction (§5) |

### FTS indexes (new)

```sql
-- Added in schema migration v11 (sprint 2)
CREATE VIRTUAL TABLE IF NOT EXISTS knowledge_fts USING fts5(
  content, context,
  content='knowledge', content_rowid='rowid',
  tokenize='porter unicode61'
);

CREATE VIRTUAL TABLE IF NOT EXISTS skills_fts USING fts5(
  name, description, instructions,
  content='skills', content_rowid='rowid',
  tokenize='porter unicode61'
);

-- Sync triggers
CREATE TRIGGER knowledge_fts_ai AFTER INSERT ON knowledge BEGIN
  INSERT INTO knowledge_fts(rowid, content, context) VALUES (new.rowid, new.content, new.context);
END;
-- (+ delete/update triggers)
```

### Context assembly with budgets

The context assembler uses `UnifiedQuery` to build context blocks. Each
surface has a token budget. When the total exceeds `totalTokens -
reservedTokens`, surfaces are trimmed in priority order:

```typescript
export interface ContextBudget {
  totalTokens: number;         // overall limit (e.g., 8000)
  reservedTokens: number;      // system prompt + history (e.g., 3000)
  surfacePriority: Array<"code" | "memory" | "skill" | "session">;
  // default: ["code", "memory", "skill", "session"]
}
```

**Resolution when over budget:** Starting from the lowest-priority
surface, reduce results until total fits. If a single surface's results
already exceed the available budget, truncate that surface's results.
This ensures code context (highest priority by default) is never crowded
out by memories.

### Tool integration

The existing `code_search` tool gains an optional `surfaces` parameter:

```typescript
parameters: z.object({
  query: z.string(),
  surfaces: z.array(z.enum(["code", "memory", "skill", "session"]))
    .optional()
    .describe("Knowledge surfaces to search. Default: code only."),
  limit: z.number().optional(),
})
```

Default behavior (code only) is preserved for backward compatibility.

### Acceptance criteria

- [ ] `code_search` with no `surfaces` param returns code results only (backward compatible)
- [ ] `code_search` with `surfaces: ["code", "memory", "skill"]` returns cross-surface results
- [ ] Results are ranked by RRF score across surfaces
- [ ] Missing workspace DB (no `mp index` run) doesn't crash — code surface is skipped
- [ ] Context assembly respects per-surface token budgets
- [ ] Code surface is never crowded out by lower-priority surfaces

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 3.1 | `UnifiedQuery` class with two-DB fan-out and RRF scoring | 2 days |
| 3.2 | FTS5 indexes on knowledge and skills + sync triggers | 1 day |
| 3.3 | Extend `code_search` tool with `surfaces` parameter | 0.5 days |
| 3.4 | Context assembler integration with priority-based budget trimming | 1.5 days |

---

## 4. Computed Intelligence Views

### Problem

The agent and UI have no way to get a quick health check without ad-hoc
queries. SQL views compute health metrics and actionable suggestions.

### `mp_health` view

```sql
CREATE VIEW mp_health AS
SELECT
  (SELECT COUNT(*) FROM sessions) AS total_sessions,
  (SELECT COUNT(*) FROM sessions
   WHERE last_activity_at > unixepoch() - 86400) AS sessions_today,
  (SELECT COALESCE(SUM(json_extract(cost, '$.totalCost')), 0) FROM sessions
   WHERE last_activity_at > unixepoch() - 86400) AS cost_today_usd,
  (SELECT COALESCE(SUM(json_extract(cost, '$.totalCost')), 0) FROM sessions
   WHERE last_activity_at > unixepoch() - 604800) AS cost_week_usd,
  (SELECT COUNT(*) FROM messages WHERE role = 'assistant') AS total_responses,
  (SELECT COUNT(*) FROM skills) AS total_skills,
  (SELECT COUNT(*) FROM knowledge) AS total_knowledge_entries,
  (SELECT COUNT(*) FROM jobs WHERE enabled = 1) AS active_jobs,
  (SELECT COUNT(*) FROM job_runs
   WHERE status = 'failed' AND created_at > unixepoch() - 86400) AS failed_jobs_today,
  (SELECT COUNT(*) FROM gov_events
   WHERE created_at > unixepoch() - 86400) AS gov_events_today,
  (SELECT COUNT(*) FROM gov_events
   WHERE effect = 'deny' AND created_at > unixepoch() - 86400) AS denied_today,
  (SELECT MAX(last_activity_at) FROM sessions) AS last_session_at,
  (SELECT MAX(created_at) FROM job_runs) AS last_job_run_at;
```

### `mp_suggestions` view

The prior version had a skill-usage detection query that used
`json_each(messages.content)` — this doesn't work because message content
is not structured JSON with a `$.skill` field. Revised to use a simpler
heuristic: skills not referenced by name in any recent message text.

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

-- Unused skills: not mentioned by name in any message in 30 days
SELECT
  'review_skill' AS suggestion_type,
  sk.name AS target_id,
  sk.name AS target_label,
  'Skill "' || sk.name || '" not mentioned in any message for 30+ days' AS reason,
  30 AS priority_score
FROM skills sk
WHERE NOT EXISTS (
  SELECT 1 FROM messages m
  WHERE m.created_at > unixepoch() - 2592000
    AND m.content LIKE '%' || sk.name || '%'
)

UNION ALL

-- High-cost agents (daily spend > 2x weekly average)
SELECT
  'review_cost' AS suggestion_type,
  costs.agent_name AS target_id,
  costs.agent_name AS target_label,
  'Agent spent $' || ROUND(costs.daily_cost, 4) || ' today (>' || ROUND(costs.avg_cost * 2, 4) || ' 2x avg)' AS reason,
  CAST(costs.daily_cost * 1000 AS INTEGER) AS priority_score
FROM (
  SELECT
    s.agent_name,
    SUM(CASE WHEN s.last_activity_at > unixepoch() - 86400
        THEN COALESCE(json_extract(s.cost, '$.totalCost'), 0) ELSE 0 END) AS daily_cost,
    AVG(COALESCE(json_extract(s.cost, '$.totalCost'), 0)) AS avg_cost
  FROM sessions s
  WHERE s.last_activity_at > unixepoch() - 604800
  GROUP BY s.agent_name
) costs
WHERE costs.daily_cost > costs.avg_cost * 2 AND costs.daily_cost > 0.01

UNION ALL

-- Failed jobs needing attention
SELECT
  'fix_job' AS suggestion_type,
  j.id AS target_id,
  j.name AS target_label,
  'Job failed ' || fr.fail_count || ' times in last 24h' AS reason,
  fr.fail_count * 10 AS priority_score
FROM jobs j
JOIN (
  SELECT job_id, COUNT(*) AS fail_count
  FROM job_runs
  WHERE status = 'failed' AND created_at > unixepoch() - 86400
  GROUP BY job_id
) fr ON fr.job_id = j.id
WHERE fr.fail_count >= 2

ORDER BY priority_score DESC;
```

### Acceptance criteria

- [ ] `SELECT * FROM mp_health` returns all metrics correctly
- [ ] `SELECT * FROM mp_suggestions` returns actionable suggestions
- [ ] Skill usage detection works with simple `LIKE` matching
- [ ] Cost anomaly detection correctly identifies 2x daily spikes
- [ ] `GET /api/v1/observe/health` returns mp_health data
- [ ] `GET /api/v1/observe/suggestions` returns mp_suggestions data
- [ ] `mp status` CLI command displays health + top 5 suggestions

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 4.1 | `mp_health` view + API endpoint | 1 day |
| 4.2 | `mp_suggestions` view (revised skill detection) | 1.5 days |
| 4.3 | Web UI: health dashboard + suggestion list in Observe page | 1.5 days |
| 4.4 | `mp status` CLI command | 0.5 days |

---

## 5. Conversation Compaction

### Problem

Long sessions accumulate hundreds of messages. Loading them into context
windows wastes tokens on verbose tool call/result pairs.

### Design

```typescript
export interface CompactionConfig {
  triggerThreshold: number;     // messages before compaction triggers (default 40)
  keepRecent: number;           // messages to keep uncompacted (default 10)
  model: string;                // model for summary generation
  maxSummaryTokens: number;     // token budget for summary (default 2000)
}
```

### Cost and model

Compaction uses **claude-3-5-haiku** (cheapest Anthropic model). At ~$0.25
per million input tokens, a 50-message session (~20K tokens) costs ~$0.005
to compact. The gardener (§6) has a $0.05 budget, so it can compact ~10
sessions per run.

### Context window handling

If the session exceeds the model's context window (200K for Haiku):
1. Split messages into context-window-sized chunks
2. Compact each chunk independently
3. Store multiple `compaction_markers` for the session

In practice, even 500-message sessions rarely exceed 100K tokens, so
single-pass compaction handles the vast majority of cases.

### Compaction flow

```
Session has 60 messages (turns 1..60)
  │
  ├── keepRecent = 10 → keep messages 51..60 intact
  │
  ├── Compact messages 1..50:
  │   1. Group by turns (user + assistant + tool calls)
  │   2. Send to LLM with structured summary prompt
  │   3. Store summary in compaction_markers table
  │
  └── When loading session for context:
      1. Load compaction_markers for session (ordered by up_to_turn)
      2. Load messages after latest marker's up_to_turn
      3. Assemble: [system prompt] + [compacted summaries] + [recent messages]
```

### Summary prompt

```
You are summarizing a coding conversation. Preserve:
- All decisions made and their rationale
- Files created, modified, or deleted (with paths)
- Errors encountered and how they were resolved
- Key code patterns or approaches chosen
- Tool calls that produced important results (not routine file reads)
- Any commitments or TODOs mentioned

Omit:
- Verbose tool call arguments and raw output
- Routine file reads that didn't change the approach
- Redundant back-and-forth on resolved issues

Use this structured format:
## Decisions
- ...
## Changes
- file: path/to/file — created/modified/deleted — brief description
- ...
## Issues & Resolutions
- ...
## Open Items
- ...
```

### Correctness safeguards

| Concern | Mitigation |
|---------|-----------|
| Tool call IDs referenced by later messages | Summary includes tool call context, not raw IDs |
| Code shown early, modified later | Summary tracks file paths and final state, not intermediate diffs |
| Compacting while session is active | Only compact sessions with no activity for 10+ minutes |
| Data loss | Original messages are **never deleted** — compaction markers are an overlay |

### Acceptance criteria

- [ ] Sessions with 50+ messages trigger compaction after 10 min idle
- [ ] Compacted summary fits within 2000 tokens
- [ ] Loading a compacted session assembles: summaries + recent messages
- [ ] Original messages remain in DB (no deletion)
- [ ] `context_curate.summarize_session` triggers manual compaction
- [ ] Compaction cost < $0.01 per session (Haiku pricing)

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 5.1 | `CompactionConfig` types, `compaction_markers` schema (already in v10) | 0.5 days |
| 5.2 | Summary generation: prompt, LLM call, structured extraction | 2 days |
| 5.3 | Automatic trigger (threshold check, idle detection, post-turn hook) | 1 day |
| 5.4 | Context assembly integration (load markers + recent messages) | 1 day |
| 5.5 | Manual trigger via `context_curate.summarize_session` | 0.5 days |

---

## 6. Gardener Agent

### Problem

The intelligence file accumulates stale data. A solo developer shouldn't
have to manually curate their agent's knowledge.

### Design

The gardener is a built-in blueprint (`_gardener`) that runs as a scheduled
job. It uses the `context_curate` tool to inspect and maintain the
intelligence file.

### Blueprint

```yaml
---
name: _gardener
description: Autonomous maintenance agent.
model: claude-3-5-haiku-20241022
tools:
  - context_curate
max_turns: 20
guardrails:
  max_cost_usd: 0.05
  filesystem_sandbox: []
schedule:
  cron: "0 3 * * *"
  trigger: cron
  enabled: true
strategy: standard
---

You are the gardener agent for a developer's coding assistant. Your job is
to maintain the intelligence file.

## Routine

1. **Check health:** Use `context_curate` with `action: review_costs`.
2. **Prune stale chunks:** Use `action: index_status`, then `action: prune_stale_chunks`.
3. **Compact sessions:** Use `action: list_sessions` to find sessions with
   50+ messages and no compaction markers. Use `action: summarize_session`
   for each (max 10 per run to stay within budget).
4. **Review skills:** Use `action: list_skills`. Report anomalies but
   do not delete.
5. **Report:** Summarize what you did and any issues found.

## Rules

- Never delete skills without prior user confirmation
- Never delete sessions — only compact them
- Prune stale chunks freely
- Keep total cost under $0.05 per run
- Skip sessions belonging to agent "_gardener" (self-exclusion)
```

### Self-exclusion

The gardener creates sessions (agent: `_gardener`). To prevent recursive
compaction, the gardener's prompt explicitly excludes `_gardener` sessions.
Additionally, `context_curate.list_sessions` accepts an `exclude_agent`
parameter:

```typescript
// In context_curate handler:
if (params.action === "list_sessions") {
  const excludeAgent = params.params?.exclude_agent as string | undefined;
  // filter out sessions where agent_name === excludeAgent
}
```

### Acceptance criteria

- [ ] `_gardener` blueprint is auto-registered on `mp init` / first `mp serve`
- [ ] Gardener runs on schedule and creates a session + job_run entry
- [ ] Gardener prunes stale chunks, compacts eligible sessions
- [ ] Gardener skips its own sessions (no recursive compaction)
- [ ] Gardener cost stays under $0.05 per run
- [ ] `mp status` shows last gardener run time and results
- [ ] Users can disable gardener via Jobs page or `PATCH /api/v1/jobs/:id`

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 6.1 | Built-in `_gardener` blueprint + auto-registration | 1 day |
| 6.2 | Self-exclusion filter in `context_curate.list_sessions` | 0.5 days |
| 6.3 | End-to-end test: gardener runs, prunes, compacts, reports | 1 day |
| 6.4 | `mp status` integration | 0.5 days |

---

## Schema additions (migration v11)

```typescript
MIGRATIONS.push({
  version: 11,
  up: (db) => {
    // FTS indexes for unified query
    db.exec(`CREATE VIRTUAL TABLE IF NOT EXISTS knowledge_fts USING fts5(
      content, context,
      content='knowledge', content_rowid='rowid',
      tokenize='porter unicode61'
    )`);

    db.exec(`CREATE VIRTUAL TABLE IF NOT EXISTS skills_fts USING fts5(
      name, description, instructions,
      content='skills', content_rowid='rowid',
      tokenize='porter unicode61'
    )`);

    // FTS sync triggers for knowledge
    db.exec(`CREATE TRIGGER IF NOT EXISTS knowledge_fts_ai AFTER INSERT ON knowledge BEGIN
      INSERT INTO knowledge_fts(rowid, content, context) VALUES (new.rowid, new.content, new.context);
    END`);
    db.exec(`CREATE TRIGGER IF NOT EXISTS knowledge_fts_ad AFTER DELETE ON knowledge BEGIN
      INSERT INTO knowledge_fts(knowledge_fts, rowid, content, context) VALUES ('delete', old.rowid, old.content, old.context);
    END`);

    // FTS sync triggers for skills
    db.exec(`CREATE TRIGGER IF NOT EXISTS skills_fts_ai AFTER INSERT ON skills BEGIN
      INSERT INTO skills_fts(rowid, name, description, instructions) VALUES (new.rowid, new.name, new.description, new.instructions);
    END`);
    db.exec(`CREATE TRIGGER IF NOT EXISTS skills_fts_ad AFTER DELETE ON skills BEGIN
      INSERT INTO skills_fts(skills_fts, rowid, name, description, instructions) VALUES ('delete', old.rowid, old.name, old.description, old.instructions);
    END`);

    // Computed views
    db.exec(`CREATE VIEW IF NOT EXISTS mp_health AS ...`);
    db.exec(`CREATE VIEW IF NOT EXISTS mp_suggestions AS ...`);
  },
});
```

---

## Implementation order

```
Phase 1: Embedding pipeline (§1)
  │       ↑ unblocks vector search for unified query
  │
  ├── Phase 2: Parallel tool execution (§2) [independent]
  │
  ├── Phase 3: Unified query engine (§3) [depends on §1 for vector leg]
  │
  ├── Phase 4: Computed views (§4) [independent]
  │
  └── Phase 5: Compaction (§5) [independent]
      │
      └── Phase 6: Gardener (§6) [depends on §5 + sprint 1 §7 context_curate]
```

Phases 2, 4, 5 can start immediately. Phase 3 benefits from §1 but
works without it (BM25 fallback). Phase 6 is the capstone.

---

## What we deliberately skip

- **Reactive triggers (SQLite write hooks → event bus)** — deferred to sprint 3
- **Self-evolving prompts** — deferred to sprint 3
- **Embeddable SQL extensions** — deferred to sprint 3
- **Cross-workspace federation** — out of scope
