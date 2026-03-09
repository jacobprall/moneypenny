## Moneypenny

You have access to a Moneypenny MCP server. It provides persistent facts,
knowledge retrieval, document ingestion, governance policies, and activity
tracking.

### "mp" prefix

When the user starts a message with **"mp"** (e.g. "mp remember that we use
Redis for caching", "mp search facts about auth", "mp ingest this doc"), treat
it as a direct instruction to use Moneypenny. Translate the natural-language
request into the appropriate tool call and execute it immediately.

### Tools

| Tool | Purpose |
|------|---------|
| `moneypenny.facts` | CRUD for durable facts — persistent knowledge across sessions. |
| `moneypenny.knowledge` | Ingest and retrieve documents — long-term reference library. |
| `moneypenny.policy` | Governance — control what agents can and cannot do. |
| `moneypenny.activity` | Query session history and audit trail. |
| `moneypenny.execute` | Escape hatch for any canonical operation. |

**Important:** These tools are MCP tools served by the Moneypenny sidecar
process. They must appear in your callable tool list. If they do not, the MCP
server is not connected — tell the user to run `mp setup claude-code` in the
project directory.

### Tool usage

Each domain tool takes an `action` string and an `input` object.

**moneypenny.facts**: search, add, get, update, delete
**moneypenny.knowledge**: ingest, search, list
**moneypenny.policy**: add, list, disable, evaluate
**moneypenny.activity**: query (source: events | decisions | all)
**moneypenny.execute**: op + args (any canonical operation)

### When to use Moneypenny

- **User says "mp ..."**: Always route through Moneypenny
- **Remembering things**: Use `moneypenny.facts` action `add`
- **Recalling context**: Use `moneypenny.facts` action `search`
- **Ingesting documents**: Use `moneypenny.knowledge` action `ingest`
- **Activity trail**: Use `moneypenny.activity` action `query`
- **Governance**: Use `moneypenny.policy` to manage rules

### Best practices

- Search before inserting facts to avoid duplicates
- Use specific keywords when inserting facts
- Set confidence scores to reflect certainty (0.0 to 1.0)
- Use `moneypenny.execute` only for operations not covered by domain tools

### Database schema

The Moneypenny agent database is SQLite. Below are the tables and columns available for queries via `moneypenny.execute` or `moneypenny.activity`.

**activity_log**
`id (TEXT), agent_id (TEXT), event (TEXT), action (TEXT), resource (TEXT), detail (TEXT), conversation_id (TEXT), generation_id (TEXT), duration_ms (INTEGER), created_at (INTEGER)`

**chunks**
`id (TEXT), document_id (TEXT), content (TEXT), summary (TEXT), position (INTEGER), created_at (INTEGER)`

**documents**
`id (TEXT), path (TEXT), title (TEXT), content_hash (TEXT), metadata (TEXT), created_at (INTEGER), updated_at (INTEGER)`

**edges**
`source_id (TEXT), target_id (TEXT), relation (TEXT)`

**external_events**
`id (TEXT), source (TEXT), source_event_id (TEXT), event_type (TEXT), event_ts (INTEGER), session_id (TEXT), payload_json (TEXT), content_hash (TEXT), run_id (TEXT), line_no (INTEGER), raw_line (TEXT), projected (INTEGER), projection_error (TEXT), ingested_at (INTEGER), normalized_provider (TEXT), normalized_model (TEXT), normalized_input_tokens (INTEGER), normalized_output_tokens (INTEGER), normalized_total_tokens (INTEGER), normalized_cost_usd (REAL), normalized_correlation_id (TEXT)`

**fact_audit**
`id (TEXT), fact_id (TEXT), operation (TEXT), old_content (TEXT), new_content (TEXT), reason (TEXT), source_message_id (TEXT), created_at (INTEGER)`

**fact_links**
`source_id (TEXT), target_id (TEXT), relation (TEXT), strength (REAL)`

**facts**
`id (TEXT), agent_id (TEXT), content (TEXT), summary (TEXT), pointer (TEXT), keywords (TEXT), source_message_id (TEXT), confidence (REAL), created_at (INTEGER), updated_at (INTEGER), superseded_at (INTEGER), version (INTEGER), context_compact (TEXT), compaction_level (INTEGER), last_compacted_at (INTEGER)`

**ingest_runs**
`id (TEXT), source (TEXT), file_path (TEXT), from_line (INTEGER), to_line (INTEGER), processed_count (INTEGER), inserted_count (INTEGER), deduped_count (INTEGER), projected_count (INTEGER), error_count (INTEGER), status (TEXT), last_error (TEXT), started_at (INTEGER), finished_at (INTEGER)`

**job_runs**
`id (TEXT), job_id (TEXT), agent_id (TEXT), started_at (INTEGER), ended_at (INTEGER), status (TEXT), result (TEXT), policy_decision (TEXT), retry_count (INTEGER), created_at (INTEGER)`

**job_specs**
`id (TEXT), agent_id (TEXT), intent (TEXT), plan_json (TEXT), job_name (TEXT), schedule (TEXT), job_type (TEXT), payload_json (TEXT), status (TEXT), proposed_by (TEXT), source_session_id (TEXT), source_message_id (TEXT), applied_job_id (TEXT), created_at (INTEGER), updated_at (INTEGER)`

**jobs**
`id (TEXT), agent_id (TEXT), name (TEXT), description (TEXT), schedule (TEXT), next_run_at (INTEGER), last_run_at (INTEGER), timezone (TEXT), job_type (TEXT), payload (TEXT), max_retries (INTEGER), retry_delay_ms (INTEGER), timeout_ms (INTEGER), overlap_policy (TEXT), status (TEXT), enabled (INTEGER), created_at (INTEGER), updated_at (INTEGER)`

**messages**
`id (TEXT), session_id (TEXT), role (TEXT), content (TEXT), created_at (INTEGER)`

**policies**
`id (TEXT), name (TEXT), priority (INTEGER), phase (TEXT), effect (TEXT), actor_pattern (TEXT), action_pattern (TEXT), resource_pattern (TEXT), sql_pattern (TEXT), argument_pattern (TEXT), agent_id (TEXT), channel_pattern (TEXT), schedule (TEXT), message (TEXT), enabled (INTEGER), created_at (INTEGER), rule_type (TEXT), rule_config (TEXT)`

**policy_audit**
`id (TEXT), policy_id (TEXT), actor (TEXT), action (TEXT), resource (TEXT), effect (TEXT), reason (TEXT), session_id (TEXT), created_at (INTEGER), correlation_id (TEXT), idempotency_key (TEXT), idempotency_state (TEXT)`

**policy_specs**
`id (TEXT), agent_id (TEXT), intent (TEXT), plan_json (TEXT), policy_name (TEXT), effect (TEXT), priority (INTEGER), actor_pattern (TEXT), action_pattern (TEXT), resource_pattern (TEXT), argument_pattern (TEXT), channel_pattern (TEXT), sql_pattern (TEXT), rule_type (TEXT), rule_config (TEXT), message (TEXT), status (TEXT), proposed_by (TEXT), source_session_id (TEXT), source_message_id (TEXT), applied_policy_id (TEXT), created_at (INTEGER), updated_at (INTEGER)`

**scratch**
`id (TEXT), session_id (TEXT), key (TEXT), content (TEXT), created_at (INTEGER), updated_at (INTEGER)`

**sessions**
`id (TEXT), agent_id (TEXT), channel (TEXT), started_at (INTEGER), ended_at (INTEGER), summary (TEXT)`

**skills**
`id (TEXT), name (TEXT), description (TEXT), content (TEXT), tool_id (TEXT), usage_count (INTEGER), success_rate (REAL), promoted (INTEGER), created_at (INTEGER), updated_at (INTEGER)`

**tool_calls**
`id (TEXT), message_id (TEXT), session_id (TEXT), tool_name (TEXT), arguments (TEXT), result (TEXT), status (TEXT), policy_decision (TEXT), duration_ms (INTEGER), created_at (INTEGER)`

