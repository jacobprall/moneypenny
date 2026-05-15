# Schema Additions

### Migration strategy

The existing migration system uses `SCHEMA_VERSION` (currently 9) with a
`MIGRATIONS` array in `schema.ts`. Sprint 1 adds migration **version 10**.

> **Implementation note:** The existing migration system uses
> `{ version: number; sql: string }` — a plain SQL string, not a callback.
> The migration below MUST be written as a SQL string to match the existing
> pattern. The `up: (db) => {}` pseudo-code below is for readability only.

```typescript
MIGRATIONS.push({
  version: 10,
  sql: `
    -- Sub-agent invocation log
    CREATE TABLE IF NOT EXISTS subagent_runs (
      id TEXT PRIMARY KEY,
      parent_session_id TEXT,
      child_session_id TEXT,
      blueprint TEXT NOT NULL,
      input TEXT,
      output TEXT,
      status TEXT NOT NULL DEFAULT 'running',
      started_at INTEGER NOT NULL,
      ended_at INTEGER,
      cost_usd REAL DEFAULT 0,
      created_at INTEGER NOT NULL
    );

    -- Agents table additions
    ALTER TABLE agents ADD COLUMN strategy TEXT DEFAULT 'standard';
    ALTER TABLE agents ADD COLUMN memory_config TEXT;
    ALTER TABLE agents ADD COLUMN guardrails TEXT;
    ALTER TABLE agents ADD COLUMN sub_agents TEXT;

    -- Hooks table: recreate with updated phases and declarative columns.
    -- Existing hooks used Function()-based scripts which are being removed.
    -- Phase values change from 'pre:validation','pre:injection','post:transform'
    -- to 'pre_tool','post_tool','pre_llm','post_llm' to match HookPipeline.
    CREATE TABLE IF NOT EXISTS hooks_new (
      id TEXT PRIMARY KEY,
      name TEXT NOT NULL,
      phase TEXT NOT NULL CHECK(phase IN ('pre_tool','post_tool','pre_llm','post_llm')),
      priority INTEGER DEFAULT 0,
      condition TEXT,
      action TEXT,
      enabled INTEGER DEFAULT 1,
      created_at INTEGER NOT NULL,
      updated_at INTEGER NOT NULL
    );
    INSERT OR IGNORE INTO hooks_new (id, name, phase, priority, enabled, created_at, updated_at)
      SELECT id, name,
        CASE phase
          WHEN 'pre:validation' THEN 'pre_tool'
          WHEN 'pre:injection' THEN 'pre_llm'
          WHEN 'post:transform' THEN 'post_llm'
          ELSE phase
        END,
        priority, enabled, created_at, updated_at
      FROM hooks;
    DROP TABLE hooks;
    ALTER TABLE hooks_new RENAME TO hooks;
  `,
});
```

### Resolved inconsistencies

The original spec draft had several issues caught during implementation
review. These are the resolutions applied:

| Issue | Original spec | Resolution |
|-------|--------------|------------|
| `jobs.type` column | Added `ALTER TABLE jobs ADD COLUMN type TEXT DEFAULT 'agents.run'` | **Removed.** The `jobs` table already has `operation TEXT NOT NULL` which stores identical values (`'agents.run'`). The job system spec's `JobOperation` type maps directly to the existing `operation` column. No new column needed. |
| `compaction_markers` table | `CREATE TABLE IF NOT EXISTS compaction_markers (...)` | **Removed.** This table already exists in the base `SCHEMA_SQL` and was extended with `session_id` in migration v5. |
| Migration format | Used `up: (db) => { db.exec(...) }` callback syntax | **Changed to SQL string.** The existing `Migration` interface is `{ version: number; sql: string }`. |
| Hook `phase` values | Added `condition` and `action` columns but kept old CHECK constraint | **Table recreated.** Old phases (`'pre:validation'`, etc.) are incompatible with the `HookPipeline` phases (`'pre_tool'`, etc.). SQLite doesn't support `ALTER CONSTRAINT`, so we recreate the table with the correct CHECK and migrate existing rows with a phase mapping. |
| Hook `script NOT NULL` | Added new columns alongside mandatory `script` | **Resolved by table recreation.** The `script` column is dropped entirely since `Function()` constructor hooks are removed. Replaced by `condition` and `action` JSON columns. |

### SCHEMA_SQL update

`SCHEMA_VERSION` bumps to 10. The monolithic `SCHEMA_SQL` is also updated
to include these columns/tables for fresh installs. `validateSchemaConsistency()`
ensures they stay in sync.

The `hooks` table definition in `SCHEMA_SQL` must be updated to match the
new shape (new phases, `condition`/`action` instead of `script`/`match_pattern`).

The `agents` table definition in `SCHEMA_SQL` must include the new columns
(`strategy`, `memory_config`, `guardrails`, `sub_agents`).

### Backward compatibility

All new columns have defaults. Existing databases open and migrate
transparently. No data loss. The existing `operation` column on `jobs`
continues to work as-is — the job system spec extends its allowed values
without schema changes.

Existing `hooks` rows are migrated with a phase mapping. Any rows with
`Function()`-based `script` content will lose the script (it is not
migrated) and a warning should be logged at startup. These hooks must be
re-created as declarative hooks.
