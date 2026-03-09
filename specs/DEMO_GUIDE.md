# Moneypenny — Quickstart

Give your AI coding agent persistent memory, knowledge ingestion, policy governance, and a full audit trail. Three commands to set up, then everything happens through conversation.

Works with **Claude Code**, **Cortex Code CLI**, and **OpenClaw**.

**Time estimate:** 5 minutes to set up, then use it forever.

---

## 1. Install

### From source (requires Rust toolchain)

```bash
git clone --recurse-submodules https://github.com/jacobprall/moneypenny.git
cd moneypenny
cargo build --release
```

Put the binary on your PATH:

```bash
cp target/release/mp /usr/local/bin/mp
```

### Verify

```bash
mp --version
```

---

## 2. Initialize

Navigate to the project you want Moneypenny to remember context for:

```bash
cd ~/your-project
mp init
```

This creates two things:

- `moneypenny.toml` — configuration file
- `mp-data/` — one SQLite database per agent

That database file **is** the agent. Memory, knowledge, policies, skills, audit trail — everything in one portable file.

### Optional: local embeddings for vector search

```bash
mkdir -p mp-data/models
curl -L -o mp-data/models/nomic-embed-text-v1.5.gguf \
  https://huggingface.co/nomic-ai/nomic-embed-text-v1.5-GGUF/resolve/main/nomic-embed-text-v1.5.Q8_0.gguf
```

---

## 3. Connect to your agent

Pick your runtime and run one command:

### Claude Code

```bash
mp setup claude-code
```

Writes `.mcp.json` in your project root. For global registration across all projects:

```bash
mp setup claude-code --global
```

Restart Claude Code to pick up the new MCP server.

### Cortex Code CLI

```bash
mp setup cortex
```

Runs `cortex mcp add` to register directly. Verify with:

```bash
cortex mcp list
```

### OpenClaw

```bash
mp setup openclaw
```

Writes to `~/.clawdbot/clawdbot.json`. Restart OpenClaw to pick up the new MCP server.

### Verify

In any of the above, start a conversation and ask:

```
You: What Moneypenny tools do you have?
```

Your agent should list the available MCP tools (memory search, fact management, knowledge ingestion, policy engine, etc.).

---

## 4. Use it

Everything below happens through natural conversation with your agent. You never need to touch the CLI again.

### Teach it facts

Tell your agent things you want it to remember permanently — across sessions, across conversations.

```
You: Remember that our deployment pipeline uses ArgoCD with canary deploys
     at 5% traffic for 30 minutes. Deploys happen Tuesday and Thursday.
     Auto-rollback triggers if error rate exceeds 0.1%.
```

Your agent calls `memory.fact.add` and stores a structured fact with full content, a one-line summary, and a 2-5 word pointer. Facts compress progressively as context fills up.

```
You: Remember that the fleet has 200 autonomous warehouse robots across
     Austin, Chicago, and Newark. Four core services: Navigator, Picker,
     Orchestrator, and Vision — all communicating over gRPC.
```

```
You: Remember that our top priorities for Q2 are: reduce pick time from
     4.2s to 3.5s, roll out firmware v3.2, complete SOC 2 Type II, hire
     3 senior perception engineers, and migrate from CockroachDB to TiKV.
```

### Feed it knowledge

Ingest entire documents — they get chunked, indexed with FTS5 full-text search, and (if you downloaded the embedding model) vector-indexed.

```
You: Ingest docs/api-reference.md into the knowledge base
```

```
You: Ingest the project handbook at scripts/demo-data/project-handbook.md
```

Your agent calls `knowledge.ingest` and the document is chunked and stored in SQLite. No external vector DB needed.

### Search memory

Ask questions that span facts, knowledge, messages, tool calls, and policy audit — all searched in a single query.

```
You: What do you know about our deployment pipeline?
```

```
You: Search memory for anything related to trajectory optimization
```

Your agent calls `memory.search` which runs hybrid retrieval: FTS5 full-text matching plus vector KNN (when embeddings exist), fused via Reciprocal Rank Fusion with MMR re-ranking for diversity.

### Set policies

Govern what the agent can and cannot do. Policies use glob patterns for actor/action/resource matching.

```
You: Add a policy that blocks any destructive SQL — DROP, TRUNCATE, DELETE
     without WHERE. Call it "no-destructive-sql".
```

```
You: Add an audit policy that logs every memory search operation
```

```
You: Test if "DROP TABLE users" would be allowed by current policies
```

Your agent calls `policy.add` and `policy.evaluate`. Three effects: allow, deny, audit. Every policy decision is logged with full request context.

### Register skills

Skills are reusable procedures that surface automatically via RAG when relevant.

```
You: Save an incident triage skill: When triaging, first check dashboards
     for scope. Escalation path: on-call, then team lead, then VP Eng,
     then CTO. Single-robot issues go to SRE. Fleet-wide issues escalate
     to platform lead immediately. Document in incident channel. Post-
     incident review within 48 hours.
```

Your agent calls `skill.add`. The skill is stored in the agent's DB and discoverable automatically — when someone later asks about incident handling, the skill appears in context assembly.

### Schedule jobs

Create cron-scheduled recurring tasks.

```
You: Create a daily job at 9am called "metrics-check" that prompts
     "Check the latest performance metrics and summarize any regressions"
```

```
You: List all scheduled jobs
```

```
You: Pause the metrics-check job
```

Your agent calls `job.create`, `job.list`, `job.pause`. Jobs support four payload types: prompt, tool, js (QuickJS sandbox), and pipeline.

### Create custom tools

Agents can create their own tools at runtime using sandboxed JavaScript.

```
You: Create a JS tool called "celsius_to_fahrenheit" that converts
     a celsius argument to Fahrenheit
```

