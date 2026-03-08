# Moneypenny

**Enterprise-grade Edge AI agent runtime.**

Moneypenny is an open-source agent platform with structured memory, context optimization, policy-governed execution, single-transaction ACID turns, local-first/offline operation, and selective multi-agent knowledge sync.

## Quick Start

```bash
git clone --recurse-submodules https://github.com/jacobprall/moneypenny.git
cd moneypenny && cargo build

mp init       # creates config + downloads local embedding model
mp start      # starts gateway (CLI chat, HTTP API, optional Web UI)

> hello
> remember that our team standup is at 9:15am Pacific
> what time is standup?
```

Embeddings are generated locally by default. Optional Web UI: build `web-ui` and serve `web-ui/dist` from the same port.

---

## Four Natural Groupings

- **Correctness Core.** Single ACID turn semantics, database-as-runtime execution, and rollback guarantees that prevent partial state.
- **Governance and Security.** Policy checks on every action, denial-aware control flow, auditable decisions, secret redaction, and encryption at rest.
- **Intelligence System.** Structured memory, adaptive compression, hybrid retrieval, and graph-linked facts that improve quality over time.
- **Scale and Portability.** CRDT sync, scoped knowledge sharing, and one-file portability across devices and deployment environments.

---

## Features & Capabilities

### Memory that persists, propagates and compounds

- **Structured, compressed, self-curating.** After every turn, an extraction pipeline distills new knowledge into curated facts — add, update, delete, or ignore based on what the agent already knows. Facts evolve; confidence grows when re-extracted, stale knowledge decays with configurable half-life.
- **Three compression levels.** Full detail, summary, and 2–5 word pointers. The agent loads all knowledge as pointers (~2K tokens for 500 facts), then auto-expands only what’s relevant.
- **Graph-linked facts.** Traversable edges between related facts. “What do I know about X?” follows the graph without independent retrieval.
- **Hybrid retrieval.** Vector similarity + full-text via Reciprocal Rank Fusion. Results deduplicated, MMR diversity-ranked, policy-filtered at SQL. Store weights adapt by intent (facts vs knowledge vs history).
- **Cheap extraction.** A small local model (e.g. 3B) can run extraction while the main conversation uses Claude or GPT-4. Memory management stays fast and local.

### Your data never leaves your machine

- Full agent capabilities (inference, embeddings, memory, search) with **zero network**. Local GGUF. No “degraded mode” — offline is the default. Cloud is optional.
- Default: Anthropic for generation, local GGUF for embeddings. Embeddings never leave the machine. Switch to fully local GGUF for zero-network operation.

### Every action is governed

- **Policy engine** evaluates every tool call, SQL query, memory write, fact extraction, and delegation before execution. One model: `allow(actor, action, resource)`.
- **Static rules:** block destructive ops, require WHERE on DELETE, restrict tools by trust level, scope by channel. **Behavioral rules:** rate-limit shell access, detect retry loops, per-session token budgets, time-window (cron) restrictions.
- **Secret redaction.** Eighteen regex patterns scrub API keys, JWTs, PEMs, connection strings before anything touches disk. Always on.
- **Denials as context.** Policy denials don’t crash the agent; they’re returned so the agent can adapt. Stuck detection breaks retry loops and context thrashing.
- **Queryable audit trail.** Every decision logged. “Why was this denied?” “How many violations this week?” Answerable with SQL.

### Nothing is ever half-done

- **Single ACID transaction per turn.** Memory retrieval, policy check, tool execution, audit, embeddings, redaction — all in one transaction. Any failure rolls back. No orphaned state, no partial writes.
- In local mode the agent loop runs entirely inside SQLite. Crash mid-task → WAL rollback to last consistent state. Architecturally impossible when persistence is bolted onto an orchestrator.

### Agents get smarter together

- **CRDT-based sync.** Multiple agents share a knowledge mesh. No central server. Agent A’s discoveries propagate to Agent B automatically.
- **Scoped knowledge.** Facts are `private` by default; promoted to `shared` by extraction or admin. `Protected` for trusted agents only. Scope enforced at SQL — agents cannot query outside their authorization.
- Facts, knowledge, skills, policies, jobs (and run history) sync. Conversations and scratch stay local. Governance rules propagate fleet-wide.

