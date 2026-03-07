# AgentSQL

**The autonomous AI agent where the database is the runtime.**

AgentSQL is an open-source, local-first AI agent platform built on SQLite. Unlike conventional agent frameworks that bolt persistence onto an orchestration layer, AgentSQL inverts the stack: inference, memory, search, sync, policy, and tool execution all happen inside the same transactional boundary. The result is an agent that never forgets, never leaks, and works everywhere — including offline.

---

## Why another agent?

Every major open-source agent today — OpenClaw, LangChain, CrewAI, AutoGen — follows the same pattern: a Node.js or Python orchestrator that calls LLM APIs, dispatches tools, and writes state to disconnected storage (files, Redis, Postgres, etc.). Memory is an afterthought. Governance doesn't exist. Offline is an aspiration.

This architecture has fundamental problems:

- **No transactional guarantees.** If the agent crashes mid-task, you get orphaned state — partial memory writes, lost audit entries, half-executed tool chains.
- **No governance.** Nothing prevents the agent from dropping a production table, leaking a secret into a transcript, or running the same destructive query in a retry loop.
- **Cloud-dependent.** Most agents require API keys for inference and embedding. No network, no agent.
- **No built-in sync.** Each agent instance is an island. You can hack around this with git or shared databases, but there's no automatic, conflict-free knowledge propagation between agents.

AgentSQL solves these by making the database the agent's runtime — not just its storage.