Your agent calls `js.tool.add`. The tool is policy-gated, audited, and available on the next turn.

### Inspect state

Check what the agent knows, what's been audited, and what policies are active.

```
You: List all facts
```

```
You: Show the full details for fact <ID>
```

```
You: Query the audit trail for recent policy decisions
```

```
You: Show the database schema
```

Your agent calls `memory.fact.get`, `audit.query`, and other introspection tools.

### Multi-agent

Create additional agents and sync knowledge between them.

```
You: Create a new agent called "research"
```

```
You: List all agents
```

Agents sync via CRDTs at the SQLite level — merge-safe, no conflicts. Use the CLI for sync operations:

```bash
mp sync push --to research
mp sync pull --from research
```

### Import external history

Import conversation histories from other agent runtimes. Content-hash dedup makes re-imports safe.

```
You: Ingest the OpenClaw history from scripts/demo-data/openclaw-history.jsonl
```

Your agent calls `ingest.events`. Facts are automatically extracted from imported conversations.

#### Auto-ingest from Cortex Code CLI

Import all your Cortex Code conversations in one shot:

```bash
mp ingest --cortex
```

Discovers sessions from `~/.snowflake/cortex/conversations/`, converts them to Moneypenny's event format, and ingests with full dedup. Re-run anytime — already-imported messages are skipped.

#### Auto-ingest from Claude Code

Import all your Claude Code conversations:

```bash
mp ingest --claude-code
```

Or scope to a specific project:

```bash
mp ingest --claude-code=my-project-slug
```

Discovers sessions from `~/.claude/projects/`, extracts messages, tool calls, and usage stats, then ingests them. Thinking blocks and system reminders are filtered out.

---

## 5. MCP Tool Reference

These are the 33 canonical operations exposed as MCP tools when your agent connects to Moneypenny:

| Tool | Description |
|------|-------------|
| `memory.search` | Search memory across all stores |
| `memory.fact.add` | Create a structured fact |
| `memory.fact.update` | Update an existing fact |
| `memory.fact.get` | Get full fact content by ID |
| `memory.fact.compaction.reset` | Reset fact compaction state |
| `fact.delete` | Delete a fact |
| `knowledge.ingest` | Ingest a document or URL |
| `skill.add` | Add a reusable skill/procedure |
| `skill.promote` | Promote a skill |
| `policy.add` | Add a policy rule |
| `policy.evaluate` | Evaluate a policy decision (dry run) |
| `policy.explain` | Explain why a policy decision was made |
| `job.create` | Create a scheduled job |
| `job.list` | List scheduled jobs |
| `job.run` | Trigger a job immediately |
| `job.pause` | Pause a scheduled job |
| `job.history` | List job run history |
| `job.spec.plan` | Plan an agent-generated job spec |
| `job.spec.confirm` | Confirm a planned job spec |
| `job.spec.apply` | Apply a confirmed job spec |
| `session.resolve` | Resolve or create a session |
| `session.list` | List conversation sessions |
| `audit.query` | Query audit records |
| `audit.append` | Append an audit record |
| `js.tool.add` | Add a JavaScript tool |
| `js.tool.list` | List JavaScript tools |
| `js.tool.delete` | Delete a JavaScript tool |
| `agent.create` | Create a new agent |
| `agent.delete` | Delete an agent |
| `agent.config` | Update agent configuration |
| `ingest.events` | Ingest external events (JSONL) |
| `ingest.status` | List ingest run history |
| `ingest.replay` | Replay a previous ingest run |

---

## 6. How it works (30-second version)

Every agent is a single SQLite database file. Seven statically-linked SQLite extensions provide vector search, JS execution, CRDT sync, and on-device inference — all inside the database.

`mp sidecar` runs an MCP server over stdio. Your agent runtime connects to it, discovers the 33 tools above, and calls them during conversation. Every operation flows through the same pipeline:

```
Request → Idempotency check → Policy evaluation → Handler → Redaction → Audit
```

Facts have three compression levels (full content, summary, pointer) and compact progressively as the token budget fills. The agent can always expand a pointer back to full text.

Search is hybrid: FTS5 full-text plus vector KNN, fused via Reciprocal Rank Fusion with MMR re-ranking. A single query searches across facts, messages, tool calls, knowledge chunks, and policy audit.

One binary. One file per agent. No cloud dependencies.

---

## Appendix: CLI Reference

For direct CLI usage (scripting, CI, or hands-on exploration), see [CLI_DEMO.md](CLI_DEMO.md).

| Command | Purpose |
|---------|---------|
| `mp init` | Create config + data directory |
| `mp setup claude-code` | Register as MCP server in Claude Code |
| `mp setup cortex` | Register as MCP server in Cortex Code CLI |
| `mp setup openclaw` | Register as MCP server in OpenClaw |
| `mp start` / `mp stop` | Start/stop the gateway |
| `mp chat` | Interactive REPL |
| `mp send <agent> <msg>` | One-shot message |
| `mp sidecar` | MCP server over stdio |
| `mp agent list/create/delete/status` | Agent management |
| `mp facts list/search/inspect/expand` | Fact operations |
| `mp knowledge search/list` | Knowledge queries |
| `mp ingest <path>` | Document ingestion |
| `mp ingest --cortex` | Ingest Cortex Code CLI conversations |
| `mp ingest --claude-code` | Ingest Claude Code conversations |
| `mp policy list/add/test/violations` | Policy management |
| `mp skill add/list` | Skill management |
| `mp job create/list/run/pause/history` | Job scheduling |
| `mp audit search/export` | Audit trail |
| `mp sync status/push/pull` | CRDT sync |
| `mp db query/schema` | Direct SQL access |
| `mp health` | System health check |
