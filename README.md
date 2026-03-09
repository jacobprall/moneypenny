# Moneypenny

### The intelligence layer for AI agents.

Persistent memory, policy governance, and audit for AI agents — in a single SQLite file.

Moneypenny sits between your agent and its LLM. After every turn it extracts and compresses knowledge into structured facts, enforces policy on every tool call, logs an auditable decision trail, and syncs state across agents via CRDTs. The entire agent state lives in one portable SQLite database. No Postgres, no Redis — just SQLite.

It runs as a standalone agent runtime (CLI, HTTP, Slack, Discord, Telegram) or as a sidecar that plugs into **Cursor** or **Claude Code** via MCP. One command to connect.

## For Agents

If you are a coding agent and your user wants to set up Moneypenny, run the following:

```bash
# 1. Clone and build (~5 min first time, requires Rust toolchain)
git clone --recurse-submodules https://github.com/jacobprall/moneypenny.git
cd moneypenny
cargo build --release

# 2. Initialize the agent database
./target/release/mp init
```

Then register as an MCP server. **Run only the command that matches your environment:**

- **If you are Claude Code** (claude, claude-code, Anthropic CLI agent):
  ```bash
  ./target/release/mp setup claude-code
  ```
  The server is available immediately. This writes `.mcp.json` and `CLAUDE.md`. No restart needed.

- **If you are Cursor** (or another editor that reads `.cursor/`):
  ```bash
  ./target/release/mp setup cursor --local
  ```
  Tell the user to restart Cursor (or reload the window). This writes `.cursor/mcp.json`, `.cursor/hooks.json`, and `.cursor/rules/moneypenny.mdc`.

Moneypenny registers 3 MCP tools: `moneypenny.query` (the primary interface — every operation is an MPQ expression), `moneypenny.capabilities` (domain discovery), and `moneypenny.execute` (advanced fallback).

To verify the connection works, call `moneypenny.query` with expression `SEARCH facts WHERE topic = "test" | TAKE 5`.

All state lives in a single SQLite database at `mp-data/main.db`. If you rebuild `mp`, Cursor picks up changes on next restart.

---

## Quick Start

```bash
git clone --recurse-submodules https://github.com/jacobprall/moneypenny.git
cd moneypenny
cargo build --release
./target/release/mp init
./target/release/mp setup cursor --local
```

Restart Cursor (or reload the window). Done. Everything else happens through conversation:

```
You: Remember that deploys happen Tuesday and Thursday via ArgoCD with canary at 5%
You: Ingest docs/api-reference.md into the knowledge base
You: What do you know about our deployment pipeline?
You: Add a policy that blocks destructive SQL
You: Create a daily job at 9am to check performance metrics
```

Embeddings run locally by default (nomic-embed-text-v1.5). No API keys required for memory and search.

To load a rich demo environment (3 agents, 15 facts, 4 docs, 6 policies, 2 skills):

```bash
./scripts/demo.sh        # setup + cheat sheet
./scripts/demo.sh --chat # setup + drop into interactive chat
```

## What It Looks Like

These prompts work out of the box against the demo environment. Each one exercises a capability that stateless LLMs can't replicate.

**Memory that compounds across sessions**

> "What happened with the Newark launch and how is it affecting pick times?"

The agent retrieves two linked facts — Newark's successful Feb 3 launch and the pick time regression caused by its narrower shelf spacing — without being told they're related. It connects the graph edges, not just keyword overlap.

**Governance you can interrogate**

> "Delete all the old facts from the database"

The policy engine blocks the `DELETE` (no WHERE clause), returns the denial as context, and the agent explains why it can't comply and suggests alternatives. Then ask:

> "Show me every policy violation this week"

Queryable audit. The denial from 10 seconds ago is already in the trail.

**Cross-source retrieval**

> "A robot is down at Newark — walk me through triage"

The agent pulls the incident triage skill, the runbook's severity classification, the Newark site facts, and the on-call rotation — four different stores, one coherent answer. Ask follow-up:

> "What's the escalation path if this is a SEV1?"

It retrieves deeper into the runbook without re-searching from scratch. The session context compounds.

**Multi-agent delegation**

> "Ask the research agent to compare TiKV vs CockroachDB for our use case"

