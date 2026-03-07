# Moneypenny — Implementation Plan

## Milestone 1: Project Scaffolding & Binary

- Rust workspace setup (`Cargo.toml`, crate structure)
- Static linking of SQLite + all extensions (sqlite-ai, sqlite-vector, sqlite-memory, sqlite-rag, sqlite-sync, sqlite-agent, sqlite-js)
- `mp` binary skeleton with `clap` CLI parser
- TOML config parsing (`moneypenny.toml`)
- Stub `mp init` / `mp start` / `mp stop` commands

**Exit criteria:** `cargo build` produces a single `mp` binary that parses config and prints help.

---

## Milestone 2: Database Layer & Schema

- Agent database creation and lifecycle management
- All core schemas:
  - Facts (`facts`, `fact_links`, `fact_audit`)
  - Log (`sessions`, `messages`, `tool_calls`)
  - Knowledge (`documents`, `chunks`, `edges`, `skills`)
  - Scratch (`scratch`)
  - Policies (`policies`, `policy_audit`)
  - Jobs (`jobs`, `job_runs`)
- Gateway `metadata.db` (agent registry, routing)
- Schema migration system

**Exit criteria:** `mp init` creates a valid agent `.db` with all tables. Schemas match the spec. Migrations are versioned and repeatable.

---

## Milestone 3: LLM Provider Trait

- Define `LlmProvider` trait (`generate`, `embed`, `supports_streaming`)
- `SqliteAiProvider` — local GGUF inference via `sqlite-ai`
- `HttpProvider` — OpenAI-compatible API (works with OpenAI, Anthropic proxy, Ollama, vLLM)
- Per-agent provider configuration (e.g., local embeddings + cloud generation)
- Model download on first use for local provider

**Exit criteria:** Both providers pass a basic generate + embed test. Provider is selectable via config.

---

## Milestone 4: Memory Stores (CRUD)

- **Facts:** Create, read, update, delete. Three compression levels (full / summary / pointer). Embeddings at all levels. Fact linking via `fact_links`. Temporal decay + confidence scoring.
- **Log:** Append-only writes for sessions, messages, tool calls. Secret redaction before write.
- **Knowledge:** Document ingestion (parse, chunk at ~2000 chars, embed, index). Discovery-tier summaries. Skills as specialized knowledge with usage tracking.
- **Scratch:** Session-scoped key-value store. Promotion of durable findings to Facts at session end.

**Exit criteria:** Each store supports full CRUD via Rust API. Facts store and retrieve at all three compression levels. Knowledge ingests a markdown file and produces searchable chunks.

---

## Milestone 5: Search

- Hybrid search engine: FTS5 (BM25) + vector similarity (cosine via `sqlite-vector`) + Reciprocal Rank Fusion (k=60)
- Cross-store search (Facts, Log, Knowledge) with source tagging
- MMR re-ranking for diversity (λ=0.7, Jaccard similarity)
- Store weighting by query intent (keyword-based detection)
- Cross-store deduplication (cosine > 0.92 grouping)

**Exit criteria:** A single search query returns deduplicated, re-ranked results spanning all three stores with correct source attribution.

---

## Milestone 6: Context Assembly & Token Budget

- Token budget allocator with reserved + flexible segments
- Context assembly pipeline: system prompt → policy rules → fact pointers (all L2) → auto-expanded facts (L1) → scratch → log retrieval → knowledge retrieval → current message
- Dynamic rebalancing (empty scratch, new session, deep session)
- Rolling conversation summary (incremental, every N turns)
- Per-agent budget override

**Exit criteria:** Given a message, the assembler produces a complete prompt within the token budget, with correct allocation across stores.

---

## Milestone 7: Policy Engine

- Custom Rust rule engine (configurable allow/deny default)
- Two-layer interface: SQL rows (static rules), behavioral rules (rate_limit, retry_loop, token_budget, time_window)
- SQL filter generation (policy → WHERE clause for data queries)
- Secret redaction (18 regex patterns, always-on, pre-storage)
- Policy audit trail (`policy_audit` table)
- Behavioral policies: rate limiting, retry loop detection, token budget enforcement

**Exit criteria:** `allow(actor, action, resource)` evaluates correctly for tool calls, SQL execution, fact access, and channel triggers. SQL filter generation produces valid WHERE clauses. Secret redaction catches all 18 patterns.

---

## Milestone 8: Extraction Pipeline

- Async post-turn pipeline (does not block agent response)
- Extraction context assembly: new messages + rolling summary + top-K existing facts
- LLM extraction call → structured JSON candidates (content, summary, pointer, keywords, confidence, scope)
- Deduplication: vector similarity against existing facts
- LLM decision per candidate: ADD / UPDATE / DELETE / NOOP
- Fact linking on new/updated facts (embedding similarity → edges)
- Policy check on candidates (PII, scope, content)
- Transactional commit with full audit trail
- Support for smaller/cheaper extraction model separate from conversational model

**Exit criteria:** After a conversation turn, the pipeline extracts facts, deduplicates against existing facts, and commits with audit entries. Revisiting a compressed fact triggers re-extraction.

---

## Milestone 9: Tool System

- **Built-in tools:** File I/O, shell exec, HTTP requests, SQL queries
- **MCP tools:** Dynamic discovery via `tools/list`, runtime registration
- **sqlite-js tools:** User-defined JS functions, persisted in SQLite, syncable
- Tool registration in SQLite (all sources unified)
- Tool discovery via RAG (hybrid search on tool/skill descriptions)
- Tool lifecycle: register → discover → request → policy check → execute → post-audit → result + redaction