### Skills that learn and propagate

- Skills track usage and success; high-performing ones surface more in retrieval. They sync across agents.
- **Tools from anywhere:** built-in, MCP-discovered, user-defined JS in the DB. Hybrid search surfaces tools by intent. JS tools persist, survive restarts, and sync.

### One file, runs everywhere

- Entire agent state in **one portable SQLite file.** Back up by copying. Inspect with any SQLite client. Move to another machine and it works. Embeddable: mobile, desktop, IoT, browser (WASM + OPFS), CLI, server.

### Encrypted at rest

- SQLCipher for the agent DB. Keys in OS keychain (macOS Keychain, Linux keyring, Windows Credential Manager) — never plaintext on disk. Sync over TLS; data at rest encrypted on each endpoint.

---

## How It Works

- **Database as runtime.** Inference, memory, search, sync, policy, and tools live inside the same transactional boundary. The orchestrator is a thin loop; the intelligence (compression, budgeting, extraction, governance) sits between the DB and the LLM.
- **Four memory stores, one search.** Facts, conversation log, ingested knowledge, session scratch — one hybrid retrieval layer. Token budgeting allocates context across stores by query, session depth, and task.
- **Channels.** CLI, HTTP (REST + SSE + WebSocket), Slack, Discord, Telegram. Same agent loop; channel adapters are thin.
- **Multi-agent delegation.** Governed channel; policy controls who can delegate, depth limits, token caps. Delegated agent runs its own full loop and returns results.
- **Scheduled jobs.** Cron, pipelines, self-reflection prompts, custom JS — same policy engine. Jobs sync: define once, propagate.
- **Observable.** Prometheus-style metrics, structured logs, health endpoints, queryable audit trail.

---

## OpenClaw Integration (Execution Plane + Intelligence Plane)

Moneypenny can integrate with OpenClaw.

__In short: Moneypenny becomes the layer that makes OpenClaw outputs compounding, explainable, and safely reusable across sessions and agents.__

- **OpenClaw** is the execution/control plane (channels, nodes, browser automation, webhooks, device actions).
- **Moneypenny** is the intelligence/data plane (durable memory, policy governance, audit analytics, cross-session retrieval, and sync).

OpenClaw handles breadth of interfaces and action routing; Moneypenny turns logs and events into governed, queryable long-term intelligence.


## Built On

Seven [SQLite AI](https://github.com/nicholasgasior/sqliteai) extensions, statically linked into one binary:

| Component      | Role |
|----------------|------|
| `sqlite-ai`    | On-device LLM inference, embeddings, chat, audio, vision (GGUF) |
| `sqlite-vector`| Vector search, SIMD, quantization, 6 distance metrics |
| `sqlite-memory`| Persistent agent memory, hybrid search, markdown chunking |
| `sqlite-rag`   | Hybrid search, RRF, multi-format docs |
| `sqlite-sync`  | CRDT offline-first sync, row-level security |
| `sqlite-agent`| Agent execution in SQLite, MCP tools |
| `sqlite-js`    | User-defined JS functions, aggregates |
| `sqlite-wasm`  | Browser runtime, OPFS persistence |


---

## Why SQLite

Only SQLite delivers all of this together:

- **ACID** over full agent state (memory, tools, audit, policy)
- **Zero infra** — no Postgres, Redis, Docker, or cloud
- **Single-file** — backup, move, inspect, embed
- **Offline-first** — same behavior with or without network
- **Encrypted at rest** — opaque without the key

Other frameworks force tradeoffs. Moneypenny gets these from the foundation.

---

## Documentation

| Doc | Description |
|-----|-------------|
| [SPEC.md](./SPEC.md) | Architecture, schemas, algorithms |
| [PLAN.md](./PLAN.md) | Roadmap, milestones |
| [TASKS.md](./TASKS.md) | Current status, active work |
| [OPENCLAW_INTEGRATION.md](./docs/OPENCLAW_INTEGRATION.md) | V1 OpenClaw integration contract and event mapping |

---

*Deep memory. Governed actions. One file. Moneypenny: remembers everything, leaks nothing, runs anywhere.*
