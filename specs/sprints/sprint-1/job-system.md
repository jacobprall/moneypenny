# Generic Job System

### What exists today

The `jobs` and `job_runs` tables exist. `jobs-repo.ts` provides CRUD.
The scheduler ticks on an interval, finds due jobs, and executes them —
but only supports `AGENT_RUN_OPERATION`. This sprint extends the job
system to support arbitrary job types while preserving the existing
schema and scheduler behavior.

### Extended design

> **Note:** The `jobs` table already has an `operation TEXT NOT NULL` column
> that stores these values. No new `type` column is needed — the original
> schema-v10 draft proposed one but it was redundant. The `JobOperation`
> type below maps directly to the existing `operation` column.

```typescript
// Extend the existing Job interface, don't replace it.
// Values stored in the existing `operation` column.
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
