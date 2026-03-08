---
title: CLI Reference
description: Every command, flag, and subcommand
---

## Global Options

| Flag | Default | Description |
|---|---|---|
| `-c, --config <PATH>` | `moneypenny.toml` | Path to config file |
| `--version` | | Show version |
| `--help` | | Show help |

---

## `mp init`

Create `moneypenny.toml` and the data directory. Downloads the default local
embedding model if not present.

```bash
mp init
```

---

## `mp start`

Start the gateway, spawn worker processes for all configured agents, and
bind channel adapters.

```bash
mp start
```

---

## `mp stop`

Gracefully shut down the gateway and all workers.

```bash
mp stop
```

---

## `mp health`

Show system health status.

```bash
mp health
```

---

## `mp chat`

Interactive CLI chat with an agent.

```bash
mp chat [AGENT] [--session-id <ID>]
```

| Argument | Description |
|---|---|
| `AGENT` | Agent name (defaults to first configured agent) |
| `--session-id` | Resume an existing session |

---

## `mp send`

Send a one-off message and print the response.

```bash
mp send <AGENT> <MESSAGE> [--session-id <ID>]
```

| Argument | Description |
|---|---|
| `AGENT` | Agent name |
| `MESSAGE` | Message text |
| `--session-id` | Resume an existing session |

---

## `mp agent`

Manage agents.

### `mp agent list`

List all registered agents.

### `mp agent create <NAME>`

Create a new agent.

### `mp agent delete <NAME> [--confirm]`

Delete an agent and its database file.

### `mp agent status [NAME]`

Show agent status and memory stats. Without a name, shows all agents.

### `mp agent config <NAME> <KEY> <VALUE>`

Set an agent configuration value at runtime.

```bash
mp agent config main persona "You are a senior SRE."
```

---

## `mp session`

Manage conversation sessions.

### `mp session list [AGENT] [--limit <N>]`

List recent sessions for an agent. Default limit is 20.

---

## `mp facts`

Manage facts (extracted knowledge).

### `mp facts list [AGENT]`

List all facts showing pointer and summary.

### `mp facts search <QUERY> [AGENT]`

Search across facts using hybrid retrieval.

### `mp facts inspect <ID>`

Show the full fact record with audit history.

### `mp facts expand <ID>`

Expand a compacted pointer to full content.

### `mp facts reset-compaction [ID] [--all] [--confirm] [AGENT]`

Reset compaction state. Use `--all --confirm` to reset all facts.

### `mp facts promote <ID> [--scope <SCOPE>]`

Promote a fact's visibility scope. Default target scope is `shared`.

### `mp facts delete <ID> [--confirm]`

Soft-delete a fact (marks as superseded, retained for audit).

---

## `mp ingest`

Ingest documents into the knowledge store.

```bash
mp ingest [PATH] [AGENT] [OPTIONS]
```

| Flag | Description |
|---|---|
| `--url <URL>` | Ingest from a URL |
| `--openclaw-file <PATH>` | Ingest OpenClaw JSONL events |
| `--replay` | Replay from start (ignore prior cursor) |
| `--status` | Show recent ingest runs |
| `--replay-run <ID>` | Replay a specific prior run |
| `--replay-latest` | Replay the latest matching run |
| `--replay-offset <N>` | Offset for `--replay-latest` (0 = newest) |
| `--status-filter <STATUS>` | Filter by run status |
| `--file-filter <STRING>` | Filter by file path substring |
| `--dry-run` | Preview replay without writing |
| `--apply` | Apply replay writes |
| `--source <LABEL>` | Source label (default: `openclaw`) |
| `--limit <N>` | Limit for status output (default: 20) |

---

## `mp knowledge`

Manage the knowledge store.

### `mp knowledge list`

List ingested documents with title, source, and chunk count.

### `mp knowledge search <QUERY>`

Search ingested knowledge chunks.

---

## `mp skill`

Manage skills.

### `mp skill add <PATH> [AGENT]`

Add a skill from a markdown file.

### `mp skill list [AGENT]`

List skills with usage stats.

### `mp skill promote <ID>`

Manually promote a skill's retrieval weight.

---

## `mp policy`

Manage policies.

### `mp policy list`

List all active policies.

### `mp policy add [OPTIONS]`

Add a policy rule.

| Flag | Description |
|---|---|
| `--name <NAME>` | Policy name (required) |
| `--effect <EFFECT>` | `allow`, `deny`, or `audit` (default: `deny`) |
| `--priority <N>` | Higher = evaluated first (default: 0) |
| `--actor <PATTERN>` | Actor glob pattern |
| `--action <PATTERN>` | Action glob pattern |
| `--resource <PATTERN>` | Resource glob pattern |
| `--argument <PATTERN>` | Argument glob pattern |
| `--channel <PATTERN>` | Channel glob pattern |
| `--sql <REGEX>` | SQL regex pattern |
| `--rule-type <TYPE>` | `rate_limit`, `retry_loop`, `token_budget`, `time_window` |
| `--rule-config <JSON>` | Behavioral rule configuration |
| `--message <TEXT>` | Denial/audit message |

### `mp policy test <INPUT>`

Dry-run a policy evaluation.

### `mp policy violations [--last <WINDOW>]`

Show recent policy violations. Default window is `7d`.

### `mp policy load <FILE>`

Load policies from a JSON file.

---

## `mp job`

Manage scheduled jobs.

### `mp job list [AGENT]`

List scheduled jobs.

### `mp job create [OPTIONS]`

Create a new job.

| Flag | Description |
|---|---|
| `--name <NAME>` | Job name (required) |
| `--schedule <CRON>` | Cron schedule (required) |
| `--job-type <TYPE>` | `prompt`, `tool`, `js`, or `pipeline` (required) |
| `--payload <JSON>` | Job payload (required) |
| `--agent <NAME>` | Agent name |

### `mp job run <ID>`

Trigger a job immediately.

### `mp job pause <ID>`

Pause a scheduled job.

### `mp job history [ID]`

View job run history. Without an ID, shows all jobs.

---

## `mp audit`

View the audit trail.

```bash
mp audit [AGENT]
```

### `mp audit search <QUERY>`

Search audit entries.

### `mp audit export [--format <FORMAT>]`

Export audit trail. Formats: `json` (default), `csv`, `sql`.

---

## `mp sync`

Manage CRDT sync.

### `mp sync status [AGENT]`

Show site ID, database version, and per-table sync status.

### `mp sync now [AGENT]`

Bidirectional sync with all configured peers.

### `mp sync push --to <AGENT> [AGENT]`

One-way push to a peer.

### `mp sync pull --from <AGENT> [AGENT]`

One-way pull from a peer.

### `mp sync connect <URL> [AGENT]`

Set or update the cloud sync URL.

---

## `mp db`

Direct database access (read-only).

### `mp db query <SQL> [AGENT]`

Run a read-only SQL query against an agent's database.

```bash
mp db query "SELECT pointer, confidence FROM facts WHERE status='active'" main
```

### `mp db schema [AGENT]`

Show the database schema.

---

## `mp sidecar`

Run the canonical operation sidecar over stdio (JSONL). Reads operations from
stdin, writes results to stdout.

```bash
echo '{"op":"memory.search","args":{"query":"test"}}' | mp sidecar [--agent <NAME>]
```