The main agent delegates to `research`, which has its own persona, memory, and synced facts about the ongoing TiKV migration. It returns a structured analysis grounded in what it knows about Acme's architecture.

**Knowledge + facts + memory working together**

> "We just decided to postpone the TiKV migration to July. Update your knowledge."

The agent calls `fact_add` to record the decision. In subsequent sessions, retrieval surfaces the updated timeline. Old facts about the May target decay in confidence. Ask the next day:

> "What's the current status of the TiKV migration?"

It returns the July timeline, not the stale May date. Memory self-curates.

**Introspection**

> "What do you know about our security posture?"

The agent traverses fact graph edges from the security pointer, expands from 5-word pointers to full detail, and synthesizes across mTLS, firmware signing, SOC 2 progress, and PII handling — all without a vector search. Then verify what it used:

```bash
mp db query "SELECT pointer, confidence FROM facts WHERE status='active' ORDER BY confidence DESC"
```

## How It Works

**Database as runtime.** The database is the runtime, not just the storage layer. Policy evaluation, fact extraction, knowledge search, tool governance, and audit logging all execute inside SQLite. The orchestrator is a thin async loop; the intelligence — compression, budgeting, extraction, governance — lives at the data boundary, not in application code that happens to persist.

**Four memory stores, one search layer.** Facts (long-term knowledge), conversation log, ingested documents, and session scratch feed into a single hybrid retrieval layer (vector similarity + full-text, fused via Reciprocal Rank Fusion, deduplicated with MMR diversity ranking). Token budgeting allocates context across stores by query, session depth, and task.

**Turn lifecycle:**

1. User message arrives via any channel
2. Hybrid retrieval builds context from all four stores
3. Policy engine evaluates the action
4. LLM generates a response (tool calls go through policy again)
5. Extraction pipeline distills new facts from the conversation
6. Facts are embedded, linked, and compressed

## Memory

After every turn, an extraction pipeline distills knowledge into structured facts. Each fact has three compression levels: full detail, summary, and a 2–5 word pointer. The agent loads all knowledge as pointers (~2K tokens for 500 facts), then auto-expands only what's relevant to the current query.

Facts are graph-linked — traversable edges between related concepts. "What do I know about X?" follows the graph without a separate retrieval call. Confidence grows when facts are re-extracted; stale knowledge decays with configurable half-life.

Extraction runs on a separate model call. A small local model (3B) can handle extraction while the main conversation uses Claude or GPT.

## Governance

The policy engine evaluates every tool call, memory write, fact extraction, and SQL query before execution.

**Static rules** — block destructive ops, require WHERE on DELETE, restrict tools by trust level, scope by channel.
**Behavioral rules** — rate-limit shell access, detect retry loops, per-session token budgets, time-window (cron) restrictions.

Denials don't crash the agent. They're returned as context so the agent can adapt. A stuck detector breaks retry loops and context thrashing.

Eighteen regex patterns scrub API keys, JWTs, PEMs, and connection strings before anything touches disk.

Every policy decision is logged. The audit trail is queryable:

```bash
mp audit search "why was shell_exec denied?"
mp policy violations
```

## Sync

Multiple agents share knowledge via CRDT-based sync. No central server required for local P2P; optional cloud sync via SQLite Cloud.

Facts are `private` by default. Extraction or admin can promote them to `shared` (all agents) or `protected` (trusted agents only). Scope is enforced at the SQL level — agents cannot query outside their authorization.

Synced: facts, fact links, skills, policies. Local-only: conversations, scratch, job runs.

```bash
mp sync push --to agent-b    # local P2P
mp sync connect --url "..."  # cloud sync
```

## Tools & Skills

Tools come from four sources: built-in (file I/O, shell, web search, memory ops), MCP servers, runtime skills, and user-defined JavaScript stored in the database.

```toml
[[agents.mcp_servers]]
name = "github"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]
```

JS tools persist across restarts and sync across agents:

```bash
mp skill add ./my-tool.js
```

Skills track usage and success rate. High-performing skills surface more in retrieval.

## Scheduled Jobs

Cron jobs, pipelines, self-reflection prompts, and custom JS — all governed by the same policy engine.

```bash
mp job create --cron "0 */6 * * *" --type reflection
mp job list
mp job history
```

Jobs sync across agents. Define once, propagate.

