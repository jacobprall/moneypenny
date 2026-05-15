# Sprint 1 — Core Platform

> The sprint that turns moneypenny from a CLI coding agent into a platform.
> Web UI, file watcher, research strategy, richer blueprints, generic job
> system, self-reflection tool, hook consolidation, and the unified event
> protocol that ties everything together.

---

## Existing foundations (already implemented)

Before listing workstreams, the following are **already built** and should
not be rebuilt. Sprint 1 extends them:

| Component | Location | Status |
|-----------|----------|--------|
| `DbWriter` (exclusive + defer) | `@moneypenny/db/writer.ts` | Production. Sprint 2 wires parallel tools. |
| `DbReadPool` (round-robin readers) | `@moneypenny/db/read-pool.ts` | Production. Used by scheduler. |
| Jobs + job_runs tables, CRUD | `@moneypenny/agents/jobs-repo.ts` | Production. Sprint 1 §6 extends, doesn't replace. |
| Scheduler (cron tick, run tracking) | `@moneypenny/agents/scheduler.ts` | Production. Sprint 1 §6 adds job types. |
| Blueprint watcher (chokidar on `.mp/agents/`) | `@moneypenny/agents/loader.ts` | Production. Sprint 1 §3 extends scope. |
| In-process `HookPipeline` | `@moneypenny/ctx/builtin/pipeline.ts` | Production. Sprint 1 §9 consolidates DB hooks into it. |
| SSE event streaming | `@moneypenny/http/routes/events.ts` | Production. Sprint 1 §2 adds WebSocket. |

---

## Overview

| # | Workstream | Packages touched |
|---|-----------|-----------------|
| 1 | AgentBridge event protocol | `@moneypenny/loop`, new `@moneypenny/bridge` |
| 2 | Web UI from `mp serve` | new `apps/web`, `@moneypenny/http` |
| 3 | File watcher (extended) | new `@moneypenny/watch`, extends `@moneypenny/agents/loader` |
| 4 | Research iteration strategy | `@moneypenny/loop` |
| 5 | Richer blueprint system | `@moneypenny/agents`, `@moneypenny/db` |
| 6 | Generic job system | `@moneypenny/agents`, `@moneypenny/db`, `@moneypenny/http` |
| 7 | `context_curate` tool | `@moneypenny/tools` |
| 8 | Hook system consolidation | `@moneypenny/ctx`, `@moneypenny/db` |
| 9 | Schema additions | `@moneypenny/db` |
| 10 | Graceful shutdown | `@moneypenny/http`, all runtime packages |

---

## Implementation order

```
Phase 0: Schema migration §9 (version 10)
  │
  ├── Phase 1: AgentBridge §1 ─────────────────┐
  │                                              │
  ├── Phase 3: File watcher §3 [independent]     │
  │                                              │
  ├── Phase 4: Research strategy §4 [independent] │
  │                                              │
  ├── Phase 5: Blueprints §5 [depends on §4]     │
  │                                              │
  ├── Phase 6: Job system §6 [independent]       │
  │                                              │
  ├── Phase 7: context_curate §7 [independent]   │
  │                                              │
  ├── Phase 8: Hook consolidation §8 [independent]│
  │                                              │
  └── Phase 10: Graceful shutdown §10 [independent]
                                                 │
  Phase 2: Web UI §2 ──────────────────────────┘
    (depends on §1 for chat streaming)
    (depends on §6 for Jobs page)
```

---

## What we deliberately skip

- **EvolutionStrategy** — deferred (complex scoring mechanism design needed)
- **TUI** — web UI is the management surface
- **Telegram / webhook channels** — deferred to sprint 3
- **Prompt evolution** — deferred to sprint 3
- **Eval harness** — dedicated sprint 4
