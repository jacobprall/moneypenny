//! Shared Moneypenny documentation content — single source of truth.

/// Overview paragraph for Moneypenny.
pub const OVERVIEW: &str = "You have access to a Moneypenny MCP server. It provides persistent facts, \
knowledge retrieval, document ingestion, governance policies, and activity tracking.";

/// "mp" prefix instructions.
pub const MP_PREFIX: &str = r#"When the user starts a message with **"mp"** (e.g. "mp remember that we use
Redis for caching", "mp search facts about auth", "mp ingest this doc"), treat
it as a direct instruction to use Moneypenny. Translate the natural-language
request into the appropriate tool call and execute it immediately."#;

/// Tools table in markdown format.
pub const TOOLS_TABLE: &str = r#"| Tool | Purpose |
|------|---------|
| `moneypenny_facts` | CRUD for durable facts — persistent knowledge across sessions. |
| `moneypenny_knowledge` | Ingest and retrieve documents — long-term reference library. |
| `moneypenny_policy` | Governance — control what agents can and cannot do. |
| `moneypenny_activity` | Query session history and audit trail. |
| `moneypenny_execute` | Escape hatch for any canonical operation. |"#;

/// Short tool usage summary (one line per tool).
pub const TOOL_USAGE_SHORT: &str = r#"**moneypenny_facts**: search, add, get, update, delete
**moneypenny_knowledge**: ingest, search, list
**moneypenny_policy**: add, list, disable, evaluate
**moneypenny_activity**: query (source: events | decisions | all)
**moneypenny_execute**: op + args (any canonical operation)"#;

/// Detailed tool usage (per-tool action/input docs).
pub const TOOL_USAGE_DETAILED: &str = r#"Each domain tool takes an `action` string and an `input` object.

### moneypenny_facts
- `search`: `{query, limit?}` — hybrid search across facts
- `add`: `{content, summary?, keywords?, confidence?}` — store a new fact
- `get`: `{id}` — retrieve a fact by ID
- `update`: `{id, content, summary?}` — update an existing fact
- `delete`: `{id, reason?}` — remove a fact

### moneypenny_knowledge
- `ingest`: `{path?, content?, title?}` — add a document (pass `path` as an HTTP URL to fetch a webpage, or provide `content` directly)
- `search`: `{query, limit?}` — search ingested documents
- `list`: `{}` — list all documents

### moneypenny_policy
- `add`: `{name, effect?, priority?, action_pattern?, resource_pattern?, sql_pattern?, message?}` — create a policy
- `list`: `{enabled?, effect?, limit?}` — list policies
- `disable`: `{id}` — disable a policy
- `evaluate`: `{actor, action, resource}` — test if action is allowed

### moneypenny_activity
- `query`: `{source?, event?, action?, resource?, query?, limit?}` — query events and decisions

### moneypenny_execute
- `op`: canonical operation name (e.g. `job.create`, `ingest.events`)
- `args`: operation-specific arguments"#;

/// When to use Moneypenny.
pub const WHEN_TO_USE: &str = r#"- **User says "mp ..."**: Always route through Moneypenny
- **Remembering things**: Use `moneypenny_facts` action `add`
- **Recalling context**: Use `moneypenny_facts` action `search`
- **Ingesting documents**: Use `moneypenny_knowledge` action `ingest`
- **Activity trail**: Use `moneypenny_activity` action `query`
- **Governance**: Use `moneypenny_policy` to manage rules"#;

/// Best practices.
pub const BEST_PRACTICES: &str = r#"- Search before inserting facts to avoid duplicates
- Use specific keywords when inserting facts
- Set confidence scores to reflect certainty (0.0 to 1.0)
- Use `moneypenny_execute` only for operations not covered by domain tools"#;