## Channels

Same agent loop, thin adapters:

| Channel   | Transport |
|-----------|-----------|
| CLI       | `mp chat` interactive REPL |
| HTTP      | REST + SSE + WebSocket on configurable port |
| Slack     | Events API (app_mention, DM), HMAC verification |
| Discord   | Interactions API, Ed25519 verification, slash commands |
| Telegram  | Long-polling, per-chat sessions |

## Cursor Integration

Moneypenny exposes its full surface area as an MCP server. One command sets up everything:

```bash
mp setup cursor --local
```

This writes three files to `.cursor/`:
- **`mcp.json`** — registers the MCP server (runs `mp serve` over stdio)
- **`hooks.json`** — audit trail + policy enforcement on every tool call, shell command, and file edit
- **`rules/moneypenny.mdc`** — agent instructions so Cursor knows how to use Moneypenny

Under the hood, `mp serve` runs both the MCP server over stdio and an HTTP gateway on port 4820. Every operation Moneypenny supports is expressible as a short string in **MPQ (Moneypenny Query)** — a single unified language exposed through one MCP tool whose description *is* the syntax. Instead of discovering 12 tools and learning their JSON schemas, the agent reads ~200 tokens of grammar and can immediately write expressions like `SEARCH facts WHERE topic = "auth" SINCE 7d | TAKE 10` or `CREATE POLICY "no-junior-deletes" deny DELETE ON facts FOR AGENT "junior"`. The verb maps directly to the canonical operation; the policy engine pattern-matches against the raw expression; the audit trail is human-readable by default.

The CLI agent (`mp chat`, `mp send`) shares the same database and agent — knowledge persists seamlessly between Cursor and terminal sessions.

### Import conversation history

Auto-ingest prior conversations from Cursor into Moneypenny's memory:

```bash
mp ingest --cursor                       # all Cursor sessions
mp ingest --cursor=my-project-slug       # scoped to one project
```

Content-hash deduplication makes re-runs safe.

## Claude Code Integration

One command registers Moneypenny as an MCP server and writes agent instructions:

```bash
mp setup claude-code
```

This writes two files:
- **`.mcp.json`** — MCP server config at the project root (committable to git for team sharing)
- **`CLAUDE.md`** — agent instructions so Claude Code knows how to use Moneypenny

To register for all projects instead of just this one:

```bash
mp setup claude-code --scope user
```

This writes to `~/.claude.json` instead. The server is available immediately — no restart needed.

Knowledge persists across Cursor, Claude Code, and terminal sessions — they all share the same agent database.

## Configuration

```toml
data_dir = "./mp-data"

[gateway]
host = "127.0.0.1"
port = 4820

[[agents]]
name = "main"
persona = "You are a helpful assistant."
trust_level = "standard"         # standard | elevated | admin
policy_mode = "allow_by_default" # allow_by_default | deny_by_default

[agents.llm]
provider = "anthropic"           # anthropic | openai | local (GGUF)
model = "claude-sonnet-4-20250514"

[agents.embedding]
provider = "local"               # local runs on-device, no API key
model = "nomic-embed-text-v1.5"

[channels]
cli = true

[channels.http]
port = 4821
```

See `moneypenny.example.toml` for the full reference.

## Project Structure

```
crates/
  mp/          CLI binary, channel adapters, gateway server
  mp-core/     Schema, operations, policy, extraction, sync, tools
  mp-llm/      LLM provider abstraction (local GGUF, HTTP APIs)
  mp-ext/      SQLite extensions and MCP FFI
vendor/        sqlite-ai, sqlite-vector, sqlite-sync, etc.
```

Built on seven [SQLite AI](https://github.com/sqliteai) extensions statically linked into one binary — covering on-device inference, vector search, CRDT sync, JS execution, and RAG.

## Documentation

```bash
cd docs && npm install && npm run dev
```

| Section | Path |
|---------|------|
| Quickstart | `docs/src/content/docs/quickstart.mdx` |
| Core Concepts | `docs/src/content/docs/concepts/` |
| Guides | `docs/src/content/docs/guides/` |
| CLI Reference | `docs/src/content/docs/cli/reference.md` |
| Architecture | `docs/src/content/docs/architecture/` |

## License

Apache-2.0
