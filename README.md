# Moneypenny

**Enterprise-grade local-first AI agent platform.**

Moneypenny is an open-source agent platform with state-of-the-art memory, context optimization, and retrieval — plus the enterprise features no other framework has: offline execution, ACID transactions, policy governance, secret redaction, encrypted storage, and selective multi-agent knowledge sync. All from a single portable file.

---

## The Problem

Open-source AI agents today are built the same way: a Python or Node orchestrator calls cloud APIs, dispatches tools, and scatters state across files, Redis, and Postgres. This works for demos. It breaks for anything real.

- **No crash recovery.** Agent dies mid-task and you get orphaned state, partial memory writes, and lost audit entries. No rollback.
- **No secret redaction.** API keys, tokens, and credentials end up in transcripts and logs with nothing to stop them.
- **No guardrails.** Nothing stands between the LLM and your infrastructure. One bad tool call drops a production table.
- **No offline mode.** No network, no agent. Full stop.
- **No knowledge sharing.** Each agent is an island. Syncing what they learn is your problem to solve.
- **No context optimization.** Everything gets stuffed into the context window with no compression or prioritization.

These characteristics make open-source agents unusable for anything an enterprise would trust.

---

## What Makes Moneypenny Different

### Memory that compounds, not just persists

Most agent memory is a flat append log — dump everything into context and hope the LLM sorts it out. Moneypenny's memory is structured, compressed, self-curating, and graph-linked.

After every conversation turn, an async extraction pipeline distills new knowledge into curated facts — deciding what to **add**, **update**, **delete**, or **ignore** based on what the agent already knows. No manual saving. Facts evolve over time as they're validated across conversations. Confidence increases when the same fact is independently re-extracted; stale knowledge decays with configurable half-life.

Every fact is stored at three compression levels: full detail, summary, and a 2–5 word pointer. The agent loads *all* its knowledge as pointers in every prompt — 500 facts cost ~2K tokens — then auto-expands the relevant ones and pulls full detail on demand. Conversations are incrementally summarized so long sessions stay sharp without blowing the context window.

Facts link to related facts in a traversable graph. Ask "what do I know about ORDERS?" and the agent follows the edges — soft deletes, indexing strategies, migration history — without needing each fact to be independently retrieved.

Retrieval combines vector similarity with full-text search via Reciprocal Rank Fusion — semantic meaning and exact tokens (error codes, function names, UUIDs) both work. Results are deduplicated across stores, diversity-ranked via MMR, and policy-filtered at the SQL level before they reach the LLM. Store weights shift automatically by intent: more facts for "what do I know about X," more knowledge for "how do I do X," more history for "when did we discuss X."

The extraction model can be a small, cheap local model (3B parameters is sufficient) even when the conversational model is Claude or GPT-4. Memory management stays fast and nearly free.

### Your data never leaves your machine

Moneypenny runs full agent capabilities — inference, embeddings, memory, search — with zero network connectivity. Local models via GGUF. No cloud API required. No "degraded mode." Offline *is* the mode. Cloud is an optional upgrade, not a dependency.

The default setup uses Anthropic for generation and local GGUF for embeddings — cloud intelligence with local privacy for all vector operations. Embeddings never leave your machine. Switch to fully local GGUF inference for zero-network operation.

### Every action is governed

A built-in policy engine evaluates every tool call, SQL query, memory write, fact extraction, and agent delegation before it executes. One universal model governs everything: `allow(actor, action, resource)`.

**Static rules** block destructive operations, require WHERE clauses on DELETE, restrict tools by agent trust level, and scope access by channel. **Behavioral rules** go further: rate-limit shell access per time window, detect and break retry loops, enforce per-session token budgets, restrict operations to specific hours via cron.

Eighteen regex patterns scrub secrets — API keys, JWTs, PEM keys, connection strings, database URIs — before anything touches disk. Always on. Not optional.

Policy denials don't crash the agent — they're returned as context. The agent sees "destructive shell operations are blocked by policy" and adapts its approach. Stuck detection catches retry loops, error clusters, and context thrashing, injecting diagnostic prompts to break the cycle.

Every decision is logged to a queryable audit trail. "Why was this tool call denied?" "How many policy violations this week?" "Which agent triggers the most audits?" All answerable with a SQL query.

### Nothing is ever half-done

When the agent processes a message, the entire operation — memory retrieval, policy check, tool execution, audit logging, embedding generation, secret redaction — happens inside a single ACID transaction. If anything fails, everything rolls back. No orphaned state. No partial writes. No lost audit entries.

In local mode, the agent loop runs entirely inside SQLite — goal evaluation, tool selection, MCP execution, result processing — as a single transaction. In cloud mode, the Rust orchestrator wraps each turn in a transaction. Either way, crash mid-task means WAL rollback to the last consistent state.

This guarantee is architecturally impossible in frameworks that bolt persistence onto an orchestrator.

### Agents get smarter together

Multiple agents share a synchronized knowledge mesh via conflict-free replication (CRDTs). Agent A discovers a pattern on one machine — that knowledge automatically propagates to Agent B on another. No central server. No cloud dependency. No manual sharing.

Shared knowledge is scoped: facts start `private` by default, get promoted to `shared` when the extraction pipeline detects project-level knowledge, when multiple agents independently converge on the same fact, or when an admin promotes explicitly. `Protected` scope restricts to trusted agents. Scope is enforced at the SQL level — agents physically cannot query facts outside their authorization.

Facts, knowledge, skills, policies, jobs, and job run history all sync. Conversation logs and scratch stay local. Governance rules propagate fleet-wide automatically.

### Skills that learn and propagate

