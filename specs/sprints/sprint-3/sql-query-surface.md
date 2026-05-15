# Stable SQL Query Surface

> **Renamed from "Embeddable SQL Extension"** — the previous draft
> described custom SQL functions registered via `db.function()`, but
> these only work when the DB is opened through Bun. External tools
> (Datasette, DBeaver, `sqlite3` CLI) won't have access to custom
> functions. The spec now clearly separates what's portable (views)
> from what's Bun-only (functions).

### Problem

The intelligence file is a SQLite database, but its schema is an
implementation detail. External tools can query it, but they need to
understand internal table structures. A stable view layer provides a
documented API.

### Portable layer: SQL views (works everywhere)

These views work in any SQLite client:

```sql
CREATE VIEW IF NOT EXISTS mp_agent_activity AS
SELECT
  a.name AS agent,
  COUNT(DISTINCT s.id) AS sessions,
  SUM(COALESCE(json_extract(s.cost, '$.totalCost'), 0)) AS total_cost_usd,
  SUM(COALESCE(json_extract(s.cost, '$.inputTokens'), 0)) AS total_input_tokens,
  SUM(COALESCE(json_extract(s.cost, '$.outputTokens'), 0)) AS total_output_tokens,
  MAX(s.last_activity_at) AS last_active
FROM agents a
LEFT JOIN sessions s ON s.agent_name = a.name
GROUP BY a.name;

CREATE VIEW IF NOT EXISTS mp_tool_usage AS
SELECT
  json_extract(m.content, '$.name') AS tool_name,
  COUNT(*) AS call_count,
  AVG(COALESCE(json_extract(m.metadata, '$.durationMs'), 0)) AS avg_duration_ms,
  SUM(CASE WHEN json_extract(m.metadata, '$.success') = 1 THEN 1 ELSE 0 END) AS success_count,
  SUM(CASE WHEN json_extract(m.metadata, '$.success') = 0 THEN 1 ELSE 0 END) AS failure_count
FROM messages m
WHERE m.role = 'tool'
GROUP BY tool_name;

CREATE VIEW IF NOT EXISTS mp_daily_cost AS
SELECT
  DATE(last_activity_at, 'unixepoch') AS day,
  COUNT(*) AS sessions,
  SUM(COALESCE(json_extract(cost, '$.totalCost'), 0)) AS cost_usd,
  SUM(COALESCE(json_extract(cost, '$.inputTokens'), 0)) AS input_tokens,
  SUM(COALESCE(json_extract(cost, '$.outputTokens'), 0)) AS output_tokens
FROM sessions
GROUP BY day
ORDER BY day DESC;

CREATE VIEW IF NOT EXISTS mp_governance_log AS
SELECT
  ge.id,
  ge.session_id,
  ge.tool_name,
  ge.effect,
  ge.policy_name,
  ge.reason,
  ge.args_snapshot,
  DATETIME(ge.created_at, 'unixepoch') AS created_at_iso
FROM gov_events ge
ORDER BY ge.created_at DESC;

CREATE VIEW IF NOT EXISTS mp_knowledge AS
SELECT
  k.id,
  k.context,
  k.content,
  k.source,
  DATETIME(k.created_at, 'unixepoch') AS created_at_iso,
  LENGTH(k.embedding) > 0 AS has_embedding
FROM knowledge k
ORDER BY k.created_at DESC;
```

### Bun-only layer: custom SQL functions

These functions are registered via `db.function()` and only work when the
DB is opened through Bun (moneypenny process, `mp query` command):

```typescript
function installFunctions(db: Database): void {
  db.function("mp_token_cost", {
    args: 3,
    handler: (model: string, inputTokens: number, outputTokens: number) => {
      return calculateCost({ model, inputTokens, outputTokens });
    },
  });

  db.function("mp_time_ago", {
    args: 1,
    handler: (ts: number) => {
      const diff = Date.now() / 1000 - ts;
      if (diff < 60) return "just now";
      if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
      if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
      return `${Math.floor(diff / 86400)}d ago`;
    },
  });
}
```

**Excluded:** `mp_search()` as a SQL function. Hybrid search requires
async embedding generation and multi-database access — it doesn't fit
the synchronous SQL function model. Use the `code_search` tool or the
`/api/v1/search` endpoint instead.

### FTS integration

Add FTS5 index for message full-text search:

```sql
CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
  content,
  content='messages',
  content_rowid='rowid',
  tokenize='porter unicode61'
);

CREATE TRIGGER IF NOT EXISTS messages_fts_ai AFTER INSERT ON messages BEGIN
  INSERT INTO messages_fts(rowid, content) VALUES (new.rowid, new.content);
END;

CREATE TRIGGER IF NOT EXISTS messages_fts_ad AFTER DELETE ON messages BEGIN
  INSERT INTO messages_fts(messages_fts, rowid, content) VALUES ('delete', old.rowid, old.content);
END;
```

This allows searching across message history via plain SQL:

```sql
SELECT m.*, mf.rank
FROM messages_fts mf
JOIN messages m ON m.rowid = mf.rowid
WHERE messages_fts MATCH 'authentication'
ORDER BY mf.rank
LIMIT 20;
```

### `mp query` command

A new CLI command to run SQL against the intelligence file with
custom functions pre-loaded:

```bash
# Views work in any tool
mp query "SELECT * FROM mp_agent_activity"

# Custom functions work via mp query
mp query "SELECT agent, mp_time_ago(last_active) FROM mp_agent_activity"

# Export to CSV
mp query --csv "SELECT * FROM mp_daily_cost" > costs.csv

# Pipe-friendly JSON output
mp query --json "SELECT * FROM mp_health"
```

### Documentation: `SCHEMA.md`

The extension ships with a generated `SCHEMA.md` that documents every
view and function. This serves as the API contract:

```markdown
# moneypenny Intelligence File — SQL API

## Portable Views (work in any SQLite client)

### mp_agent_activity
| Column | Type | Description |
| ...

### mp_health (from sprint 2)
...

## Bun-only Functions (require mp query or moneypenny process)

### mp_token_cost(model, input_tokens, output_tokens)
Returns cost in USD. Uses moneypenny's internal pricing table.

### mp_time_ago(unix_timestamp)
Returns human-readable relative time string.
```

### Acceptance criteria

- [ ] All views return correct data when queried from `sqlite3` CLI
- [ ] Views survive schema migrations (created in migration, not at runtime)
- [ ] `mp query` command runs SQL with custom functions loaded
- [ ] `mp query --csv` and `--json` output modes work
- [ ] `SCHEMA.md` is auto-generated and accurate
- [ ] External tools (Datasette) can query views without custom functions

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 4.1 | Stable views (agent_activity, tool_usage, daily_cost, governance_log, knowledge) | 1.5 days |
| 4.2 | Custom SQL functions (mp_token_cost, mp_time_ago) | 0.5 days |
| 4.3 | FTS5 index + sync triggers for messages | 0.5 days |
| 4.4 | `mp query` CLI command with --csv and --json output | 1 day |
| 4.5 | `SCHEMA.md` generation script | 0.5 days |
| 4.6 | Integration tests: views correctness, external tool compatibility | 0.5 days |