---

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│  Channels (Slack, Discord, CLI, Web, Telegram, API, ...)        │
│  Thin adapters — message in, response out                       │
├─────────────────────────────────────────────────────────────────┤
│  Orchestrator                                                   │
│  Conversation management · Tool dispatch · Streaming            │
│  Heartbeat scheduler · Session lifecycle                        │
├─────────────────────────────────────────────────────────────────┤
│  AgentSQL Core (SQLite + extensions)                            │
│                                                                 │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────────────┐   │
│  │sqlite-ai │ │sqlite-   │ │sqlite-   │ │sqlite-agent      │   │
│  │          │ │vector    │ │memory    │ │                  │   │
│  │ Inference│ │          │ │          │ │ Autonomous tool  │   │
│  │ Embed    │ │ Vector   │ │ Hybrid   │ │ use via MCP      │   │
│  │ Chat     │ │ search   │ │ search   │ │ Table extraction │   │
│  │ Audio    │ │ SIMD     │ │ Chunking │ │ Auto-embeddings  │   │
│  │ Vision   │ │ Quantize │ │ FTS5+vec │ │                  │   │
│  └──────────┘ └──────────┘ └──────────┘ └──────────────────┘   │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────────────┐   │
│  │sqlite-js │ │sqlite-   │ │sqlite-   │ │Policy engine     │   │
│  │          │ │sync      │ │rag       │ │                  │   │
│  │ Custom   │ │          │ │          │ │ Audit trail      │   │
│  │ JS funcs │ │ CRDTs    │ │ Hybrid   │ │ Secret redaction │   │
│  │ in SQL   │ │ Offline  │ │ RRF      │ │ Rule enforcement │   │
│  │          │ │ Multi-   │ │ Multi-   │ │ Query governance │   │
│  │          │ │ device   │ │ format   │ │                  │   │
│  └──────────┘ └──────────┘ └──────────┘ └──────────────────┘   │
│                                                                 │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │ SQLite (single file, encrypted at rest)                  │   │
│  │ Sessions · Messages · Memory · Embeddings · Policies     │   │
│  │ Audit log · Contexts · Tool calls · Sync metadata        │   │
│  └──────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
```

Everything below the orchestrator is **a single SQLite database** with loaded extensions. Everything above it is a thin message adapter. The orchestrator itself is a small loop: receive message → retrieve context → check policy → call LLM → dispatch tools → store results → respond. The intelligence lives in the data layer.

---

## Differentiators

### Everything is a transaction

When an AgentSQL agent processes a message, the entire operation — memory retrieval, policy check, tool execution, audit logging, embedding generation, secret redaction — happens inside a single SQLite transaction. If anything fails, everything rolls back. No orphaned state. No partial writes. No lost audit entries.

No other agent framework can make this claim.

### Offline-first, for real

AgentSQL runs full agent capabilities with zero network connectivity. Inference runs on-device via GGUF models through `sqlite-ai`. Embeddings are generated locally. Search is local. Memory is local. There is no degraded mode — offline *is* the mode. Network is an optimization, not a requirement.

### Governed by default

Every tool call passes through a configurable policy engine before execution. Block destructive SQL. Require WHERE clauses on DELETE. Log all DDL. Deny operations on production tables. Eighteen regex patterns scrub secrets (API keys, tokens, PEM keys, connection strings) before any data touches disk. A complete audit trail records every query, tool call, and policy decision.

This is table stakes for enterprise use. No other open-source agent has it.

### Multi-agent memory mesh

`sqlite-sync` enables something no other agent platform offers: multiple agent instances sharing a synchronized memory database across devices, with offline-first CRDT conflict resolution. Agent A discovers a pattern on one machine. That knowledge propagates to Agent B on another machine — no central server, no cloud dependency, no manual sharing. Each agent gets smarter as the mesh grows.

### Embeddable everywhere

The entire agent runtime is a stack of SQLite extensions. Anything that can load a SQLite extension can host an AgentSQL agent: mobile apps (iOS, Android), desktop apps, IoT devices, browsers (via WASM + OPFS), CLI tools, server processes. One portable `.db` file contains the complete agent state — memory, embeddings, audit history, policies, sync metadata.

### Hybrid search with Reciprocal Rank Fusion

Memory retrieval combines vector similarity search with FTS5 full-text search, merged via Reciprocal Rank Fusion. Vector search handles semantic similarity ("find notes about deployment issues" matches "CI/CD pipeline failures"). Full-text search handles exact tokens (error codes, variable names, UUIDs). RRF merges both ranking signals without requiring score calibration.

---

## What exists today

AgentSQL is built on a mature extension ecosystem. These are not prototypes — they ship with cross-platform binaries, package manager distribution (npm, pip, pub, Maven, Swift PM), and test coverage:

| Component | Status | What it does |
|---|---|---|
| `sqlite-ai` | Shipping | On-device inference, embeddings, chat, audio transcription, vision via GGUF |
| `sqlite-vector` | Shipping | Vector search with SIMD, quantization, 6 distance metrics, 6 vector types |
| `sqlite-memory` | Shipping | Persistent agent memory with hybrid search, markdown-aware chunking |
| `sqlite-rag` | Shipping | Hybrid search engine with RRF, multi-format document processing |
| `sqlite-sync` | Shipping | CRDT-based offline-first sync with row-level security |
| `sqlite-agent` | Shipping | Autonomous agents inside SQLite with MCP tool integration |
| `sqlite-js` | Shipping | User-defined JavaScript functions, aggregates, window functions |
| `sqlite-wasm` | Shipping | Browser runtime with OPFS persistence |
| Policy engine | In coco-db | Rule evaluation, audit, enforcement, secret redaction |

## What needs to be built

| Component | Description |
|---|---|
| Orchestrator | Thin agent loop: message → context → policy → LLM → tools → store → respond |
| Channel adapters | Message ingress/egress for Slack, Discord, Telegram, CLI, Web, API |
| Heartbeat scheduler | Autonomous task execution on configurable intervals |
| Configuration system | Agent config as SQL tables, synced across devices via CRDTs |
| Web UI | Conversation interface, memory browser, audit viewer, policy editor |

The hard infrastructure — the data layer — is done. What remains is the orchestration shell and channel adapters. These are the *easy* parts of an agent system.

---

## Vision

A world where AI agents are as reliable, auditable, and portable as the databases they run on. Where "works offline" isn't a footnote but the default. Where a team's collective knowledge lives in a synchronized, governed, encrypted data layer — not scattered across Markdown files and API call logs. Where you can embed a full-capability agent into a mobile app, a CLI tool, or a browser tab with a single SQLite file.

AgentSQL: the agent that remembers everything, leaks nothing, and runs anywhere.

**add my vision from other doc**
