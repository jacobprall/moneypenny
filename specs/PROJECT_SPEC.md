# Moneypenny — Project Specification

> **Audience:** AI agents and developers navigating this codebase for the first time.
> Read this document to understand what Moneypenny is, how it's structured, and where to find things. Module-level specs live in `specs/`.

## What Is Moneypenny

Moneypenny is an **intelligence layer for AI agents**. It provides persistent memory, policy governance, and audit for AI agents — all stored in a single SQLite database per agent.

It sits between your agent and its LLM. After every turn it:
1. Extracts and compresses knowledge into structured facts
2. Enforces policy on every tool call
3. Logs an auditable decision trail
4. Syncs state across agents via CRDTs

It runs as a **standalone agent runtime** (CLI, HTTP, Slack, Discord, Telegram) or as a **sidecar** that plugs into existing runtimes via MCP (Model Context Protocol). The `mp setup` command auto-registers Moneypenny as an MCP server for Claude Code, Cortex Code CLI, or OpenClaw — one command to connect.

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                        mp (binary crate)                        │
│  CLI parsing · command handlers · worker process management     │
│  agent turn loop · channel adapters (HTTP/Slack/Discord/TG)     │
├────────────────┬────────────────────────┬───────────────────────┤
│   mp-core      │      mp-llm            │      mp-ext           │
│   (library)    │      (library)         │      (build crate)    │
│                │                        │                       │
│  operations    │  LLM provider traits   │  C/C++ build.rs       │
│  policy engine │  Anthropic provider    │  7 SQLite extensions  │
│  search/RRF    │  OpenAI-compat HTTP    │  static linking       │
│  context asm   │  Local GGUF embed      │  init_all_extensions  │
│  fact store    │  sqlite-ai stub        │                       │
│  knowledge     │                        │                       │
│  log store     │                        │                       │
│  tool registry │                        │                       │
│  scheduler     │                        │                       │
│  sync (CRDT)   │                        │                       │
│  ingest        │                        │                       │
│  redaction     │                        │                       │
└────────────────┴────────────────────────┴───────────────────────┘
                              │
              ┌───────────────┼───────────────┐
              │          vendor/              │
              │  sqlite-vector  sqlite-js     │
              │  sqlite-sync    sqlite-memory │
              │  sqlite-ai      sqlite-mcp    │
              │  sqlite-agent                 │
              └───────────────────────────────┘
