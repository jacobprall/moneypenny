# Sessions

## Definition

A **Session** is the atomic unit of work: a long-lived, mutable conversation stream (messages organized into runs) with an execution context (cwd, blueprint, tools, permissions). Every interaction is a session.

## Schema

```sql
CREATE TABLE sessions (
    id              TEXT PRIMARY KEY NOT NULL,
    label           TEXT,
    status          TEXT NOT NULL DEFAULT 'active'
                    CHECK (status IN ('active','running','paused','completed','failed','archived')),
    parent_id       TEXT REFERENCES sessions(id) ON DELETE SET NULL,
    idea_id         TEXT,                       -- filename of source idea (no FK; ideas are filesystem)
    config          TEXT NOT NULL DEFAULT '{}', -- JSON, mutable
    config_version  INTEGER NOT NULL DEFAULT 0, -- optimistic concurrency for config writes
    created_at      INTEGER NOT NULL DEFAULT (unixepoch()),
    last_active_at  INTEGER NOT NULL DEFAULT (unixepoch()),
    completed_at    INTEGER,
    failed_at       INTEGER,
    archived_at     INTEGER
);

CREATE INDEX idx_sessions_status ON sessions(status, last_active_at DESC);
CREATE INDEX idx_sessions_parent ON sessions(parent_id) WHERE parent_id IS NOT NULL;
CREATE INDEX idx_sessions_idea   ON sessions(idea_id) WHERE idea_id IS NOT NULL;

CREATE VIRTUAL TABLE sessions_fts USING fts5(label, content=sessions, content_rowid=rowid);
```

`last_active_at` updates on:
- new message inserted (user, assistant, tool)
- status transition
- config mutation
- explicit user activity (tab focus, inject)

## Config

```typescript
interface SessionConfig {
  cwd?: string;                 // working directory, mutable
  blueprint?: string;           // resolved blueprint name (snapshot at create time)
  blueprintPath?: string;       // resolved absolute path (for hot-reload tracking)
  model?: string;               // override
  tools?: string[];             // tool whitelist; null = all permitted by blueprint
  permissions?: Permissions;    // see 02-blueprints.md
  strategy?: 'autonomous' | 'hitl' | 'review';
  maxTurns?: number;
}
```

The JSON blob is **owned by the runtime** for that session. UI mutations go through `sessions.updateConfig`, which uses `config_version` for optimistic concurrency:

```sql
UPDATE sessions
SET config = ?, config_version = config_version + 1, last_active_at = unixepoch()
WHERE id = ? AND config_version = ?;
-- 0 rows affected → caller retries with fresh version
```

## Runs

A **Run** is one agent invocation. One run produces N messages (assistant text + tool calls + tool results). Runs group messages for rendering and accounting.

```sql
CREATE TABLE runs (
    id              TEXT PRIMARY KEY NOT NULL,
    session_id      TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    status          TEXT NOT NULL CHECK (status IN ('running','complete','failed','aborted')),
    model           TEXT,
    blueprint       TEXT,                       -- snapshot at run start
    started_at      INTEGER NOT NULL DEFAULT (unixepoch()),
    finished_at     INTEGER,
    tokens_in       INTEGER,
    tokens_out      INTEGER,
    cost_usd        REAL,
    error           TEXT
);

CREATE INDEX idx_runs_session ON runs(session_id, started_at DESC);
```

Messages link to runs via `messages.run_id`.

## Messages

```sql
CREATE TABLE messages (
    id              TEXT PRIMARY KEY NOT NULL,
    session_id      TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    run_id          TEXT REFERENCES runs(id) ON DELETE SET NULL,
    seq             INTEGER NOT NULL,           -- monotonic per session, append-only
    role            TEXT NOT NULL CHECK (role IN ('user','assistant','system','tool')),
    content         TEXT,
    tool_calls      TEXT,                       -- JSON array (assistant role)
    tool_call_id    TEXT,                       -- for tool role responses
    pending         INTEGER NOT NULL DEFAULT 0, -- 1 = injected, not yet consumed by runner
    created_at      INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE INDEX idx_messages_session ON messages(session_id, seq);
CREATE INDEX idx_messages_run     ON messages(run_id) WHERE run_id IS NOT NULL;
CREATE INDEX idx_messages_pending ON messages(session_id, seq) WHERE pending = 1;

CREATE VIRTUAL TABLE messages_fts USING fts5(content, content=messages, content_rowid=rowid);
```

`seq` is the message-level sequence (always increments). `run_id` groups messages from the same agent invocation.

## Lifecycle

