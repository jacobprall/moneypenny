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

## 1. AgentBridge Event Protocol

### Problem

The CLI iterates `AsyncGenerator<LoopEvent>` directly. The HTTP API wraps
it in SSE. There is no shared contract for what events a frontend receives,
which means adding governance visibility, strategy progress, or cost
updates requires changes in multiple places.

### Design

Define a canonical `AgentEvent` union type that both CLI and web UI consume.
The bridge translates internal `LoopEvent`s into `AgentEvent`s and adds
governance and cost information that `LoopEvent` does not carry today.

```typescript
// @moneypenny/bridge

export type AgentEvent =
  | { type: "stream_token"; text: string }
  | { type: "tool_call_start"; id: string; name: string; args: unknown }
  | { type: "tool_call_result"; id: string; result: string; success: boolean; durationMs: number }
  | { type: "governance_decision"; toolCallId: string; effect: PolicyEffect; policyName?: string; reason: string }
  | { type: "strategy_progress"; update: StrategyUpdate }
  | { type: "cost_update"; sessionCostUsd: number; turnCostUsd: number }
  | { type: "turn_complete"; usage: TokenUsage; costUsd: number }
  | { type: "error"; code: LoopErrorCode | "bridge_error"; message: string; retryable: boolean }
  | { type: "session_loaded"; sessionId: string; messageCount: number };
```

### AgentBridge class

```typescript
export class AgentBridge {
  private loop: AgentLoop;
  private db: AgentDB;
  private abortController: AbortController | null = null;

  async *run(message: string, options: RunOptions): AsyncGenerator<AgentEvent> {
    this.abortController = new AbortController();
    try {
      yield { type: "session_loaded", sessionId: options.sessionId, messageCount: /* ... */ };

      for await (const event of this.loop.run(this.db, message)) {
        // Translate LoopEvent → AgentEvent(s)
        // On LLM rate limit: yield error with retryable: true, back off, retry
        // On tool crash: yield tool_call_result with success: false, continue loop
        // On abort signal: break cleanly
      }
    } catch (e) {
      yield {
        type: "error",
        code: e instanceof LoopError ? e.code : "bridge_error",
        message: e instanceof Error ? e.message : String(e),
        retryable: e instanceof LoopError && e.code === "rate_limited",
      };
    }
  }

  abort(): void {
    this.abortController?.abort();
  }
}
```

### Error handling and resilience

| Error type | Bridge behavior |
|-----------|----------------|
| LLM rate limit (429) | Yield `error` with `retryable: true`, exponential backoff (1s, 2s, 4s), retry up to 3 times |
| LLM server error (500/503) | Yield `error` with `retryable: true`, retry once after 2s |
| Tool execution crash | Yield `tool_call_result` with `success: false`, let LLM decide next step |
| Cost limit exceeded | Yield `error` with `retryable: false`, code `"cost_limit"` |
| WebSocket disconnect | Client reconnects within 5s, bridge resumes from last `turn_complete` |
| Abort signal | Break loop cleanly, flush deferred writes, yield final cost_update |

### DataStore query interface

`DataStore` is a **facade** over `AgentDB` — it does not replace `AgentDB`
but provides the view-model queries that UI consumers need. `AgentDB`
remains the low-level persistence layer.

```typescript
export class DataStore {
  constructor(private db: AgentDB) {}

  listSessions(opts: { limit?: number; offset?: number; search?: string }): SessionRow[];
  getSession(id: string): SessionRow | null;
  deleteSession(id: string): void;
  exportSession(id: string, format: "markdown" | "json"): string;

  listBlueprints(): BlueprintRow[];
  getBlueprint(name: string): BlueprintDetail | null;

  listJobs(opts?: { type?: JobType }): JobRow[];
  listJobRuns(jobId: string, limit?: number): JobRunRow[];

  listMemories(opts?: { limit?: number; search?: string }): MemoryRow[];
  listSkills(): SkillRow[];
  indexHealth(): IndexHealthStats;

  listPolicyEvents(sessionId: string): GovEventRow[];
  listActivePolicies(): PolicyRow[];

  costSummary(): CostSummary;
}
```

