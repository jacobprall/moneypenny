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
