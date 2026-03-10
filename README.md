# Moneypenny

### The intelligence layer for AI agents.

Persistent memory, policy governance, and audit for AI agents — in a single SQLite file.

Moneypenny sits between your agent and its LLM. After every turn it extracts and compresses knowledge into structured facts, enforces policy on every tool call, logs an auditable decision trail, and syncs state across agents via CRDTs. The entire agent state lives in one portable SQLite database. No Postgres, no Redis — just SQLite.

It runs as a standalone agent runtime (CLI, HTTP, Slack, Discord, Telegram) or as a sidecar that plugs into **Cursor** or **Claude Code** via MCP. One command to connect.

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

## Quick Start

```bash
git clone --recurse-submodules https://github.com/jacobprall/moneypenny.git
cd moneypenny
cargo build --release
./target/release/mp init
./target/release/mp setup cursor --local   # or: mp setup claude-code
./target/release/mp doctor
```

Restart Cursor (or reload the window). Done. If doctor reports missing setup, follow the suggested command and rerun `mp doctor`.

Embeddings run locally by default (nomic-embed-text-v1.5). No API keys required for memory and search.

To load a rich demo environment (3 agents, 15 facts, 4 docs, 6 policies, 2 skills):

```bash
./scripts/demo.sh        # setup + cheat sheet
./scripts/demo.sh --chat # setup + drop into interactive chat
```

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

## Integrations

Moneypenny exposes its full surface area as an MCP server. Every operation is expressible as a short **MPQ (Moneypenny Query)** string — one tool, ~200 tokens of grammar, no JSON schemas to learn.

### Cursor

```bash
mp setup cursor --local
```

Writes `.cursor/mcp.json`, `.cursor/hooks.json`, and `.cursor/rules/moneypenny.mdc`. Restart Cursor to activate.

Auto-ingest prior conversations into memory:

```bash
mp ingest --cursor                       # all Cursor sessions
mp ingest --cursor=my-project-slug       # scoped to one project
```

### Claude Code

```bash
mp setup claude-code                # project-level (.mcp.json + CLAUDE.md)
mp setup claude-code --scope user   # user-level (~/.claude.json)
```

Available immediately — no restart needed.

### Channels

Same agent loop, thin adapters:

| Channel   | Transport |
|-----------|-----------|
| CLI       | `mp chat` interactive REPL |
| HTTP      | REST + SSE + WebSocket on configurable port |
| Slack     | Events API (app_mention, DM), HMAC verification |
| Discord   | Interactions API, Ed25519 verification, slash commands |
| Telegram  | Long-polling, per-chat sessions |

Knowledge persists across all channels — they share the same agent database.

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