### View model types

(Unchanged from prior spec — `SessionRow`, `BlueprintRow`, `BlueprintDetail`,
`JobRow`, `JobRunRow`, `IndexHealthStats`, `CostSummary`.)

### Acceptance criteria

- [ ] CLI `mp chat` works through `AgentBridge` with identical UX to today
- [ ] HTTP SSE endpoint streams `AgentEvent` JSON lines
- [ ] LLM rate limit triggers retry with backoff (visible in event stream)
- [ ] Tool crash yields `tool_call_result.success = false`, loop continues
- [ ] `DataStore` queries return correct data for all view model types
- [ ] `abort()` stops a running session within 1s

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 1.1 | `AgentEvent` types + `AgentBridge` wrapping existing loop | 2 days |
| 1.2 | Error handling: retry, backoff, abort, resilience | 1 day |
| 1.3 | `DataStore` facade over `AgentDB` | 2 days |
| 1.4 | Wire CLI `mp chat` through `AgentBridge` | 1 day |
| 1.5 | Wire HTTP SSE through `AgentBridge` | 1 day |

---

## 2. Web UI

### Technology

| Layer | Choice | Rationale |
|-------|--------|-----------|
| Framework | **React 19** + **Vite** | Familiar ecosystem, wide component availability |
| Styling | **Tailwind CSS** (build-time) | Utility classes, tree-shaken |
| State | **Zustand** | Minimal, works with WebSocket streams |
| Charts | **uPlot** (~35 KB) | GPU-accelerated, handles cost/latency charts |
| Icons | **Lucide** (tree-shaken) | Consistent line icons |
| Fonts | System stack + JetBrains Mono for code/data | No external font loads |

### Bundle budget (revised)

| Asset | Target | Hard limit |
|-------|--------|------------|
| JS (gzip) | 180 KB | 300 KB |
| CSS (gzip) | 15 KB | 30 KB |
| Total | ~200 KB | 350 KB |

React 19 is ~45 KB gzipped. With Zustand (~2 KB), uPlot (~35 KB), router
(~8 KB), and application code, 180 KB is realistic. The hard limit of
300 KB still delivers sub-second load on 3G.

### WebSocket protocol

Single WebSocket at `/api/v1/ws` multiplexing chat and observe:

```
Client → Server:
  {"type":"message", "sessionId":"...", "blueprint":"...", "text":"..."}
  {"type":"abort"}
  {"type":"subscribe", "channels":["events","costs","latency"]}
  {"type":"unsubscribe", "channels":["events"]}
  {"type":"ping"}

Server → Client:
  AgentEvent (streamed during agent run)
  {"type":"event", "event":{...}}       (observe subscription)
  {"type":"pong"}
```

**Reconnection protocol:** Client reconnects with exponential backoff
(1s, 2s, 4s, max 30s). On reconnect, client sends
`{"type":"resume", "lastEventId":"..."}`. Server replays missed events
from the event log if available, or sends `session_loaded` to resync.

### Pages

(Unchanged: Chat, Sessions, Agents, Jobs, Observe, Tune, System.)

### Tune page: configuration model

Settings on the Tune page are scoped:

| Setting | Scope | Storage |
|---------|-------|---------|
| Temperature, top-p, max tokens | Per-blueprint | Blueprint frontmatter `model_params:` |
| History depth, chunk retrieval count | Per-blueprint | Blueprint frontmatter `context:` |
| Cost cap, warning threshold | Per-blueprint (overridable global) | Blueprint `guardrails:` / global `config` table |
| Max turns, parallel tools | Global | `config` table |
| Sub-agent depth limit | Global | `config` table |

