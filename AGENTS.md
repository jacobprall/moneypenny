---
description: Moneypenny MCP server - persistent facts, knowledge, governance, and activity tracking for AI agents
globs:
alwaysApply: true
---

# Moneypenny

You have access to a Moneypenny MCP server. It provides persistent facts,
knowledge retrieval, document ingestion, governance policies, and activity
tracking.

## "mp" prefix

When the user starts a message with **"mp"** (e.g. "mp remember that we use
Redis for caching", "mp search facts about auth", "mp ingest this doc"), treat
it as a direct instruction to use Moneypenny. Translate the natural-language
request into the appropriate tool call and execute it immediately.

## Tools

| Tool | Purpose |
|------|---------|
| `moneypenny.facts` | CRUD for durable facts — persistent knowledge across sessions. |
| `moneypenny.knowledge` | Ingest and retrieve documents — long-term reference library. |
| `moneypenny.policy` | Governance — control what agents can and cannot do. |
| `moneypenny.activity` | Query session history and audit trail. |
| `moneypenny.execute` | Escape hatch for any canonical operation. |

**Important:** These tools are MCP tools served by the Moneypenny sidecar
process. They must appear in your callable tool list. If they do not, the MCP
server is not connected — tell the user to run `mp setup <target>` (e.g.
`mp setup cursor`, `mp setup cortex`, `mp setup claude-code`) and restart
their editor / CLI.

## Tool usage

Each domain tool takes an `action` string and an `input` object:

### moneypenny.facts

| Action | Input | Description |
|--------|-------|-------------|
| `search` | `{query, limit?}` | Hybrid search across facts |
| `add` | `{content, summary?, keywords?, confidence?}` | Store a new fact |
| `get` | `{id}` | Retrieve a fact by ID |
| `update` | `{id, content, summary?}` | Update an existing fact |
| `delete` | `{id, reason?}` | Remove a fact |

### moneypenny.knowledge

| Action | Input | Description |
|--------|-------|-------------|
| `ingest` | `{content, title?, path?}` | Add a document |
| `search` | `{query, limit?}` | Search ingested documents |
| `list` | `{}` | List all documents |

### moneypenny.policy

| Action | Input | Description |
|--------|-------|-------------|
| `add` | `{name, effect?, priority?, action_pattern?, resource_pattern?, sql_pattern?, message?}` | Create a policy rule |
| `list` | `{enabled?, effect?, limit?}` | List policies |
| `disable` | `{id}` | Disable a policy |
| `evaluate` | `{actor, action, resource}` | Test if an action would be allowed |

### moneypenny.activity

| Action | Input | Description |
|--------|-------|-------------|
| `query` | `{source?, event?, action?, resource?, query?, limit?}` | Query session events and policy decisions |

Source options: `events` (session history), `decisions` (policy audit), `all` (default).

### moneypenny.execute

| Field | Description |
|-------|-------------|
| `op` | Canonical operation name (e.g. `job.create`, `ingest.events`) |
| `args` | Operation-specific arguments |

## When to use Moneypenny

- **User says "mp ..."**: Always route through Moneypenny (see prefix rule above)
- **Remembering things**: Use `moneypenny.facts` action `add`
- **Recalling context**: Use `moneypenny.facts` action `search` before answering questions
- **Ingesting documents**: Use `moneypenny.knowledge` action `ingest`
- **Activity trail**: Use `moneypenny.activity` action `query`
- **Governance**: Use `moneypenny.policy` to manage rules

## Best practices

- Search before inserting facts to avoid duplicates
- Use specific keywords when inserting facts
- Set confidence scores to reflect certainty (0.0 to 1.0)
- Use `moneypenny.execute` only for operations not covered by domain tools
