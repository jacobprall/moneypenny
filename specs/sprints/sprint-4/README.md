# Sprint 4 — Evaluation Harness

> The sprint that lets you prove your agents work. A portable eval framework
> with SWE-bench import, multi-runner comparison, multi-session sequences,
> statistical analysis, Docker isolation, parallel execution, and CI
> regression gating.
>
> Ported from `moneypenny-rs/crates/mp-eval`, adapted for the TypeScript
> ecosystem with Bun as the runtime.

**Prerequisites:** Sprint 1 complete (AgentBridge, job system, blueprints).
Sprint 2 beneficial (parallel tools, embeddings). Sprints 3 is independent.

---

## Existing foundations

| Component | Location | Status |
|-----------|----------|--------|
| `AgentBridge` | `@moneypenny/bridge` (sprint 1) | Production. `mp-agent` runner wraps it. |
| Agent loop + tool execution | `@moneypenny/loop` | Production. |
| Cost tracking | `@moneypenny/loop/cost.ts` | Production. `calculateCost` function. |
| Nothing in `@moneypenny/eval` | — | Sprint 4 creates this package from scratch. |

---

## Overview

| # | Workstream | Packages touched |
|---|-----------|-----------------|
| 1 | Task format and loading | `@moneypenny/eval` |
| 2 | Runner interface and built-in runners | `@moneypenny/eval` |
| 3 | Harness (single-task and multi-session) | `@moneypenny/eval` |
| 4 | Verification system | `@moneypenny/eval` |
| 5 | Results database | `@moneypenny/eval` |
| 6 | SWE-bench importer | `@moneypenny/eval` |
| 7 | Statistical analysis | `@moneypenny/eval` |
| 8 | Reporting | `@moneypenny/eval` |
| 9 | Docker isolation | `@moneypenny/eval` |
| 10 | Parallel execution | `@moneypenny/eval` |
| 11 | CI regression gate | `@moneypenny/eval` |
| 12 | Sample task suite | `eval/tasks/moneypenny/` |
| 13 | CLI integration | `@moneypenny/cli` |

---

## Package Structure

```
packages/eval/
├── package.json
├── src/
│   ├── index.ts                 Entry point
│   ├── task.ts                  Task, TaskSequence, loadTasks
│   ├── exec.ts                  execWithTimeout (shared utility)
│   ├── runner/
│   │   ├── index.ts             Runner interface, getRunner, listRunners
│   │   ├── mp-agent.ts          MpAgentRunner
│   │   ├── claude-code.ts       ClaudeCodeRunner
│   │   ├── cursor.ts            CursorRunner
│   │   ├── aider.ts             AiderRunner
│   │   ├── codex.ts             CodexRunner
│   │   ├── shell.ts             ShellRunner
│   │   ├── http.ts              HttpRunner
│   │   └── context.ts           ContextRunner
│   ├── harness.ts               runEval, runMultiSession
│   ├── worktree.ts              prepareWorkdir, cleanupWorkdir
│   ├── environment.ts           setupEnvironment
│   ├── verify.ts                VerifySpec, runVerify, patch-then-test
│   ├── db.ts                    ResultsDB
│   ├── swe-bench.ts             SWE-bench importer
│   ├── stats.ts                 Wilson CI, McNemar, efficiency
│   ├── report.ts                All output formats
│   ├── docker.ts                DockerConfig, Dockerfile generation
│   └── parallel.ts              runParallel
├── eval/
│   ├── tasks/moneypenny/        Sample tasks (8 tasks, easy/medium/hard)
│   └── baseline.sqlite          Baseline results (committed after first run)
└── tests/
    ├── stats.test.ts
    ├── verify.test.ts
    ├── task.test.ts
    ├── exec.test.ts
    └── swe-bench.test.ts
```

---

## Implementation Order

```
Phase 1: Foundation
  ├── §1 Task format + loading
  ├── §2.1 Runner interface + execWithTimeout
  ├── §4 Verification system (including patch-then-test)
  └── §5 Results database

Phase 2: Core harness
  ├── §2.2 MpAgentRunner (most important runner)
  ├── §3 Harness (single-task + multi-session + worktrees)
  └── §10 Parallel execution

Phase 3: Runners [parallelizable]
  ├── §2.3 Claude Code runner
  ├── §2.4 Shell + HTTP runners
  ├── §2.5 Cursor + Aider + Codex runners
  └── §2.6 Context runner (IR)

Phase 4: Analysis + reporting [parallelizable with Phase 3]
  ├── §7 Statistical analysis
  └── §8 Reporting (all formats)

Phase 5: SWE-bench + isolation
  ├── §6 SWE-bench importer (with env setup + patch-then-test)
  └── §9 Docker isolation (with per-repo images)

Phase 6: Polish
  ├── §11 CI regression gate + baseline lifecycle
  ├── §12 Sample task suite
  └── §13 CLI integration
```

Phases 1–2 are sequential. Phases 3–4 parallelize. Phase 5 builds on
phases 1–2. Phase 6 ties everything together.

---

## What we deliberately skip

- **GPU-accelerated eval** — out of scope for an agent eval harness.
- **Distributed execution** (multi-machine parallelism) — single-machine
  `runParallel` is sufficient for solo dev use.
- **Live eval dashboard** (streaming trial results to web UI) — results
  are available after completion via reports.
- **Custom scoring functions** (beyond pass/fail) — binary `VerifySpec` is
  sufficient for the first release. Numeric scoring (code quality metrics)
  could be added as a `score` verify type.
- **LLM-as-judge verification** — using an LLM to grade agent output is
  useful but introduces cost and non-determinism. Deferred.