Changes made on the Tune page write to the `config` table (global) or
trigger a blueprint reload (per-blueprint). Changes take effect on the
next agent run, not mid-session.

### Authentication

Localhost mode (default): random bearer token generated on `mp serve`
startup, printed to terminal, saved to `.mp/serve-token`. Browser stores in
`localStorage` after one-time paste. WebSocket authenticates via the
first message: `{"type":"auth", "token":"..."}`.

### Build integration

(Unchanged: `apps/web/` structure, `mp serve` serves `dist/`, `--dev` proxy.)

### Acceptance criteria

- [ ] `mp serve` opens web UI at `http://localhost:1745` with auth flow
- [ ] Chat page streams responses with tool call expansion
- [ ] Sessions page lists, searches, deletes, exports sessions
- [ ] Jobs page shows all job types with run history and trigger buttons
- [ ] Observe page shows live event stream and cost charts
- [ ] Tune page persists settings to config table / blueprint frontmatter
- [ ] Command palette (`Cmd+K`) navigates between pages
- [ ] Total JS bundle < 300 KB gzipped
- [ ] WebSocket reconnects automatically within 5s of disconnect

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 2.0 | App scaffold: Vite + React + Tailwind + router + Zustand + layout shell | 1 day |
| 2.1 | Chat page + WebSocket streaming + tool call display | 3 days |
| 2.2 | Sessions page (data table, search, delete, export) | 2 days |
| 2.3 | Agents page (blueprint catalog, capability tree) | 2 days |
| 2.4 | Jobs page (all job types, run history, trigger/toggle) | 2 days |
| 2.5 | Observe page (event stream, cost tracker, token usage) | 2 days |
| 2.6 | Tune page (model params, context, cost controls, loop config) | 1.5 days |
| 2.7 | System page (config editor, policy viewer, index health, skills) | 1.5 days |
| 2.8 | Command palette, keyboard shortcuts, auth flow | 1 day |
| 2.9 | Static asset serving from `@moneypenny/http`, `--dev` proxy mode | 1 day |

---

## 3. File Watcher (Extended)

### What exists today

`@moneypenny/agents/loader.ts` exports `startWatcher()` which watches
`.mp/agents/` for blueprint changes via chokidar. This sprint extends
the scope to source files, policies, skills, and gitignore changes.

### Design

New package `@moneypenny/watch` that coordinates multiple watchers:

```typescript
export interface WatcherConfig {
  repoPath: string;
  db: AgentDB;
  debounceMs?: number;         // default 300
  excludePatterns?: string[];  // merged with .gitignore + .mpignore
}

export function startWatcher(config: WatcherConfig): WatcherHandle;

export interface WatcherHandle {
  stop(): void;
  stats(): WatcherStats;
}
```

### Watcher backend

Use Bun's built-in `fs.watch` (recursive mode) with a debounce layer.
The existing chokidar-based blueprint watcher in `loader.ts` is replaced
by a handler registered with the new unified watcher.

**Debounce strategy:** Per-file 300ms debounce window. Batch operations
(git checkout touching 50 files) are detected by counting events within
a 100ms burst window; if > 10 files change within 100ms, batch them into
a single re-index call rather than 50 individual operations.

### Event routing

