# Sprint 2 — Intelligence Infrastructure

> The sprint that makes the database smart. Parallel tool execution,
> a unified query engine, computed health views, a full session lifecycle
> with compaction/archival, embedding pipeline, and an autonomous
> custodian agent.

**Prerequisites:** Sprint 1 complete (schema v10, job system, blueprints,
`context_curate` tool)

---

## Existing foundations (already implemented)

| Component | Location | Status |
|-----------|----------|--------|
| `DbWriter` (exclusive + defer) | `@moneypenny/db/writer.ts` | Production. 107 lines. |
| `DbReadPool` (round-robin readers) | `@moneypenny/db/read-pool.ts` | Production. 70 lines. |
| `withBusyRetry` (cross-process) | `@moneypenny/db/busy-retry.ts` | Production. |
| Scheduler uses `agent.reads.read()` / `agent.writer.exclusive()` | `@moneypenny/agents/scheduler.ts` | Production. |
| BM25 + vector hybrid search (vector leg is extension-gated) | `@moneypenny/search/search.ts` | Production. RRF fusion. |
| Embeddings inserted as NULL by indexer | `@moneypenny/search/indexer.ts` | **Gap.** Vector leg dead without embeddings. |

**Key insight:** Read/write separation is built. What's missing is:
(a) wiring parallel tool execution in the loop, (b) actually populating
embeddings, and (c) building the higher-level intelligence features on
top of the existing infrastructure.

---

## Overview

| # | Workstream | Packages touched | Spec |
|---|-----------|-----------------|------|
| 1 | Embedding pipeline | `@moneypenny/search`, `@moneypenny/db` | [embedding-pipeline.md](./embedding-pipeline.md) |
| 2 | Parallel tool execution | `@moneypenny/loop` | [parallel-tools.md](./parallel-tools.md) |
| 3 | Context pipeline (composable retrieval + assembly) | `@moneypenny/ctx`, `@moneypenny/search` | [context-pipeline.md](./context-pipeline.md) |
| 4 | Computed intelligence views | `@moneypenny/db` | [computed-views.md](./computed-views.md) |
| 5 | Session lifecycle (compact → embed → archive → purge) | `@moneypenny/loop`, `@moneypenny/ctx`, `@moneypenny/db` | [session-lifecycle.md](./session-lifecycle.md) |
| 6 | Custodian agent | `@moneypenny/agents`, built-in blueprint | [custodian-agent.md](./custodian-agent.md) |
| — | Schema additions (migration v11) | `@moneypenny/db` | [schema-v11.md](./schema-v11.md) |

---

## Implementation order

```
Phase 1: Embedding pipeline (§1)
  │       ↑ unblocks vector search for context pipeline AND summary embedding
  │
  ├── Phase 2: Parallel tool execution (§2) [independent]
  │
  ├── Phase 3: Context pipeline (§3) [depends on §1 for vector leg]
  │
  ├── Phase 4: Computed views (§4) [independent]
  │
  └── Phase 5: Session lifecycle (§5) [depends on §1 for summary embedding]
      │   Stage 1 (compact) is independent
      │   Stage 2 (embed) depends on §1
      │   Stage 3 (archive/purge) depends on stage 1+2
      │
      └── Phase 6: Custodian (§6) [depends on §5]
```

Phases 2, 4, and §5 stage 1 (compaction without embedding) can start
immediately. Phase 3 benefits from §1 but works without it (BM25
fallback). §5 stages 2–3 and §6 are the capstone.

---

## What we deliberately skip

- **Reactive triggers (SQLite write hooks → event bus)** — deferred to sprint 3
- **Self-evolving prompts** — deferred to sprint 3
- **Embeddable SQL extensions** — deferred to sprint 3
- **Cross-workspace federation** — out of scope
- **Cross-session deduplication** (merging similar summaries) — out of scope