**Exit criteria:** Agent can discover tools by intent, request a call, pass policy evaluation, execute, and receive redacted results. All three tool sources work.

---

## Milestone 10: Agent Loop & Streaming

- Core loop: message → context assembly → policy → LLM → tool calls → store → respond
- Dual mode:
  - **Local:** `sqlite-agent` runs the full loop inside SQLite as a single transaction
  - **Cloud:** Rust orchestrator calls external LLM via HTTP, wraps each turn in a SQLite transaction
- Streaming: token-by-token forwarding to channel, tool call interruption (pause → policy → execute → inject result → resume)
- Error handling: policy denial as tool result (agent adapts), tool error retry (max 3), stuck detection (retry loops, error clusters, context thrashing), crash recovery via WAL rollback

**Exit criteria:** A full turn completes end-to-end in both local and cloud modes. Streaming works. Policy denials are handled gracefully. Crash mid-transaction leaves DB consistent.

---

## Milestone 11: Job Scheduler

- Jobs table CRUD
- Four job types: `prompt`, `tool`, `js`, `pipeline`
- Scheduler loop in gateway: poll `jobs` table every second, dispatch due jobs
- Overlap policy (`skip`, `queue`, `allow`)
- Retry with configurable `max_retries` and `retry_delay_ms`
- Timeout enforcement
- `job_runs` tracking
- sqlite-js as job logic layer
- Policy governance on jobs

**Exit criteria:** A cron-scheduled job fires on time, executes within a transaction, records its run, and respects overlap/retry/timeout policies.

---

## Milestone 12: CLI Channel & Developer Experience

- Interactive CLI chat (`mp chat`)
- In-chat commands (`/help`, `/facts`, `/scratch`)
- All `mp` subcommands from spec (agent, facts, knowledge, skills, policy, jobs, audit, sync, db, health)
- `mp init` full experience: create config, data dir, download models, initialize default agent
- `mp start` full experience: start gateway + agent + CLI channel
- Human-readable output by default, `--json` flag for machine consumption

**Exit criteria:** The three-command onboarding flow works: `mp init` → `mp start` → first message → fact extraction → fact recall. All CLI subcommands functional.

---

## Milestone 13: Gateway & Multi-Agent

- Gateway process: message routing, agent registry, lifecycle management
- Worker isolation: one OS process per agent, exclusive DB write access
- Agent-to-agent communication via internal message channel
- Delegation tool (`delegate(agent_id, message)`) with policy governance and depth limiting
- Sync integration: Facts, Knowledge, Skills, Policies, Jobs propagate across agents via `sqlite-sync` CRDTs
- Fact scope model: `private` / `shared` / `protected` with policy enforcement

**Exit criteria:** Two agents running under one gateway can delegate tasks to each other, share facts via sync, and respect scope/policy boundaries.

---

## Milestone 14: Channel Adapters

- Channel trait implementation (`receive`, `send`, `capabilities`)
- Progressive enhancement via `ChannelCapabilities` (threads, reactions, files, streaming)
- HTTP API adapter (REST + WebSocket)
- Slack adapter
- Discord adapter

**Exit criteria:** An agent responds to messages from CLI, HTTP API, and Slack with channel-appropriate formatting. Streaming works on capable channels.

---

## Milestone 15: Observability

- Health check endpoint (`GET /health`) and `mp health` CLI
- Prometheus-compatible metrics (messages, tool calls, policy decisions, facts, LLM latency, token usage, job runs, sync ops, DB size)
- Structured JSON logging to stderr (per-component log levels)

**Exit criteria:** `mp health` prints a complete status report. Metrics endpoint returns valid Prometheus format. Logs are structured and filterable.

---

## Milestone 16: Encryption at Rest

- SQLite encryption via SEE or SQLCipher
- Platform-specific key storage:
  - macOS: Keychain
  - Linux: kernel keyring / key file
  - Windows: Credential Manager
  - WASM: passphrase-derived
- Transparent encrypt/decrypt on open/write
- Compatible with `sqlite-sync` (decrypt for sync, re-encrypt at rest)

**Exit criteria:** Agent `.db` files are opaque without the key. Encryption is transparent to all other components. Sync works across encrypted databases.

---

## Milestone 17: Ecosystem (Phase 3)

- Skill marketplace (shareable, versionable skill packs)
- Web UI (conversation view, memory browser, audit viewer, policy editor)
- WASM runtime (browser-native agent via `sqlite-wasm`)
- Additional channel adapters (Telegram, WhatsApp, iMessage, email)

**Exit criteria:** Defined per-feature as Phase 3 work is scoped.

---

## Dependency Graph

```
M1 (Scaffolding)
 └─► M2 (Schema)
      ├─► M3 (LLM Providers)
      ├─► M4 (Memory Stores)
      │    └─► M5 (Search)
      │         └─► M6 (Context Assembly)
      └─► M7 (Policy Engine)

M3 + M6 + M7 ─► M8 (Extraction Pipeline)
M3 + M7 + M9 (Tools) ─► M10 (Agent Loop)
M10 + M7 ─► M11 (Job Scheduler)
M10 ─► M12 (CLI & DX)

M10 ─► M13 (Gateway & Multi-Agent)
M10 ─► M14 (Channel Adapters)
M10 ─► M15 (Observability)
M2  ─► M16 (Encryption)

M13 + M14 ─► M17 (Ecosystem)
```