| Path pattern | Handler | Action |
|-------------|---------|--------|
| Source files (configured extensions) | `@moneypenny/search` indexer | Incremental re-chunk + re-embed via `reindexFile()` |
| `.mp/policies/*.yaml` | Policy sync | Re-parse, sync to `policies` table |
| `.mp/agents/*.md` | Blueprint loader | Re-parse frontmatter, upsert `agents` table |
| `.mp/skills/**/*.md` | Skill scanner | Re-scan via `scanSkillDirs()` |
| `.mp/jobs/*.yaml` | Job loader | Re-parse, sync to `jobs` table |
| `.gitignore`, `.mpignore` | Watcher itself | Re-compute exclude patterns, re-filter watch list |
| File deletions | Indexer | Mark chunks as stale (don't delete — gardener handles) |

### Acceptance criteria

- [ ] `mp serve` starts watcher automatically; `--no-watch` disables
- [ ] Source file save triggers re-index within 1s (after debounce)
- [ ] Git checkout (50+ files) batches into single re-index operation
- [ ] Blueprint change triggers agent table upsert within 500ms
- [ ] Policy YAML change takes effect on next tool call (no restart)
- [ ] `GET /api/v1/observe/watcher` returns `WatcherStats`
- [ ] Watcher ignores files matching `.gitignore` + `.mpignore`

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 3.1 | Watcher core: `fs.watch` + debounce + batch detection + exclude filtering | 1.5 days |
| 3.2 | Source file handler: incremental re-index via `reindexFile()` | 1 day |
| 3.3 | Policy/agent/skill/job handlers: re-parse and sync | 1 day |
| 3.4 | Wire into `mp serve`, stats endpoint, `--no-watch` flag | 0.5 days |

---

## 4. Research Iteration Strategy

### Problem

The TS loop has a single strategy: call LLM, execute tools, repeat until
text response or max iterations. Blueprint authors should be able to
declare `strategy: research` and get multi-iteration autonomous
information gathering.

**Note:** EvolutionStrategy (self-improving loop with scoring) is deferred
to a future sprint. It adds significant complexity (scoring function design,
history clearing, LLM-as-judge calls) that isn't needed for the initial
platform release.

### Design

```typescript
export interface IterationStrategy {
  preIteration(iteration: number, history: Message[]): StrategyAction;
  postIteration(iteration: number, response: string | null, history: Message[]): StrategyAction;
  finalize(): StrategyOutput | null;
}

export type StrategyAction =
  | { action: "continue" }
  | { action: "done" }
  | { action: "inject_user_message"; message: string };
```

### StandardStrategy

The default. Preserves current behavior: `postIteration` returns `done`
when the LLM produces a text response without tool calls.

### ResearchStrategy

Multi-iteration information gathering:

1. `preIteration(0)`: injects research kickoff prompt establishing the
   agent as a researcher with structured output expectations
2. `postIteration(n)`: parses `FINDING:` / `SOURCE:` markers from the
   response. Returns `done` if `RESEARCH_COMPLETE` marker found or if
   no new findings for 2 consecutive iterations (staleness detection).
3. `preIteration(n)` for n>0: injects progress prompt listing findings so
   far, gaps identified, and instruction to search for more.
4. `finalize()`: if max iterations hit without synthesis, returns the
   findings collected so far as structured output.

Config in blueprint:
```yaml
strategy: research
research:
  max_iterations: 5
```

### Loop integration

The strategy hooks into the existing `runAfterUserMessage` generator:

```typescript
// Before each LLM call:
const action = strategy.preIteration(iteration, history);
if (action.action === "done") break;
if (action.action === "inject_user_message") {
  // append to history, continue
}

// After LLM response:
const postAction = strategy.postIteration(iteration, responseText, history);
// route accordingly
```

### Strategy progress events

Emitted as `LoopEvent`, translated by bridge to `AgentEvent.strategy_progress`:

```typescript
{ strategy: "research"; iteration: number; maxIterations: number; findingsCount: number; status: string }
```

### Acceptance criteria

- [ ] `strategy: standard` in blueprint preserves current behavior exactly
- [ ] `strategy: research` runs multiple iterations, collects findings
- [ ] Research stops early if `RESEARCH_COMPLETE` marker found
- [ ] Research stops on staleness (2 iterations with no new findings)
- [ ] Strategy progress events stream through bridge to UI
- [ ] Max iterations cap is respected

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 4.1 | `IterationStrategy` interface + `StandardStrategy` + loop refactor | 1.5 days |
| 4.2 | `ResearchStrategy` with finding extraction, gap analysis, staleness detection | 2 days |
| 4.3 | Strategy progress events through the bridge | 0.5 days |

---

## 5. Richer Blueprint System

### New frontmatter fields

```yaml
---
name: research-assistant
description: Autonomous research agent with fact-checking
model: claude-sonnet-4-6
tools:
  - web_fetch
  - code_search
  - memory_search
  - memory_add
deny_paths:
  - ".env*"
  - "credentials.*"
max_turns: 50

# NEW: Sub-agent declarations
sub_agents:
  - name: fact-checker
    blueprint: ./fact-checker.md
    model: claude-3-5-haiku-20241022
    history: fresh              # fresh | persistent
    memory: read_only           # shared | isolated | read_only
  - name: summarizer
    blueprint: ./summarizer.md
    model: gemini-2.5-flash

# NEW: Iteration strategy
strategy: research
research:
  max_iterations: 5

# NEW: Memory configuration
memory:
  context: "research"
  inject: true
  extract: true

# NEW: Guardrail overrides (per-blueprint)
guardrails:
  max_cost_usd: 0.50
  max_iterations: 15
  filesystem_sandbox:
    - "./src"
    - "./docs"

# Existing schedule, enhanced
schedule:
  cron: "0 */6 * * *"
  trigger: cron
  input_template: "Review and summarize recent activity in the codebase"
  enabled: true
---
```

### Sub-agent execution

Each sub-agent in `sub_agents` is registered as a tool via the existing
`delegate` tool infrastructure. When the parent LLM calls the sub-agent
tool, the executor:

1. Loads the sub-agent blueprint
2. Creates a provider with the sub-agent's model (or parent's if not set)
3. Creates fresh or persistent history based on `history` mode
4. Configures memory sharing based on `memory` mode
5. Runs `createAgentLoop` + `loop.run()` with the sub-agent config
6. Returns the response as the tool result

