# Data Model

## Connection Model

Bun's `bun:sqlite` is synchronous on the calling fiber. There is no benefit to a "pool" of read connections — handlers grab a handle, use it synchronously, return.

We open exactly two `Database` instances per process:

```typescript
import { Database } from 'bun:sqlite';

export function openWriteDb(path: string): Database {
  const db = new Database(path, { create: true });
  db.exec("PRAGMA journal_mode = WAL");
  db.exec("PRAGMA foreign_keys = ON");
  db.exec("PRAGMA busy_timeout = 5000");
  db.exec("PRAGMA synchronous = NORMAL");
  return db;
}

export function openReadDb(path: string): Database {
  const db = new Database(path, { readonly: true });
  db.exec("PRAGMA journal_mode = WAL");
  return db;
}
```

The `writeDb` is shared by all writers (runner, custodian, watcher, work loop) and serializes naturally on the event loop. The `readDb` is shared by all readers (HTTP handlers, SSE) and never blocks on writers thanks to WAL snapshot isolation.

## Write Discipline

Every writer MUST:

1. Keep transactions under 10ms
2. Batch bulk operations (indexing, embedding) into chunks of 50–100 rows per transaction
3. Yield between batches: `await Bun.sleep(0)`
4. Never hold a transaction across an `await`
5. Never perform LLM/HTTP calls inside a transaction

Violations are caught by a debug-mode wrapper that logs warnings when transactions exceed budget.

## Schema

### Sessions

```sql
CREATE TABLE sessions (
    id              TEXT PRIMARY KEY NOT NULL,
    label           TEXT,
    status          TEXT NOT NULL DEFAULT 'active'
                    CHECK (status IN ('active','running','paused','completed','failed','archived')),
    parent_id       TEXT REFERENCES sessions(id) ON DELETE SET NULL,
    idea_id         TEXT,
    config          TEXT NOT NULL DEFAULT '{}',
    config_version  INTEGER NOT NULL DEFAULT 0,
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

### Runs

```sql
CREATE TABLE runs (
    id           TEXT PRIMARY KEY NOT NULL,
    session_id   TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    status       TEXT NOT NULL CHECK (status IN ('running','complete','failed','aborted')),
    model        TEXT,
    blueprint    TEXT,
    started_at   INTEGER NOT NULL DEFAULT (unixepoch()),
    finished_at  INTEGER,
    tokens_in    INTEGER,
    tokens_out   INTEGER,
    cost_usd     REAL,
    error        TEXT
);