```
                  user message or
                  agent.launch                 needs human
   ┌────────┐    ───────────────→  ┌────────┐  input  ┌────────┐
   │ active │                      │running │ ──────→ │ paused │
   └────────┘ ←───────────────     └────────┘ ←────── └────────┘
                  run completes,        │     user injects
                  loop yields           │  ↓ unhandled error
                                        ▼
                                   ┌────────┐
                                   │ failed │
                                   └────────┘
                                        │
                                  user/auto archive
                                        │ + custodian extracts knowledge
                                        ▼
   ┌──────────┐  ──────────────→  ┌──────────┐
   │completed │                   │ archived │
   └──────────┘                   └──────────┘
        ↑
        └─ user explicit "complete"
           or auto after extended inactivity (configurable)
```

| State | Meaning |
|-------|---------|
| `active` | session exists, no run in flight, accepts user input |
| `running` | a run is executing (LLM call in flight or tool executing) |
| `paused` | run yielded for HITL or explicit `request_human_input`; resumes on user message |
| `completed` | user-marked done; readable; not yet mined |
| `failed` | unhandled error in run; preserved for inspection; can be resumed or archived |
| `archived` | knowledge extracted, hidden from default views |

Permitted transitions:

| From → To | Trigger |
|-----------|---------|
| active → running | first message of a run starts |
| running → active | run completes, no pause requested |
| running → paused | HITL checkpoint or `request_human_input` tool |
| running → failed | unhandled error |
| paused → running | user injects message → resumes |
| active → completed | explicit user action |
| completed → active | "reopen" |
| {completed, failed, paused} → archived | user action or custodian sweep |
| archived → * | none (terminal) |

## Input Injection

Two phases:

**Write phase** (always succeeds):

```sql
INSERT INTO messages (id, session_id, seq, role, content, pending)
VALUES (?, ?, ?, 'user', ?, 1);
```

**Wake phase**:
- if `status = 'paused'` → set to `running`, runtime drains pending messages on next loop tick
- if `status = 'active'` → set to `running`, runtime starts a new run
- if `status = 'running'` → message is queued; the running loop reads `WHERE pending = 1` between turns and clears the flag

Pending semantics give the agent atomic visibility into queued user input without losing race-condition messages.

## Runs and Token Budget

Long-running sessions accumulate context. The runtime applies live compaction:

- before each run, if message count > `compact_after_turns`, custodian summarizes the older half into a single `system` message and marks original messages compacted (preserved but excluded from prompt assembly)
- token budget per run is bounded by `model.contextWindow * 0.7` (room for output)
- if a single run would exceed budget even after compaction, runtime pauses with a `budget_exceeded` reason

## Parent / Child

A session can spawn child sessions via the `spawn_agent` tool. The parent's stream gets a `child_spawn` event; the message at that point shows a child status card (rendered inline at the spawn point in the conversation).

Rules:
- `parent_id` set on child at creation
- on parent delete: `ON DELETE SET NULL` (children survive, become roots)
- on parent archive: children unaffected
- max depth: 5 (configurable; prevents runaway recursion)
- children inherit parent's permissions; can be narrower, never broader (see 02-blueprints.md)
- when a child completes/fails, an event of type `child.{status}` is emitted on the parent's session channel

## Costs

Aggregated at the run level (`runs.cost_usd, tokens_in, tokens_out`). Session-level cost is a derived view:

```sql
CREATE VIEW v_session_cost AS
SELECT session_id,
       COALESCE(SUM(cost_usd), 0)   AS total_cost_usd,
       COALESCE(SUM(tokens_in), 0)  AS total_tokens_in,
       COALESCE(SUM(tokens_out), 0) AS total_tokens_out,
       COUNT(*)                     AS run_count
FROM runs
GROUP BY session_id;
```

## Tabs

Tabs are server-side persisted (see `05-data.md` for the `tabs` table). Each tab references a session id (or a special view: `overview`, `ideas`, `search`). The "open tabs" set survives restarts and can be queried via API.

A single session can be open in multiple tabs (allowed; harmless — both subscribe to the same SSE channel).

## Knowledge Extraction

On transition to `archived`, the custodian:

1. Generates a final label if absent
2. Extracts pointers (key decisions, patterns)
3. Detects new skills (reusable procedures, only on successfully completed sessions)
4. Updates conventions (if new patterns emerged across multiple sessions)
5. Compacts messages: original kept, but excluded from default rendering

`completed` sessions are NOT auto-archived. Archive is explicit (user) or custodian sweep (configurable, default 30 days inactivity post-completion).

## Scope

Single `config.cwd` in v2. The agent's "world" is that directory. Tools resolve paths relative to it. Multi-root (cross-repo search within a session) is deferred to a later spec.

If `cwd` doesn't exist when the session resumes, runtime emits a `cwd_missing` warning event and pauses the session for user remediation.