Nesting is limited to 3 levels. Each level inherits the parent's cost
budget minus what has been consumed.

### Acceptance criteria

- [ ] New frontmatter fields parse without breaking existing blueprints
- [ ] Sub-agent tool calls work through the delegate executor
- [ ] Sub-agent nesting respects 3-level depth limit
- [ ] Cost budget propagates correctly to sub-agents
- [ ] `guardrails.filesystem_sandbox` restricts tool access
- [ ] `memory.inject` enriches system prompt from stored knowledge

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 5.1 | Parse new frontmatter fields (extend Zod schema in `agents/schema.ts`) | 1 day |
| 5.2 | Sub-agent tool registration and execution via `delegate` | 2 days |
| 5.3 | Memory config: inject (system prompt enrichment) and extract (post-session) | 1.5 days |
| 5.4 | Guardrail override wiring (cost guard, max iterations, sandbox) | 1 day |

---

## 6. Generic Job System

### What exists today

The `jobs` and `job_runs` tables exist. `jobs-repo.ts` provides CRUD.
The scheduler ticks on an interval, finds due jobs, and executes them —
but only supports `AGENT_RUN_OPERATION`. This sprint extends the job
system to support arbitrary job types while preserving the existing
schema and scheduler behavior.

### Extended design

```typescript
// Extend the existing Job interface, don't replace it
export type JobOperation =
  | "agents.run"                    // existing
  | "pipeline.run"                  // new: data pipeline
  | "index.run"                     // new: re-index
  | "sync.run"                      // new: cloud sync
  | "custom.run";                   // new: dynamic handler

// The existing `payload` JSON field carries operation-specific config:
export type JobPayload =
  | { agent_id: string }                               // agents.run (existing)
  | { steps: PipelineStep[] }                           // pipeline.run
  | { scope: "full" | "incremental" | "stale" }        // index.run
  | { target: "cloud" | "team" }                       // sync.run
  | { handler: string; params: Record<string, unknown> }; // custom.run
```

### Pipeline steps