CREATE INDEX idx_runs_session ON runs(session_id, started_at DESC);
```

### Messages

```sql
CREATE TABLE messages (
    id            TEXT PRIMARY KEY NOT NULL,
    session_id    TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    run_id        TEXT REFERENCES runs(id) ON DELETE SET NULL,
    seq           INTEGER NOT NULL,
    role          TEXT NOT NULL CHECK (role IN ('user','assistant','system','tool')),
    content       TEXT,
    tool_calls    TEXT,
    tool_call_id  TEXT,
    pending       INTEGER NOT NULL DEFAULT 0,
    created_at    INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE INDEX idx_messages_session ON messages(session_id, seq);
CREATE INDEX idx_messages_run     ON messages(run_id) WHERE run_id IS NOT NULL;
CREATE INDEX idx_messages_pending ON messages(session_id, seq) WHERE pending = 1;

CREATE VIRTUAL TABLE messages_fts USING fts5(content, content=messages, content_rowid=rowid);

CREATE TRIGGER messages_ai AFTER INSERT ON messages BEGIN
  INSERT INTO messages_fts(rowid, content) VALUES (NEW.rowid, NEW.content);
END;
CREATE TRIGGER messages_ad AFTER DELETE ON messages BEGIN
  INSERT INTO messages_fts(messages_fts, rowid, content) VALUES ('delete', OLD.rowid, OLD.content);
END;
```

### Events

```sql
CREATE TABLE events (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    type        TEXT NOT NULL,
    session_id  TEXT REFERENCES sessions(id) ON DELETE SET NULL,
    run_id      TEXT REFERENCES runs(id) ON DELETE SET NULL,
    blueprint   TEXT,
    detail      TEXT,
    created_at  INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE INDEX idx_events_type    ON events(type, created_at DESC);
CREATE INDEX idx_events_session ON events(session_id, id) WHERE session_id IS NOT NULL;
CREATE INDEX idx_events_recent  ON events(id DESC);
```

Retention: events older than 30 days pruned by custodian (configurable via `system.config`).

### Tabs

```sql
CREATE TABLE tabs (
    id          TEXT PRIMARY KEY NOT NULL,
    kind        TEXT NOT NULL CHECK (kind IN ('session','overview','ideas','search')),
    session_id  TEXT REFERENCES sessions(id) ON DELETE CASCADE,
    label       TEXT,
    position    INTEGER NOT NULL,
    is_active   INTEGER NOT NULL DEFAULT 0,
    opened_at   INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE INDEX idx_tabs_position ON tabs(position);
CREATE UNIQUE INDEX idx_tabs_one_active ON tabs(is_active) WHERE is_active = 1;
```

### Code Index (carried from v1)

```sql
CREATE TABLE code_chunks (
    id            TEXT PRIMARY KEY NOT NULL,
    file_path     TEXT NOT NULL,
    chunk_index   INTEGER NOT NULL,
    content       TEXT NOT NULL,
    language      TEXT,
    symbol_name   TEXT,
    start_line    INTEGER,
    end_line      INTEGER,
    embedding     BLOB,
    embedding_dim INTEGER,                    -- explicit for migration safety
    updated_at    INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE INDEX idx_code_chunks_file ON code_chunks(file_path);

CREATE TABLE file_tree (
    path        TEXT PRIMARY KEY NOT NULL,
    is_dir      INTEGER NOT NULL DEFAULT 0,
    size_bytes  INTEGER,
    language    TEXT,
    updated_at  INTEGER NOT NULL DEFAULT (unixepoch())
);
```

### Knowledge

```sql
CREATE TABLE skills (
    id                TEXT PRIMARY KEY NOT NULL,
    name              TEXT NOT NULL UNIQUE,
    description       TEXT NOT NULL,
    instructions      TEXT,
    confidence        REAL NOT NULL DEFAULT 0.5,
    source_session_id TEXT REFERENCES sessions(id) ON DELETE SET NULL,
    created_at        INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE TABLE conventions (
    id                TEXT PRIMARY KEY NOT NULL,
    name              TEXT NOT NULL UNIQUE,
    category          TEXT NOT NULL,
    description       TEXT NOT NULL,
    confidence        REAL NOT NULL DEFAULT 0.5,
    source_session_id TEXT REFERENCES sessions(id) ON DELETE SET NULL,
    created_at        INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE TABLE session_pointers (
    id          TEXT PRIMARY KEY NOT NULL,
    session_id  TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    key         TEXT NOT NULL,
    phrase      TEXT NOT NULL,
    pinned      INTEGER NOT NULL DEFAULT 0,
    archived    INTEGER NOT NULL DEFAULT 0,
    created_at  INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE INDEX idx_pointers_session ON session_pointers(session_id);
CREATE INDEX idx_pointers_active  ON session_pointers(pinned DESC, created_at DESC) WHERE archived = 0;
```

### Policies

Policies live as `.toml` files in `~/.moneypenny/policies/` and `<repo>/.moneypenny/policies/`. The runtime syncs them into a queryable table for fast enforcement:

```sql
CREATE TABLE policies (
    id           TEXT PRIMARY KEY NOT NULL,
    name         TEXT NOT NULL UNIQUE,
    effect       TEXT NOT NULL CHECK (effect IN ('deny','warn','allow')),
    description  TEXT NOT NULL,
    conditions   TEXT,                  -- JSON
    enabled      INTEGER NOT NULL DEFAULT 1,
    source_path  TEXT NOT NULL,         -- file watched for changes
    updated_at   INTEGER NOT NULL DEFAULT (unixepoch())
);
```

Sync is one-way (file → DB). UI edits open the file, not the table.

### Schedules

Replaces the v1 `jobs` table. State for blueprints with `trigger_on: schedule`:

```sql
CREATE TABLE schedules (
    id              TEXT PRIMARY KEY NOT NULL,
    blueprint       TEXT NOT NULL,
    cron_expr       TEXT NOT NULL,
    enabled         INTEGER NOT NULL DEFAULT 1,
    last_run_at     INTEGER,
    last_session_id TEXT REFERENCES sessions(id) ON DELETE SET NULL,
    next_run_at     INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE INDEX idx_schedules_due ON schedules(next_run_at) WHERE enabled = 1;
```

The blueprint registry populates this table on load. The runtime's scheduler ticks every 30s, finds due rows, and calls `actions.launchAgent` for each.

### Work Queue

```sql
CREATE TABLE work_queue (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    type          TEXT NOT NULL,
    session_id    TEXT,
    payload       TEXT,
    created_at    INTEGER NOT NULL DEFAULT (unixepoch()),
    processed_at  INTEGER,
    error         TEXT
);

CREATE INDEX idx_work_pending ON work_queue(type, processed_at) WHERE processed_at IS NULL;
```

Retention: processed rows pruned after 7 days by custodian.

### Config

```sql
CREATE TABLE config (
    key         TEXT PRIMARY KEY NOT NULL,
    value       TEXT NOT NULL,
    updated_at  INTEGER NOT NULL DEFAULT (unixepoch())
);
```

## Views

```sql
CREATE VIEW v_session_cost AS
SELECT session_id,
       COALESCE(SUM(cost_usd), 0)   AS total_cost_usd,
       COALESCE(SUM(tokens_in), 0)  AS total_tokens_in,
       COALESCE(SUM(tokens_out), 0) AS total_tokens_out,
       COUNT(*)                     AS run_count
FROM runs
GROUP BY session_id;

CREATE VIEW v_cost_today AS
SELECT COALESCE(SUM(cost_usd), 0) AS total,
       COUNT(DISTINCT session_id) AS sessions,
       COALESCE(SUM(tokens_in), 0) AS tokens_in,
       COALESCE(SUM(tokens_out), 0) AS tokens_out
FROM runs
WHERE date(started_at, 'unixepoch') = date('now');

CREATE VIEW v_health AS
SELECT json_object(
  'sessions_total',    (SELECT COUNT(*) FROM sessions),
  'sessions_active',   (SELECT COUNT(*) FROM sessions WHERE status IN ('active','running','paused')),
  'sessions_running',  (SELECT COUNT(*) FROM sessions WHERE status = 'running'),
  'runs_total',        (SELECT COUNT(*) FROM runs),
  'messages_total',    (SELECT COUNT(*) FROM messages),
  'chunks_total',      (SELECT COUNT(*) FROM code_chunks),
  'work_pending',      (SELECT COUNT(*) FROM work_queue WHERE processed_at IS NULL),
  'work_failed',       (SELECT COUNT(*) FROM work_queue WHERE error IS NOT NULL)
) AS health;
```

## Concurrency Rules

| Field | Writers | Strategy |
|-------|---------|----------|
| `messages` | runner only (assistant/tool); UI inject (user) | append-only, never updated |
| `sessions.status` | runner only | direct UPDATE |
| `sessions.config` | runner + UI | optimistic (config_version) |
| `sessions.last_active_at` | many | direct UPDATE; not authoritative |
| `runs` | runner only | append on start, single UPDATE on finish |
| `events` | many | append-only |
| `code_chunks` | watcher + indexer | UPSERT by id |
| `tabs` | UI | direct CRUD |

The `config_version` discipline prevents lost updates when both runner and UI try to mutate config concurrently. The runner reads version, applies change, writes with `WHERE config_version = ?`; if zero rows affected, re-reads and retries.

## Migration Strategy

`packages/db/sql/v2/` is a **new flat migration set**. Because v1 schema diverges substantially, v2 ships as:

1. New install: applies `v2/*.sql` in order (one numbered file per concern)
2. Existing install: a one-time migration script reads v1 data, transforms, and writes into v2 schema. v1 db is backed up to `moneypenny.v1.db` before transformation.

The migration script is in `packages/db/src/migrate-v1-to-v2.ts` and runs idempotently when `schema_version` shows v1. v1's incremental migration system is preserved for v1 → v1.x transitions but unused going forward.

## What Lives Where

| Data | Storage | Why |
|------|---------|-----|
| Sessions, runs, messages, events, tabs | SQLite | Runtime state, queryable, FTS, transactional |
| Code chunks, embeddings, file tree | SQLite | Indexed, bulk-updateable |
| Skills, conventions, pointers | SQLite | Extracted knowledge, queryable |
| Schedules | SQLite | Cron evaluator needs efficient `WHERE next_run_at <= now` |
| Blueprints | Filesystem `.md` | Human-authored, version-controlled |
| Ideas | Filesystem `.md` | Human-authored, arbitrary frontmatter |
| Policies | Filesystem `.toml` → SQLite (synced) | Authored as files, queried as table |
| Config (models, MCP servers, retention) | SQLite `config` | Runtime settings |
