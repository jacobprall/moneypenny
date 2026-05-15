# Custodian Agent

### Problem

The intelligence file accumulates stale data. A solo developer shouldn't
have to manually curate their agent's knowledge or worry about database
size. The session lifecycle tools (§5) and maintenance actions exist as
standalone `context_curate` operations — anyone can call them from the
CLI or from any agent. What's missing is an autonomous agent that decides
**when** to run **which** tools based on the current state of the database.

### Design

The custodian is a built-in blueprint (`_custodian`) that runs as a
scheduled job. It has access to the `context_curate` tool and uses
judgment to drive the session lifecycle and perform maintenance. The
tools work without the custodian — `mp sessions archive <id>` calls the
same business logic directly. The custodian automates the decision layer.

### Blueprint

```yaml
---
name: _custodian
description: Autonomous database maintenance agent.
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

You are the custodian agent for a developer's coding assistant. Your job is
to maintain the intelligence file by running the right maintenance tools at
the right time.

## Routine

1. **Check health:** Use `context_curate` with `action: review_costs`.

2. **Compact eligible sessions:** Use `action: list_sessions` (with
   `exclude_agent: "_custodian"`) to find sessions with 50+ messages and
   no compaction markers. Use `action: summarize_session` for each
   (max 10 per run to stay within budget). Summaries are automatically
   embedded after compaction.

3. **Archive old sessions:** Use `action: list_sessions` with
   `filter: "compacted_older_than_30d"` to find sessions eligible for
   archival. Use `action: archive_session` for each. This writes
   messages to JSONL and marks the session for purge after the hold period.

4. **Purge held sessions:** Use `action: list_sessions` with
   `filter: "archived_older_than_7d"` to find sessions past the purge
   hold period. Use `action: purge_session` for each. This deletes raw
   messages from SQLite.

5. **Check DB size:** If the database exceeds the size threshold, archive
   additional warm sessions starting with the oldest.

6. **Prune stale chunks:** Use `action: index_status`, then
   `action: prune_stale_chunks`.

7. **Review skills:** Use `action: list_skills`. Report anomalies but
   do not delete.

8. **Report:** Summarize what you did and any issues found.

## Rules

- Never delete skills without prior user confirmation
- Never purge sessions that haven't been archived first
- Never archive sessions that haven't been compacted first
- Prune stale chunks freely
- Keep total cost under $0.05 per run
- Skip sessions belonging to agent "_custodian" (self-exclusion)
```

### Custodian configuration

The custodian's thresholds are configurable via `.mp/config.yaml`:

```yaml
custodian:
  enabled: true
  schedule: "0 3 * * *"
  compact_after_messages: 40
  compact_idle_minutes: 10
  embed_summaries: true
  archive_after_days: 30
  archive_path: ".mp/archives/"
  archive_format: "jsonl.gz"
  purge_after_archive_days: 7
  max_db_size_mb: 500
  max_cost_usd: 0.05
```

### `context_curate` extensions

New actions added to the `context_curate` tool. These are standalone
operations — any agent or CLI command can use them, not just the
custodian:

| Action | Description |
|--------|-------------|
| `archive_session` | Archive a session's messages to JSONL, verify checksum |
| `purge_session` | Delete raw messages for an archived session past hold period |
| `list_sessions` (extended) | New filters: `compacted_older_than_30d`, `archived_older_than_7d`, `hot`, `warm`, `cold` |
| `db_size` | Returns current database file size in MB |

### Self-exclusion

The custodian creates sessions (agent: `_custodian`). To prevent recursive
processing, the custodian's prompt explicitly excludes `_custodian`
sessions. Additionally, `context_curate.list_sessions` accepts an
`exclude_agent` parameter:

```typescript
if (params.action === "list_sessions") {
  const excludeAgent = params.params?.exclude_agent as string | undefined;
  // filter out sessions where agent_name === excludeAgent
}
```

### Acceptance criteria

- [ ] `_custodian` blueprint is auto-registered on `mp init` / first `mp serve`
- [ ] Custodian runs on schedule and creates a session + job_run entry
- [ ] Custodian compacts eligible sessions (summary + embed)
- [ ] Custodian archives sessions older than threshold to JSONL
- [ ] Custodian purges messages for sessions past the hold period
- [ ] Custodian triggers early archival when DB size exceeds threshold
- [ ] Custodian skips its own sessions (no recursive processing)
- [ ] Custodian cost stays under $0.05 per run
- [ ] `mp status` shows last custodian run time and results
- [ ] Users can disable custodian via Jobs page or `PATCH /api/v1/jobs/:id`
- [ ] Custodian config is customizable via `.mp/config.yaml`
- [ ] All custodian operations also work via CLI without the agent running

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 6.1 | Built-in `_custodian` blueprint + auto-registration | 1 day |
| 6.2 | `context_curate` extensions: `archive_session`, `purge_session`, `db_size` | 1.5 days |
| 6.3 | Self-exclusion filter in `context_curate.list_sessions` | 0.5 days |
| 6.4 | End-to-end test: custodian runs full lifecycle | 1.5 days |
| 6.5 | `mp status` integration | 0.5 days |