```typescript
export interface PipelineStep {
  name: string;
  action: PipelineAction;
}

export type PipelineAction =
  | { type: "http_fetch"; url: string; method?: string; headers?: Record<string, string> }
  | { type: "transform"; script: string }
  | { type: "index_content"; table: string }
  | { type: "shell"; command: string; timeout?: number }
  | { type: "agent_run"; blueprint: string; input: string };
```

### Pipeline security model

Pipeline steps execute in a restricted context:

| Step type | Restrictions |
|-----------|-------------|
| `http_fetch` | No localhost/private IP access. Configurable URL allowlist in `.mp/config.yaml`. |
| `transform` | Runs in a `new Function()` sandbox with no `require`/`import`, no `process`, no `fs`. Input is the prior step's output string. Return value is the next step's input. |
| `shell` | Governed by the same `filesystem_sandbox` as agent tools. Timeout defaults to 30s. |
| `agent_run` | Subject to blueprint guardrails. |
| `index_content` | Can only write to allowlisted tables (`skills`, `knowledge`, `docs`). |

Pipeline jobs created via the HTTP API require the serve token (same auth
as all API endpoints). Jobs defined in `.mp/jobs/*.yaml` are trusted
(user-authored, like policies).

### Scheduler extension

The existing `startScheduler` is refactored to use a `JobExecutor` registry:

```typescript
// In scheduler.ts, the if/else on job.operation becomes:
const executor = executors.get(job.operation);
if (!executor) {
  throw new Error(`Unsupported job operation: ${job.operation}`);
}
await executor.execute(job, run);
```

Built-in executors: `AgentRunExecutor` (migrated from existing code),
`PipelineExecutor`, `IndexExecutor`, `SyncExecutor`, `CustomExecutor`.

### Job file loading

On startup (`mp serve`), the scheduler scans `.mp/jobs/*.yaml` and syncs
definitions to the `jobs` table (insert if new, update if changed, disable
if file deleted). Agent blueprints with `schedule:` blocks continue to be
registered as `agents.run` jobs via the existing loader.

### Acceptance criteria

- [ ] Existing `agents.run` jobs continue to work identically
- [ ] `pipeline.run` job executes steps sequentially, passes data between steps
- [ ] `http_fetch` step blocks localhost/private IPs
- [ ] `transform` step cannot access filesystem or network
- [ ] `index.run` job re-indexes codebase (full/incremental/stale)
- [ ] Jobs page in web UI shows all job types with run history
- [ ] `.mp/jobs/*.yaml` files are synced to DB on startup
- [ ] `POST /api/v1/jobs/:id/trigger` triggers any job type

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 6.1 | `JobExecutor` interface, refactor scheduler to use registry | 1 day |
| 6.2 | `PipelineExecutor` with security restrictions per step type | 2 days |
| 6.3 | `IndexExecutor` + `SyncExecutor` + `CustomExecutor` | 1 day |
| 6.4 | Job file loading from `.mp/jobs/*.yaml` + sync on startup | 1 day |
| 6.5 | HTTP API endpoints for job management | 1 day |

---

## 7. `context_curate` Tool

### Design

A governed tool that provides semantic operations over the intelligence
file. The agent can inspect its own state, search across knowledge
surfaces, and perform maintenance operations.

```typescript
const contextCurateTool = defineTool({
  name: "context_curate",
  description: "Query and manage your own intelligence file.",
  parameters: z.object({
    action: z.enum([
      "search_memory",
      "forget_memory",
      "review_costs",
      "list_skills",
      "update_skill",
      "list_sessions",
      "summarize_session",
      "index_status",
      "inspect_policies",
      "prune_stale_chunks",
    ]),
    params: z.record(z.unknown()).optional(),
  }),
});
```

### Governance

Destructive actions (`forget_memory`, `update_skill`, `summarize_session`,
`prune_stale_chunks`) are gated by policy. Default policy scaffolded by
`mp init`:

```yaml
- name: curation-guard
  effect: confirm
  tool_pattern: "context_curate"
  args_pattern: '{"action":"forget_memory|update_skill"}'
  message: "This action modifies your knowledge base. Proceed?"
  priority: 100
```

Note: `prune_stale_chunks` and `summarize_session` use `effect: allow`
by default since they're non-destructive (pruning removes chunks for
deleted files; summarizing doesn't delete messages).

### Acceptance criteria

- [ ] Read-only actions (`search_memory`, `review_costs`, `list_*`, `index_status`, `inspect_policies`) work without governance gates
- [ ] Destructive actions trigger policy evaluation
- [ ] `search_memory` returns results from messages, skills, and knowledge
- [ ] `review_costs` returns per-agent and aggregate cost data
- [ ] `prune_stale_chunks` removes chunks for files no longer on disk

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 7.1 | Tool definition + read-only actions | 1.5 days |
| 7.2 | Destructive actions | 1.5 days |
| 7.3 | Default curation policy | 0.5 days |

---

## 8. Hook System Consolidation

### Problem

Two hook systems exist today:

1. **In-process `HookPipeline`** (`@moneypenny/ctx/builtin/pipeline.ts`):
   Used by the agent loop. Runs `dbPolicyHook`, `credentialRedactor`,
   `costGuard`, `confirmationGate`. This is the production code path.

2. **DB `hooks` table + `operations.execute`** (`@moneypenny/ctx/hooks.ts`,
   `operations.ts`): Stores hook scripts as `Function()` constructor
   strings in the database. Implemented but **not wired** to the main
   agent loop. Dead code on the primary execution path.

### Design: merge with declarative conditions

Replace the `Function()`-based DB hooks with **declarative condition
rules** that load into the same `HookPipeline` at startup. This gives:

