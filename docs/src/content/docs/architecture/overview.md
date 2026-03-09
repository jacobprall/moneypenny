---
title: Architecture Overview
description: How Moneypenny's database-as-runtime architecture works
---

Moneypenny uses a **database-as-runtime** architecture where SQLite is not
just storage — it's the execution boundary for memory, policy, search, sync,
and tools.

## System Layers

```
┌─────────────────────────────────────────────┐
│  Channels (CLI, HTTP, Slack, Discord, TG)   │
├─────────────────────────────────────────────┤
│  Gateway (routing, worker management)        │
├─────────────────────────────────────────────┤
│  Agent Loop (context → policy → LLM → tool) │
├─────────────────────────────────────────────┤
│  Canonical Operation Layer                   │
├─────────────────────────────────────────────┤
│  SQLite + Extensions                         │
│  (AI, Vector, Memory, RAG, Sync, JS, MCP)   │
└─────────────────────────────────────────────┘
```

### Channels

Thin adapters that convert external protocols into internal messages.
No business logic lives here — channels translate between wire format and
the agent loop's message protocol.

Supported: CLI, HTTP (REST + SSE + WebSocket), Slack Events API, Discord
Interactions, Telegram long-polling.

### Gateway

Routes messages to agent workers, manages worker lifecycles, runs the
scheduler, and binds channel adapters to a shared HTTP server.

Each agent runs as a separate worker process with its own database file.

### Agent Loop

The core execution cycle for each turn:

1. **Context assembly** — load facts (as pointers), expand relevant ones,
   retrieve knowledge chunks, include session history, attach skills
2. **Policy evaluation** — check if the action is permitted
3. **LLM generation** — send assembled context to the configured provider
4. **Tool execution** — dispatch tool calls through the policy-gated pipeline
5. **Fact extraction** — distill new knowledge from the conversation
6. **Response** — return the result to the channel

### Canonical Operation Layer

All mutations and policy-relevant queries flow through a single pipeline.
No adapter-specific business logic — CLI, HTTP, sidecar, and MCP all compile
down to the same canonical operations.

See [Canonical Operations](/architecture/canonical-operations/) for details.

### SQLite + Extensions

The foundation. Seven extensions statically linked into one binary provide:

| Extension | Capability |
|---|---|
| `sqlite-ai` | On-device LLM inference, embeddings, chat (GGUF) |
| `sqlite-vector` | Vector search with SIMD acceleration |
| `sqlite-memory` | Persistent agent memory, hybrid search |
| `sqlite-rag` | Hybrid retrieval, RRF, multi-format docs |
| `sqlite-sync` | CRDT offline-first sync |
| `sqlite-js` | QuickJS sandbox for user-defined functions |

## Data Model

Each agent's state lives in a single SQLite file:

| Table | Purpose |
|---|---|
| `facts` | Long-term structured memory (content, summary, pointer) |
| `fact_links` | Graph edges between related facts |
| `messages` | Conversation history |
| `documents` | Ingested document metadata |
| `chunks` | Document chunks (FTS5 indexed) |
| `skills` | Reusable procedures |
| `policies` | Governance rules |
| `policy_audit` | Decision log |
| `jobs` | Scheduled task definitions |
| `job_runs` | Job execution history |
| `scratch` | Session-scoped working memory |

A separate `metadata.db` tracks the agent registry across all agents.

## Four Memory Stores, One Search

Facts, messages, knowledge chunks, and tool call records are all searchable
through a unified hybrid retrieval layer. FTS5 handles keyword matching,
vector search handles semantic similarity, and Reciprocal Rank Fusion merges
the results.

Token budgeting allocates context across stores based on query intent,
session depth, and task requirements.

## Design Principles

- **ACID over full agent state.** Every turn is a transaction. If the LLM
  call fails, memory and audit roll back cleanly.
- **No external dependencies.** SQLite provides storage, search, sync,
  and compute. No Postgres, Redis, or Docker required.
- **Policy is authoritative.** Every action passes through the policy engine.
  Hooks are programmable middleware but cannot bypass policy.
- **Offline is the default.** Full capabilities without network access.
  Cloud is an optional enhancement, not a requirement.
- **One file = one agent.** Back up, move, inspect, or embed by copying a
  single `.db` file.

## Encryption

SQLCipher provides encryption at rest. Keys are stored in the OS keychain
(macOS Keychain, Linux keyring, Windows Credential Manager) — never as
plaintext on disk. Sync traffic runs over TLS.
