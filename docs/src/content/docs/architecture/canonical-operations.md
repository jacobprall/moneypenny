---
title: Canonical Operations
description: The unified execution pipeline for all mutations and queries
---

Every mutating or policy-relevant action in Moneypenny flows through the
canonical operation layer. This is the single execution path — CLI, HTTP,
sidecar, and MCP all compile down to the same operations.

## Why Canonical Operations

Without a canonical layer, each transport adapter (CLI, HTTP, sidecar, MCP)
would implement its own business logic, leading to divergent behavior and
security gaps. The canonical operation layer guarantees:

- **Consistent policy enforcement** across all entry points
- **Uniform audit trail** regardless of how an action was triggered
- **Single place to add hooks** (pre/post processing, transformation)
- **No adapter-specific business logic** — adapters only translate wire formats

## Operation Envelope

Every request follows this structure:

```json
{
  "op": "namespace.action",
  "args": { ... }
}
```

Every response uses a standard envelope:

```json
{
  "ok": true,
  "code": "success",
  "message": "Operation completed",
  "data": { ... },
  "policy": {
    "effect": "allow",
    "policy_id": "pol_abc123"
  },
  "audit": {
    "recorded": true
  }
}
```

## Execution Pipeline

Every operation follows these steps in order:

```
1. Parse operation envelope
2. Resolve context (actor, session, tenant)
3. Pre-policy evaluation
4. Pre-hooks (DB-backed hook registry + baseline guardrails)
5. Handler execution
6. Post-hooks (DB-backed hook registry + baseline redaction)
7. Secret redaction + audit write
8. Standard result envelope
```

### Steps in Detail

**Parse** — validate the operation name and argument schema.

**Resolve context** — determine who is making the request (actor), which
session it belongs to, and any tenant scoping.

**Pre-policy** — evaluate the action against the policy engine before
executing anything. If denied, return immediately with the denial reason.

**Pre-hooks** — run registered pre-hooks from the DB hook registry. Hooks
can transform arguments or abort the operation. Baseline guardrails
(e.g. argument validation) run here.

**Handler** — execute the actual operation logic (insert a fact, run a
search, create a job, etc.).

**Post-hooks** — run registered post-hooks. These can transform output
before it's returned.

**Redaction + audit** — scrub sensitive content from the result using the
18-pattern secret redaction engine, then write the audit record.

**Envelope** — wrap the result in the standard response envelope with
policy and audit metadata.

## Operation Catalog

### Memory Operations

| Operation | Arguments | Description |
|---|---|---|
| `memory.search` | `query`, `limit` | Hybrid search across all stores |
| `memory.fact.add` | `content`, `summary`, `pointer`, `confidence`, `keywords` | Store a fact |
| `memory.fact.update` | `id`, `content`, `summary`, `pointer` | Update a fact |
| `memory.fact.get` | `id` | Retrieve a fact |
| `memory.fact.compaction.reset` | `id` | Reset compaction state |
| `fact.delete` | `id` | Soft-delete a fact |

### Knowledge Operations

| Operation | Arguments | Description |
|---|---|---|
| `knowledge.ingest` | `path` or `url` | Ingest a document |

### Policy Operations

| Operation | Arguments | Description |
|---|---|---|
| `policy.add` | `name`, `effect`, `action`, `resource`, ... | Add a rule |
| `policy.evaluate` | `actor`, `action`, `resource` | Evaluate an action |
| `policy.explain` | `actor`, `action`, `resource` | Explain a decision |

### Skill Operations

| Operation | Arguments | Description |
|---|---|---|
| `skill.add` | `name`, `description`, `content` | Add a skill |
| `skill.promote` | `id` | Promote retrieval weight |

### Job Operations

| Operation | Arguments | Description |
|---|---|---|
| `job.create` | `name`, `schedule`, `job_type`, `payload` | Create a job |
| `job.list` | | List all jobs |
| `job.run` | `id` | Trigger immediately |
| `job.pause` | `id` | Pause scheduling |
| `job.history` | `id` (optional) | View run history |
| `job.spec.plan` | `description` | Plan a job (agent flow) |
| `job.spec.confirm` | `spec_id` | Confirm a planned job |
| `job.spec.apply` | `spec_id` | Apply a confirmed job |

### JS Tool Operations

| Operation | Arguments | Description |
|---|---|---|
| `js.tool.add` | `name`, `description`, `source`, `parameters_schema` | Register a tool |
| `js.tool.list` | | List JS tools |
| `js.tool.delete` | `name` | Remove a tool |

### Session Operations

| Operation | Arguments | Description |
|---|---|---|
| `session.resolve` | `session_id` (optional) | Resolve or create a session |
| `session.list` | `limit` | List recent sessions |

### Agent Operations

| Operation | Arguments | Description |
|---|---|---|
| `agent.create` | `name` | Create a new agent |
| `agent.config` | `name`, `key`, `value` | Update configuration |
| `agent.delete` | `name` | Delete an agent |

### Audit Operations

| Operation | Arguments | Description |
|---|---|---|
| `audit.query` | `query`, `limit` | Search audit records |
| `audit.append` | `action`, `resource`, `message` | Write an entry |

### Ingest Operations (External Events)

| Operation | Arguments | Description |
|---|---|---|
| `ingest.events` | `source`, `file` | Ingest external events |
| `ingest.status` | `source`, `limit` | Check run status |
| `ingest.replay` | `run_id`, `dry_run` | Replay a prior run |

## Transport Mapping

Each transport adapter maps to canonical operations:

| Transport | How It Works |
|---|---|
| **CLI** | `clap` commands map to operations; `mp facts list` → `memory.fact.list` |
| **HTTP** | `POST /v1/ops` accepts the JSON envelope directly |
| **Sidecar** | JSONL over stdin/stdout, one operation per line |
| **MCP** | MCP tool calls are translated to canonical operations |

The agent loop itself uses canonical operations for internal mutations
(fact extraction, audit writes, session management). There are no hidden
"agent-only" mutation paths.