- **Single execution path** at runtime (no two-system confusion)
- **Portability** (hooks defined in DB are shareable via cloud sync)
- **No arbitrary code execution** (conditions are data, not eval'd strings)

```typescript
// Declarative hook definition (stored in hooks table)
export interface DeclarativeHook {
  id: string;
  name: string;
  phase: "pre_tool" | "post_tool" | "pre_llm" | "post_llm";
  priority: number;
  condition: HookCondition;
  action: HookAction;
  enabled: boolean;
}

export type HookCondition =
  | { type: "tool_name"; pattern: string }      // glob match
  | { type: "args_match"; jsonpath: string; value: string }
  | { type: "cost_exceeds"; usd: number }
  | { type: "session_turns_exceed"; count: number }
  | { type: "always" };

export type HookAction =
  | { type: "deny"; message: string }
  | { type: "audit"; message: string }
  | { type: "confirm"; message: string }
  | { type: "transform_args"; jsonpath: string; value: string }
  | { type: "inject_context"; content: string };
```

At startup, `createHookPipeline` loads declarative hooks from the DB and
merges them with code-defined hooks (policies, credential redactor, cost
guard), sorted by priority.

### Migration

The existing `hooks` table schema is altered to store `condition` and
`action` JSON columns instead of `script`. Any existing `Function()`-based
hooks are logged as warnings and skipped.

### Acceptance criteria

- [ ] Existing code-defined hooks (`costGuard`, `credentialRedactor`, `dbPolicyHook`) work unchanged
- [ ] Declarative hooks from DB are loaded and execute in priority order
- [ ] `Function()` constructor is no longer used anywhere in the hook system
- [ ] Hooks can be created/updated via the API (`POST /api/v1/hooks`)
- [ ] Hook execution is visible in governance events

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 8.1 | `DeclarativeHook` types, condition evaluator, action executor | 1.5 days |
| 8.2 | Load declarative hooks into `HookPipeline` at startup | 1 day |
| 8.3 | Migrate `hooks` table schema, remove `Function()` usage | 0.5 days |
| 8.4 | HTTP API for hook CRUD | 0.5 days |

---

## 9. Schema Additions

### Migration strategy

The existing migration system uses `SCHEMA_VERSION` (currently 9) with a
`MIGRATIONS` array in `schema.ts`. Sprint 1 adds migration **version 10**:

```typescript
MIGRATIONS.push({
  version: 10,
  up: (db) => {
    // Sub-agent invocation log
    db.exec(`CREATE TABLE IF NOT EXISTS subagent_runs (...)`);

    // Compaction markers (if not already present from schema.sql)
    db.exec(`CREATE TABLE IF NOT EXISTS compaction_markers (...)`);

    // Agents table additions
    db.exec(`ALTER TABLE agents ADD COLUMN strategy TEXT DEFAULT 'standard'`);
    db.exec(`ALTER TABLE agents ADD COLUMN memory_config TEXT`);
    db.exec(`ALTER TABLE agents ADD COLUMN guardrails TEXT`);
    db.exec(`ALTER TABLE agents ADD COLUMN sub_agents TEXT`);

    // Hooks table migration (declarative)
    db.exec(`ALTER TABLE hooks ADD COLUMN condition TEXT`);
    db.exec(`ALTER TABLE hooks ADD COLUMN action TEXT`);

    // Jobs table additions for generic job types
    db.exec(`ALTER TABLE jobs ADD COLUMN type TEXT DEFAULT 'agents.run'`);
  },
});
```

`SCHEMA_VERSION` bumps to 10. The monolithic `SCHEMA_SQL` is also updated
to include these columns for fresh installs. `validateSchemaConsistency()`
ensures they stay in sync.

### Backward compatibility

All new columns have defaults. Existing databases open and migrate
transparently. No data loss. The `type` column on `jobs` defaults to
`'agents.run'` so existing jobs continue to work.

---

## 10. Graceful Shutdown

### Problem

`mp serve` runs HTTP server, WebSocket connections, file watcher, scheduler,
event bus (sprint 3), and potentially active agent sessions. An ungraceful
kill can lose deferred writes, leave stale WebSocket connections, or
corrupt in-progress job runs.

### Design

```typescript
export interface ShutdownManager {
  register(name: string, handler: () => Promise<void>, priority: number): void;
  shutdown(reason: string): Promise<void>;
}
```

Shutdown order (by priority, highest first):

| Priority | Component | Action |
|----------|-----------|--------|
| 100 | Active agent sessions | Signal abort, wait up to 5s for current LLM call to complete |
| 90 | WebSocket connections | Send close frame with "going away" (1001), wait 1s |
| 80 | HTTP server | Stop accepting new connections, drain in-flight requests (5s) |
| 70 | File watcher | Stop watching |
| 60 | Scheduler | Cancel next tick timer |
| 50 | Channel adapters (sprint 3) | Stop polling/listening |
| 40 | Event bus (sprint 3) | Drain pending async handlers (5s timeout) |
| 30 | DbWriter | Flush deferred write queue |
| 20 | DbReadPool | Close read connections |
| 10 | Write connection | Close |

`mp serve` registers a `SIGTERM` and `SIGINT` handler that calls
`shutdownManager.shutdown()`. Total shutdown budget: 15 seconds. If
any component exceeds its timeout, it's force-killed and logged.

### Acceptance criteria

- [ ] `Ctrl+C` on `mp serve` flushes all deferred writes before exit
- [ ] Active WebSocket clients receive close frame before disconnect
- [ ] In-progress agent sessions complete current LLM call or abort within 5s
- [ ] Stale job_runs are marked `failed` with error "server shutdown"
- [ ] Process exits with code 0 on clean shutdown, 1 on timeout

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 10.1 | `ShutdownManager` class with priority-ordered handlers | 0.5 days |
| 10.2 | Wire all `mp serve` components into shutdown manager | 1 day |

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
