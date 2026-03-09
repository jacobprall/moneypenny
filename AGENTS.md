---
description: Moneypenny MCP server - persistent memory, knowledge, and governance for AI agents
globs:
alwaysApply: true
---

# Moneypenny

You have access to a Moneypenny MCP server. It provides persistent memory,
knowledge retrieval, document ingestion, governance policies, and scheduled jobs.

## "mp" prefix

When the user starts a message with **"mp"** (e.g. "mp remember that we use
Redis for caching", "mp search facts about auth", "mp ingest this doc"), treat
it as a direct instruction to use Moneypenny. Translate the natural-language
request into the appropriate `moneypenny.query` call and execute it immediately.

## Tools

| Tool | Purpose |
|------|---------|
| `moneypenny.query` | **Primary.** Execute MPQ expressions (see syntax below). |
| `moneypenny.capabilities` | Discover available domains and example expressions. |
| `moneypenny.execute` | Fallback for advanced operations not yet in MPQ. |

**Important:** These tools are MCP tools served by the Moneypenny sidecar
process. They must appear in your callable tool list. If they do not, the MCP
server is not connected — tell the user to run `mp setup <target>` (e.g.
`mp setup cursor`, `mp setup cortex`, `mp setup claude-code`) and restart
their editor / CLI.

## MPQ syntax

```
SEARCH <store> [WHERE <filters>] [SINCE <duration>] [| SORT field ASC|DESC] [| TAKE n]
INSERT INTO facts ("content", key=value ...)
UPDATE facts SET key=value WHERE id = "id"
DELETE FROM facts WHERE <filters>
INGEST "url"
SEARCH audit WHERE <filters> [| TAKE n]
CREATE POLICY "name" allow|deny|audit <action> ON <resource> [MESSAGE "reason"]
CREATE JOB "name" SCHEDULE "cron" [TYPE type]
```

### Stores

`facts`, `knowledge`, `log`, `audit`

### Examples

```
SEARCH facts WHERE topic = "auth" SINCE 7d | SORT confidence DESC | TAKE 10
INSERT INTO facts ("Redis preferred for caching", topic="infrastructure", confidence=0.9)
DELETE FROM facts WHERE confidence < 0.3 AND BEFORE 30d
SEARCH knowledge WHERE "deployment runbook" | TAKE 5
SEARCH facts | COUNT
```

### Multi-statement

Separate with `;` to run multiple operations:

```
INSERT INTO facts ("new fact"); SEARCH facts | TAKE 5
```

### Dry run

Set `dry_run: true` to parse and policy-check without executing.

## When to use Moneypenny

- **User says "mp ..."**: Always route through Moneypenny (see prefix rule above)
- **Remembering things**: Store facts the user tells you to remember
- **Recalling context**: Search facts and knowledge before answering questions
- **Ingesting documents**: Use INGEST to add URLs or documents to the knowledge store
- **Audit trail**: Search audit logs to understand what happened
- **Governance**: Create policies to control what agents can do

## Best practices

- Search before inserting facts to avoid duplicates
- Use specific topics and keywords when inserting facts
- Set confidence scores to reflect certainty (0.0 to 1.0)
- Prefer `moneypenny.query` over `moneypenny.execute` whenever possible