Skills are more than static tool definitions. They're living capabilities with usage tracking, success rates, and automatic promotion. High-performing skills surface more readily in retrieval. Skills sync across agents — Agent A learns a new capability and every agent in the mesh inherits it.

Agents discover tools by intent, not by static registration. Describe what you need and hybrid search surfaces relevant tools from any source — built-in, MCP-discovered, or user-defined JavaScript functions stored in the database. The agent works with an unbounded tool surface.

### One file. Runs everywhere.

The entire agent — memory, embeddings, policies, audit trail, sync metadata, skills, job schedules — lives in a single portable SQLite file. Anything that can open a SQLite database can host a Moneypenny agent: mobile apps, desktop apps, IoT devices, browsers (via WASM + OPFS), CLI tools, server processes.

Back it up by copying a file. Inspect it with any SQLite client. Move it to another machine and it just works.

### Encrypted at rest

Every agent database is encrypted via SQLCipher. Keys are stored in your OS keychain (macOS Keychain, Linux kernel keyring, Windows Credential Manager) — never written to disk in plaintext. A lost device means zero leaked agent memory. Sync transport uses TLS; data at rest on each endpoint is encrypted independently.

---

## Quick Start

```bash
git clone --recurse-submodules https://github.com/jacobprall/moneypenny.git
cd moneypenny && cargo build

mp init       # creates config + downloads local embedding model
mp start      # starts agent with CLI chat

> hello
> remember that our team standup is at 9:15am Pacific
> what time is standup?
```

Three commands to a working agent with persistent memory. No Docker. No database setup. Embeddings run locally out of the box.

---

## How It Works

Moneypenny inverts the typical agent architecture. Instead of an orchestrator with persistence bolted on, the database *is* the runtime. Inference, memory, search, sync, policy, and tool execution all happen inside the same transactional boundary. The orchestrator is a thin loop on top.

**Four memory stores, one search interface.** Curated facts, conversation history, ingested knowledge, and session scratch — all searchable through a single hybrid retrieval layer. Smart token budgeting dynamically allocates context across stores based on the query, the session depth, and what the agent is working on.

**Tools from anywhere.** Built-in tools, MCP-discovered tools, and user-defined JavaScript functions all go through the same policy-governed lifecycle. JS tools persist in the database, survive restarts, and sync across agents.

**Multi-agent delegation.** Agents delegate tasks to each other through a governed internal channel. Policy controls who can delegate to whom, with depth limits to prevent infinite chains and token budgets to cap costs. The delegated agent processes the task through its own full loop — its own memory, tools, and policies — and returns results.

**Scheduled autonomy.** A SQLite-native job scheduler runs cron tasks, multi-step pipelines, prompted self-reflection, and custom JS functions — governed by the same policy engine as everything else. Jobs sync across agents: define once, propagate fleet-wide.

**Channels.** Thin bidirectional adapters for CLI, HTTP API, Slack, Discord, and more. Channel-specific features (threads, reactions, file uploads, streaming) are progressive enhancements. The agent loop doesn't change regardless of where the message comes from.

**Observable.** Prometheus-compatible metrics, structured JSON logging, health check endpoints, and the queryable audit trail. You can always answer: what is the agent doing, why did it do that, and is it healthy.

---

## Built On Battle-Tested Infrastructure

Moneypenny is assembled from a mature ecosystem of SQLite extensions that ship today with cross-platform binaries and package manager distribution:

| Component | What it does |
|---|---|
| `sqlite-ai` | On-device LLM inference, embeddings, chat, audio, vision via GGUF |
| `sqlite-vector` | Vector search with SIMD optimization, quantization, 6 distance metrics |
| `sqlite-memory` | Persistent agent memory with hybrid search and markdown-aware chunking |
| `sqlite-rag` | Hybrid search engine with RRF, multi-format document processing |
| `sqlite-sync` | CRDT-based offline-first sync with row-level security |
| `sqlite-agent` | Autonomous agent execution inside SQLite with MCP tool integration |
| `sqlite-js` | User-defined JavaScript functions, aggregates, and window functions |
| `sqlite-wasm` | Browser runtime with OPFS persistence |


Everything compiles into a **single Rust binary** with all extensions statically linked. No runtime dependencies. No dynamic libraries. No downloads at runtime.

---

## Why SQLite?

It's the only architecture that delivers all of these simultaneously:

- **ACID transactions** across the full agent state (memory + tools + audit + policy)
- **Zero infrastructure** — no Postgres, no Redis, no Docker, no cloud
- **Single-file portability** — back up, move, inspect, embed
- **Offline-first** — works identically with or without a network
- **Embeddable** — runs inside any app on any platform
- **Encrypted at rest** — opaque without the key

Every other agent framework requires you to *choose* between these properties. Moneypenny gets them for free from its foundation.

---

## Documentation

| Document | Description |
|---|---|
| [SPEC.md](./SPEC.md) | Full technical specification — architecture, schemas, algorithms |
| [PLAN.md](./PLAN.md) | Implementation roadmap with milestones and dependency graph |
| [TASKS.md](./TASKS.md) | Current development status and active work |

---

## Design Values

- **Rock-solid.** ACID transactions. Deny-by-default policies. Secret redaction always on. Full audit trail. Crash recovery via WAL.
- **Simple.** Few moving parts. Convention over configuration. Three commands to a working agent.
- **Governed.** Every action is auditable. Every resource is policy-controlled. Every secret is redacted. Trust is earned through visibility.
- **Seamless.** No seams between memory, search, policy, tools, and sync. Everything works together inside one transactional boundary.
- **Observable.** Health checks, metrics, structured logs, audit trail. You can always answer: what happened and why.

**Moneypenny: remembers everything, leaks nothing, runs anywhere.**