```

### Key Principle: Database as Runtime

Every agent owns a SQLite database file. All state — facts, sessions, knowledge, skills, policies, jobs, audit trail — lives in SQLite. The seven statically-linked SQLite extensions provide vector search, JS execution, CRDT sync, on-device inference, and MCP tooling *inside* the database. The orchestrator is a thin loop; the intelligence sits between the database and the LLM.

## Repository Layout

```
moneypenny/
├── Cargo.toml              # Workspace root. Members: crates/*. Excludes vendor/.
├── moneypenny.toml          # Default runtime config
├── PROJECT_SPEC.md          # This file
├── specs/                   # Module-level specifications
│   ├── mp-core.md
│   ├── mp-llm.md
│   ├── mp-ext.md
│   ├── mp-binary.md
│   ├── DEMO_GUIDE.md        # User-facing quickstart (agent-integrated onboarding)
│   └── CLI_DEMO.md          # CLI reference for presenters
├── crates/
│   ├── mp/                  # Binary crate — CLI, adapters, turn loop, workers
│   │   ├── src/main.rs      # Entry point + all command handlers (~3700 lines)
│   │   ├── src/cli.rs       # clap CLI definition
│   │   └── src/adapters.rs  # HTTP/Slack/Discord/Telegram transports
│   ├── mp-core/             # Core library — all business logic
│   │   └── src/
│   │       ├── lib.rs            # Module declarations
│   │       ├── operations.rs     # Canonical operation dispatcher (~35 ops)
│   │       ├── policy.rs         # ABAC policy engine with behavioral rules
│   │       ├── search.rs         # Hybrid retrieval (FTS5 + vector, RRF + MMR)
│   │       ├── context.rs        # LLM context assembly with token budgeting
│   │       ├── extraction.rs     # Fact extraction pipeline
│   │       ├── agent.rs          # Sync agent turn loop (test harness)
│   │       ├── config.rs         # TOML config types
│   │       ├── schema.rs         # DB schema + migrations (11 versions)
│   │       ├── db.rs             # Connection helpers + pragmas
│   │       ├── gateway.rs        # Multi-agent routing + delegation
│   │       ├── sync.rs           # CRDT sync wrapper (sqlite-sync)
│   │       ├── mcp.rs            # MCP stdio client
│   │       ├── scheduler.rs      # Cron job engine
│   │       ├── ingest.rs         # External event ingestion (JSONL ETL)
│   │       ├── channel.rs        # Channel trait + capability model
│   │       ├── observability.rs  # Health checks + Prometheus metrics
│   │       ├── encryption.rs     # DB-at-rest encryption config
│   │       ├── store/
│   │       │   ├── facts.rs      # Fact CRUD, linking, audit, compaction
│   │       │   ├── log.rs        # Sessions, messages, tool calls
│   │       │   ├── knowledge.rs  # Documents, chunks, skills, edges
│   │       │   ├── scratch.rs    # Session-scoped ephemeral KV
│   │       │   └── redact.rs     # 18-pattern secret scanner
│   │       └── tools/
│   │           ├── registry.rs   # Tool registration, discovery, execution
│   │           ├── runtime.rs    # 19 self-awareness tools + JS bridge
│   │           ├── builtins.rs   # OS tools (file_read, shell_exec, etc.)
│   │           └── hooks.rs      # Pre/post tool execution hooks
│   ├── mp-llm/              # LLM abstraction layer
│   │   └── src/
│   │       ├── lib.rs            # Provider factories
│   │       ├── types.rs          # Provider-agnostic message types
│   │       ├── provider.rs       # LlmProvider + EmbeddingProvider traits
│   │       ├── anthropic.rs      # Anthropic Messages API + SSE streaming
│   │       ├── http.rs           # OpenAI-compatible chat completions
│   │       ├── local_embed.rs    # Local GGUF embeddings via sqlite-ai
│   │       └── sqlite_ai.rs      # Local generation provider (stub)
│   └── mp-ext/              # SQLite extension build + loader
│       ├── build.rs              # cc/cmake compilation of all C/C++ sources
│       ├── src/lib.rs            # FFI declarations + init_all_extensions()
│       └── tests/integration.rs  # Smoke tests
├── vendor/                  # Git submodules — 7 SQLite extensions
│   ├── sqlite-vector/       # Vector similarity search (KNN, cosine)
│   ├── sqlite-js/           # QuickJS-based JS execution
│   ├── sqlite-sync/         # CRDT merge + cloud sync
│   ├── sqlite-memory/       # Memory/knowledge management
│   ├── sqlite-ai/           # On-device LLM + whisper + audio (llama.cpp)
│   ├── sqlite-mcp/          # Model Context Protocol over FFI
│   └── sqlite-agent/        # Agent orchestration
├── scripts/
│   ├── demo.sh              # Demo environment setup (3 agents, 15 facts, etc.)
│   └── demo-data/           # Sample documents for demo
└── docs/                    # Astro-based documentation site
```

## Data Model

There are two database types:

### Agent Database (one per agent: `{name}.db`)

| Table | Purpose |
|---|---|
| `facts` | Long-term distilled knowledge. Three compression levels: content, summary, pointer. Graph-linked. Confidence-scored. Soft-deleted via `superseded_at`. |
| `fact_links` | Directed edges between facts (relation + strength). |
| `fact_audit` | Immutable audit trail for every fact mutation. |
| `sessions` | Conversation sessions with rolling summaries. |
| `messages` | Append-only message log (user/assistant/tool roles). |
| `tool_calls` | Detailed tool invocation records with policy decisions. |
| `documents` | Ingested document metadata (path, title, content hash). |
| `chunks` | Document chunks with position and embeddings. |
| `edges` | Knowledge graph edges (source, target, relation). |
| `skills` | Tool/skill registry with usage tracking and promotion. |
| `scratch` | Session-scoped ephemeral key-value store. |
| `policies` | ABAC policy rules (static + behavioral). |
| `policy_audit` | Every policy decision logged with full context. |
| `jobs` | Cron-scheduled recurring jobs. |
| `job_runs` | Job execution history. |
| `job_specs` | Agent-proposed job plans (plan/confirm/apply workflow). |
| `policy_specs` | Agent-proposed policy plans (plan/confirm/apply workflow). |
| `external_events` | Ingested external events (JSONL). |
| `ingest_runs` | Ingestion run metadata. |
| `operation_idempotency` | Idempotency keys for mutation replay/dedup. |
| `operation_hooks` | Configurable pre/post operation hooks. |

Schema is versioned (currently v11) with incremental migrations in `schema.rs`.

### Metadata Database (one per gateway: `metadata.db`)

| Table | Purpose |
|---|---|
| `agents` | Agent registry: name, persona, trust level, LLM config, DB path. |

## Core Flows

### Agent Turn (the main loop)

```
User message arrives (CLI / HTTP / Slack / Discord / Telegram / sidecar)
  │
  ├── 1. Store message in log
  ├── 2. Assemble context (token-budgeted across 9 segments)
  │       ├── system prompt / persona
  │       ├── active deny policies (so LLM knows constraints)
  │       ├── session summary (rolling, bridges across turns)
  │       ├── ALL fact pointers (~2K tokens for 500 facts)
  │       ├── expanded facts (relevance-matched to current query)
  │       ├── scratch pad (session working memory)
  │       ├── conversation log (last 20 messages)
  │       ├── knowledge chunks (relevance-matched)
  │       └── current message
  ├── 3. Policy-check the incoming message
  ├── 4. LLM generation (with tool definitions)
  ├── 5. Parse tool calls → policy-check each → execute → collect results
  │       (up to 10 rounds, with loop-break guards)
  ├── 6. Final LLM call with tool results → natural language response
  ├── 7. Redact secrets (18 regex patterns, always-on)
  ├── 8. Store assistant response
  └── Post-turn async:
        ├── Fact extraction (LLM-driven, dedup, policy-gated)
        ├── Embed pending facts + chunks
        └── Session summarization
```

### Canonical Operations

Every action (user-initiated or agent-initiated) flows through `operations::execute()`:

```
OperationRequest { op, args, actor, idempotency_key, ... }
  │
  ├── Idempotency check (replay stored response if seen)
  ├── Pre-hooks (configurable deny/transform from operation_hooks table)
  ├── Policy evaluation (every op, even reads)
  ├── Dispatch to handler (one of ~35 named operations)
  ├── Post-hooks (redaction, truncation, etc.)
  ├── Store idempotency record
  └── Return OperationResponse { ok, code, data, policy, audit }
```

### Search Pipeline

```
Query text + optional embedding
  │
  ├── FTS5/LIKE text search across 5 stores
  ├── Vector KNN search across 5 stores (if embedding provided)
  ├── Reciprocal Rank Fusion (K=60) across all ranked lists
  ├── Store-weight application (facts 0.4, knowledge 0.4, log 0.2)
  ├── Maximal Marginal Relevance re-ranking (lambda=0.7)
  └── Top-K results with source attribution
```

### Policy Evaluation

```
PolicyRequest { actor, action, resource, sql_content?, channel?, arguments? }
  │
  ├── Load all enabled policies (priority DESC)
  ├── For each: match patterns (glob) + optional behavioral rule check
  │       ├── rate_limit: count tool_calls in time window
  │       ├── retry_loop: detect repeated (tool, args) combos
  │       ├── token_budget: estimate session token usage
  │       └── time_window: check hour/weekday constraints
  ├── First match wins → Allow | Deny | Audit
  ├── No match → fall back to PolicyMode (allow_by_default | deny_by_default)
  └── Log decision to policy_audit
```

## Configuration

All config lives in `moneypenny.toml` (TOML format). Key sections:

| Section | Controls |
|---|---|
| `data_dir` | Path to agent databases and model files |
| `[gateway]` | Host, port, log level |
| `[[agents]]` | Agent name, persona, trust level, policy mode |
| `[agents.llm]` | Provider (anthropic/http/local), model, API key |
| `[agents.embedding]` | Provider (local/http), model, dimensions |
| `[[agents.mcp_servers]]` | External MCP server connections |
| `[channels]` | CLI, HTTP, Slack, Discord, Telegram |
| `[sync]` | CRDT peers, tables, interval, cloud URL |

## Extension Points

- **Tools:** Built-in (5), runtime self-awareness (19), MCP servers (any), JS tools (stored in DB)
- **Policies:** Stored in DB, CRUD via CLI/API, plan/confirm/apply workflow
- **Skills:** Stored in DB, usage-tracked, promotable, sync-able
- **Jobs:** Cron-scheduled, four types (prompt/tool/js/pipeline), policy-gated
- **Hooks:** Pre/post operation hooks stored in DB, configurable per-op-pattern
- **Channels:** Trait-based adapters, progressive capability model

## How to Build and Run

```bash
git clone --recurse-submodules https://github.com/jacobprall/moneypenny.git
cd moneypenny && cargo build

mp init                # creates moneypenny.toml + downloads local embedding model
mp start               # starts gateway (spawns worker per agent)
mp chat                # interactive REPL
mp sidecar             # canonical operations over stdio JSONL
```

## Module Specs

For detailed per-module documentation, see:

- [`specs/mp-core.md`](specs/mp-core.md) — Core library (operations, policy, search, stores, tools, sync)
- [`specs/mp-llm.md`](specs/mp-llm.md) — LLM abstraction layer (providers, types, embeddings)
- [`specs/mp-ext.md`](specs/mp-ext.md) — SQLite extension build and loader
- [`specs/mp-binary.md`](specs/mp-binary.md) — Binary crate (CLI, adapters, turn loop, workers)
