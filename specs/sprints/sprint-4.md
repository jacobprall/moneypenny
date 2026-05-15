# Sprint 4 — Evaluation Harness

> The sprint that lets you prove your agents work. A portable eval framework
> with SWE-bench import, multi-runner comparison, multi-session sequences,
> statistical analysis, Docker isolation, parallel execution, and CI
> regression gating.
>
> Ported from `moneypenny-rs/crates/mp-eval`, adapted for the TypeScript
> ecosystem with Bun as the runtime.

**Prerequisites:** Sprint 1 complete (AgentBridge, job system, blueprints).
Sprint 2 optional but beneficial (read/write separation for parallel trials).

---

## Overview

The eval harness ships as a new package `@moneypenny/eval` and a CLI
entry point `mp eval`. It is designed to answer one question: **does
changing my agent configuration, prompt, toolset, model, or context
pipeline make outcomes better or worse?**

| # | Workstream | Packages touched |
|---|-----------|-----------------|
| 1 | Task format and loading | `@moneypenny/eval` |
| 2 | Runner interface and built-in runners | `@moneypenny/eval` |
| 3 | Harness (single-task and multi-session) | `@moneypenny/eval` |
| 4 | Verification system | `@moneypenny/eval` |
| 5 | Results database | `@moneypenny/eval` |
| 6 | SWE-bench importer | `@moneypenny/eval` |
| 7 | Statistical analysis | `@moneypenny/eval` |
| 8 | Reporting (table, comparison, leaderboard, HTML) | `@moneypenny/eval` |
| 9 | Docker isolation | `@moneypenny/eval` |
| 10 | Parallel execution | `@moneypenny/eval` |
| 11 | CI regression gate | `@moneypenny/eval` |
| 12 | CLI integration (`mp eval`) | `@moneypenny/cli` |

---

## 1. Task Format and Loading

### Task definition

Tasks are YAML/JSON files that describe a problem, a repo context, and
how to verify the solution:

```typescript
export interface Task {
  id: string;
  repo: string;
  ref: string;                    // git ref to reset to (default: "HEAD")
  prompt: string;                 // the problem statement given to the agent
  verify?: VerifySpec;            // how to check if the agent succeeded
  contextGroundTruth?: RelevantDoc[];  // for context-quality (IR) evals
  description?: string;
  tags?: string[];
  difficulty?: "easy" | "medium" | "hard";
  language?: string;
  timeoutMs?: number;             // per-task timeout override
}

export interface TaskSequence {
  repo: string;
  ref: string;
  tasks: Task[];                  // ordered sequence of tasks
}
```

### Task file formats

A task file can contain:
- A single task object
- An array of tasks
- A `{ tasks: [...] }` wrapper

All three are auto-detected during loading.

### Loading

```typescript
export function loadTasks(dir: string): Task[];
export function loadSequence(path: string): TaskSequence;
```

`loadTasks` recursively walks a directory for `*.yaml`, `*.yml`, and
`*.json` files. Each file is parsed and normalized (default `ref` to
`"HEAD"`, inherit `repo` from parent directory name if not specified).

### Example task

```yaml
id: fix-null-check
repo: my-app
ref: abc123
prompt: |
  The `getUserName` function in `src/users.ts` throws a TypeError when the
  user object is null. Fix the null check and add a test.
verify:
  type: command
  command: "npx jest src/users.test.ts --no-coverage"
difficulty: easy
language: typescript
tags:
  - bug-fix
  - null-safety
```

### Example multi-session sequence

```yaml
repo: my-app
ref: main
tasks:
  - id: session-1-scaffold
    prompt: "Create a new REST endpoint for user preferences at /api/preferences"
    verify:
      type: command
      command: "curl -s http://localhost:3000/api/preferences | jq .status"

  - id: session-2-validation
    prompt: "Add input validation to the preferences endpoint. Reject invalid theme values."
    verify:
      type: command
      command: "npx jest src/api/preferences.test.ts"

  - id: session-3-persistence
    prompt: "Persist preferences to the database. Add a migration and update the endpoint."
    verify:
      type: composite
      checks:
        - type: command
          command: "npx jest src/api/preferences.test.ts"
        - type: grep-absent
          pattern: "TODO|FIXME|HACK"
          glob: "src/api/preferences.ts"
```

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 1.1 | `Task`, `TaskSequence` types, YAML/JSON parser | 1 day |
| 1.2 | Recursive directory walker, normalization, filtering | 0.5 days |

---

## 2. Runner Interface

### Core trait

Every evaluable agent system implements the `Runner` interface:

```typescript
export interface Runner {
  readonly name: string;

  run(task: Task, opts: RunOptions): Promise<RunMetrics>;

  /**
   * Whether this runner supports session persistence across multi-session
   * sequences. When true, the harness:
   * 1. Calls sessionStart() before the sequence
   * 2. Does NOT git reset between tasks (preserving the working tree)
   * 3. Calls sessionEnd() after the last task
   *
   * This lets runners with memory (mp-agent, claude-mp) leverage
   * accumulated context, demonstrating the value of persistent intelligence.
   */
  supportsSessionPersistence?: boolean;

  sessionStart?(workdir: string, model: string): Promise<void>;
  sessionEnd?(): Promise<void>;
}

export interface RunOptions {
  workdir: string;
  model: string;
  timeoutMs: number;
}

export interface RunMetrics {
  costUsd: number;
  wallTimeMs: number;
  turns: number;
  inputTokens: number;
  outputTokens: number;
  cachedTokens: number;
  toolCalls: number;
  model: string;
  error?: string;
  recall?: number;               // IR metrics (context runner only)
  mrr?: number;
  ndcg?: number;
  hitRate?: number;
}
```

### Built-in runners

| Runner name | Description | Session persistence |
|------------|-------------|:---:|
| `mp-agent` | Moneypenny's own agent loop via `AgentBridge` | Yes |
| `claude` | Claude Code CLI (vanilla, no MCP) | No |
| `claude-mp` | Claude Code CLI with moneypenny MCP server | Yes |
| `cursor` | Cursor CLI agent via `cursor-agent` | No |
| `aider` | Aider CLI | No |
| `codex` | OpenAI Codex CLI | No |
| `shell` | Arbitrary shell command (for custom agents) | No |
| `http` | HTTP endpoint (for remote agents) | No |
| `context` | Context-quality only (no LLM, measures IR retrieval) | No |

### `mp-agent` runner

The most important runner — it evaluates moneypenny's own agent loop:

```typescript
export class MpAgentRunner implements Runner {
  readonly name = "mp-agent";
  readonly supportsSessionPersistence = true;

  private bridge: AgentBridge | null = null;
  private sessionId: string | null = null;

  async run(task: Task, opts: RunOptions): Promise<RunMetrics> {
    const startTime = Date.now();
    const bridge = this.bridge ?? await this.createBridge(opts);

    let inputTokens = 0;
    let outputTokens = 0;
    let cachedTokens = 0;
    let toolCalls = 0;
    let costUsd = 0;
    let turns = 0;
    let error: string | undefined;

    try {
      for await (const event of bridge.run(task.prompt, {
        sessionId: this.sessionId ?? undefined,
        blueprint: "default",
      })) {
        switch (event.type) {
          case "tool_call_result":
            toolCalls++;
            break;
          case "turn_complete":
            turns++;
            inputTokens += event.usage.inputTokens;
            outputTokens += event.usage.outputTokens;
            cachedTokens += event.usage.cachedTokens ?? 0;
            costUsd += event.costUsd;
            break;
          case "error":
            error = event.message;
            break;
        }
      }
    } catch (e) {
      error = String(e);
    }

    return {
      costUsd,
      wallTimeMs: Date.now() - startTime,
      turns,
      inputTokens,
      outputTokens,
      cachedTokens,
      toolCalls,
      model: opts.model,
      error,
    };
  }

  async sessionStart(workdir: string, model: string): Promise<void> {
    this.bridge = await this.createBridge({ workdir, model, timeoutMs: 300_000 });
    this.sessionId = crypto.randomUUID();
  }

  async sessionEnd(): Promise<void> {
    this.bridge?.abort();
    this.bridge = null;
    this.sessionId = null;
  }

  private async createBridge(opts: RunOptions): Promise<AgentBridge> {
    // initialize AgentDB, create AgentLoop, wrap in AgentBridge
    // ...
  }
}
```

### `claude` runner (vanilla Claude Code)

```typescript
export class ClaudeCodeRunner implements Runner {
  readonly name: string;
  private augmented: boolean;
  private mpBinary: string;

  constructor(opts?: { augmented?: boolean; mpBinary?: string }) {
    this.augmented = opts?.augmented ?? false;
    this.name = this.augmented ? "claude-mp" : "claude";
    this.mpBinary = opts?.mpBinary ?? "mp";
  }

  get supportsSessionPersistence(): boolean {
    return this.augmented;
  }

  async run(task: Task, opts: RunOptions): Promise<RunMetrics> {
    const startTime = Date.now();
    const args = [
      "--print", task.prompt,
      "--model", opts.model,
      "--output-format", "json",
      "--max-turns", "50",
    ];

    if (this.augmented) {
      // start moneypenny MCP server before the run
    }

    const result = await execWithTimeout("claude", args, {
      cwd: opts.workdir,
      timeoutMs: opts.timeoutMs,
    });

    // parse JSON output for metrics
    return this.parseMetrics(result, opts.model, Date.now() - startTime);
  }
}
```

### `shell` runner

Wraps any command-line tool:

```typescript
export class ShellRunner implements Runner {
  readonly name = "shell";
  private commandTemplate: string;

  constructor(commandTemplate: string) {
    this.commandTemplate = commandTemplate;
  }

  async run(task: Task, opts: RunOptions): Promise<RunMetrics> {
    const startTime = Date.now();
    const command = this.commandTemplate
      .replace("{{prompt}}", task.prompt)
      .replace("{{workdir}}", opts.workdir)
      .replace("{{model}}", opts.model);

    const result = await execWithTimeout("bash", ["-c", command], {
      cwd: opts.workdir,
      timeoutMs: opts.timeoutMs,
    });

    return {
      costUsd: 0,
      wallTimeMs: Date.now() - startTime,
      turns: 0,
      inputTokens: 0,
      outputTokens: 0,
      cachedTokens: 0,
      toolCalls: 0,
      model: opts.model,
      error: result.exitCode !== 0 ? result.stderr : undefined,
    };
  }
}
```

### `context` runner (IR evaluation, no LLM)

Measures context retrieval quality without running an agent. Uses the
`contextGroundTruth` field from tasks to compute information retrieval
metrics:

```typescript
export class ContextRunner implements Runner {
  readonly name = "context";

  async run(task: Task, opts: RunOptions): Promise<RunMetrics> {
    if (!task.contextGroundTruth) {
      return RunMetrics.empty(opts.model);
    }

    // 1. Build index for the repo
    // 2. Query with task.prompt
    // 3. Compare retrieved chunks against ground truth
    // 4. Compute recall, MRR, NDCG, hit rate

    const retrieved = await queryIndex(task.prompt, opts.workdir);
    const truth = task.contextGroundTruth;

    return {
      costUsd: 0,
      wallTimeMs: 0,
      turns: 0,
      inputTokens: 0,
      outputTokens: 0,
      cachedTokens: 0,
      toolCalls: 0,
      model: "none",
      recall: computeRecall(retrieved, truth),
      mrr: computeMRR(retrieved, truth),
      ndcg: computeNDCG(retrieved, truth),
      hitRate: computeHitRate(retrieved, truth),
    };
  }
}
```

### Runner registry

```typescript
export function getRunner(name: string, opts?: RunnerOptions): Runner {
  switch (name) {
    case "mp-agent": return new MpAgentRunner();
    case "claude": return new ClaudeCodeRunner();
    case "claude-mp": return new ClaudeCodeRunner({ augmented: true, mpBinary: opts?.mpBinary });
    case "cursor": return new CursorRunner();
    case "aider": return new AiderRunner();
    case "codex": return new CodexRunner();
    case "shell": return new ShellRunner(opts?.shellCommand ?? "echo 'no command'");
    case "http": return new HttpRunner(opts?.httpEndpoint ?? "http://localhost:8080");
    case "context": return new ContextRunner();
    default: throw new Error(`Unknown runner: ${name}. Available: ${listRunners().join(", ")}`);
  }
}

export function listRunners(): string[] {
  return ["mp-agent", "claude", "claude-mp", "cursor", "aider", "codex", "shell", "http", "context"];
}
```

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 2.1 | `Runner` interface, `RunMetrics`, `RunOptions` types | 0.5 days |
| 2.2 | `MpAgentRunner` (uses AgentBridge) | 2 days |
| 2.3 | `ClaudeCodeRunner` (vanilla + augmented) | 1.5 days |
| 2.4 | `ShellRunner` + `HttpRunner` | 1 day |
| 2.5 | `CursorRunner` + `AiderRunner` + `CodexRunner` | 1.5 days |
| 2.6 | `ContextRunner` with IR metrics (recall, MRR, NDCG, hit rate) | 2 days |
| 2.7 | Runner registry + `listRunners` | 0.5 days |

---

## 3. Harness

### Single-task evaluation

The harness orchestrates: for each task × runner × trial, reset the repo,
run the agent, verify the result, record the outcome.

```typescript
export interface EvalConfig {
  tasks: Task[];
  runners: string[];
  trials: number;                 // repetitions per task per runner (default 3)
  model: string;
  reposDir: string;
  resultsDb: string;
  timeoutMs: number;              // default 300_000 (5 min)
}

export interface HarnessCallbacks {
  onTrialStart?(task: Task, runner: string, trial: number): void;
  onTrialEnd?(result: RunResult): void;
  onTaskStart?(task: Task): void;
  onTaskEnd?(task: Task): void;
  onError?(task: Task, runner: string, trial: number, error: string): void;
}

export async function runEval(
  config: EvalConfig,
  runners: Runner[],
  callbacks?: HarnessCallbacks,
): Promise<RunResult[]>;
```

### Evaluation flow

```
for each task:
  callbacks.onTaskStart(task)
  for each runner:
    for trial = 1..N:
      callbacks.onTrialStart(task, runner, trial)

      1. git reset --hard <task.ref> in repos/<task.repo>
      2. runner.run(task, opts) → RunMetrics
      3. run verification (task.verify) → passed: boolean
      4. record RunResult to results DB
      5. callbacks.onTrialEnd(result)

  callbacks.onTaskEnd(task)
```

### Multi-session evaluation

Tests agent memory and context accumulation across a sequence of related
tasks in the same repo:

```typescript
export async function runMultiSession(
  sequence: TaskSequence,
  config: EvalConfig,
  runners: Runner[],
  callbacks?: HarnessCallbacks,
): Promise<RunResult[]>;
```

Key behavior differences from single-task:

1. **Persistent runners** (those with `supportsSessionPersistence = true`):
   - `sessionStart()` called once before the sequence
   - **No git reset** between tasks — the working tree accumulates changes
   - `sessionEnd()` called after the last task
   - This simulates a real multi-session workflow where the agent builds
     on prior work

2. **Non-persistent runners** (the control group):
   - `git reset --hard` before every task
   - Starting from scratch each time
   - This measures how much value persistence provides

3. Each result includes a `sessionNumber` (1, 2, 3...) for analysis

The multi-session mode directly measures the value proposition of
moneypenny's intelligence file: does accumulated context help the agent
solve later tasks better, faster, or cheaper?

### Git operations

```typescript
export async function gitReset(workdir: string, ref: string): Promise<void>;
export async function gitClone(repo: string, dest: string, depth?: number): Promise<void>;
```

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 3.1 | `runEval` single-task harness with callbacks | 2 days |
| 3.2 | `runMultiSession` harness with persistence support | 2 days |
| 3.3 | Git operations (reset, clone) with error handling | 0.5 days |

---

## 4. Verification System

### VerifySpec

Verification checks whether the agent's changes actually solve the problem:

```typescript
export type VerifySpec =
  | { type: "command"; command: string; cwd?: string }
  | { type: "pytest"; testPath: string; args?: string[] }
  | { type: "grep-absent"; pattern: string; glob?: string }
  | { type: "composite"; checks: VerifySpec[] };

export interface VerifyResult {
  passed: boolean;
  output: string;
}
```

### Verification types

| Type | Description | Pass condition |
|------|-------------|---------------|
| `command` | Run a shell command | Exit code 0 |
| `pytest` | Run pytest on specific test files | All tests pass |
| `grep-absent` | Search for a pattern that should NOT exist | Zero matches |
| `composite` | Run multiple checks in order | All pass (short-circuit on failure) |

### Execution

```typescript
export async function runVerify(
  spec: VerifySpec,
  workdir: string,
  timeoutMs: number,
): Promise<VerifyResult> {
  switch (spec.type) {
    case "command":
      return execCommand(spec.command, spec.cwd ? `${workdir}/${spec.cwd}` : workdir, timeoutMs);

    case "pytest": {
      const extra = spec.args?.join(" ") ?? "";
      const cmd = `python -m pytest ${spec.testPath} ${extra} --tb=short -q`;
      return execCommand(cmd, workdir, timeoutMs);
    }

    case "grep-absent": {
      const glob = spec.glob ?? "**/*";
      const result = await execCommand(
        `rg --count "${spec.pattern}" --glob "${glob}" || true`,
        workdir, timeoutMs,
      );
      const count = result.output
        .split("\n")
        .filter(l => l.includes(":"))
        .reduce((sum, l) => sum + parseInt(l.split(":").pop() ?? "0", 10), 0);
      return { passed: count === 0, output: result.output };
    }

    case "composite": {
      const outputs: string[] = [];
      for (const check of spec.checks) {
        const sub = await runVerify(check, workdir, timeoutMs);
        outputs.push(sub.output);
        if (!sub.passed) {
          return { passed: false, output: outputs.join("\n---\n") };
        }
      }
      return { passed: true, output: outputs.join("\n---\n") };
    }
  }
}
```

### Timeout handling

All command executions use `Bun.spawn` with a timeout. If the process
exceeds the timeout, it is killed and the trial is marked as failed with
error `"timed out after Ns"`.

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 4.1 | `VerifySpec` types, `runVerify` dispatcher | 1 day |
| 4.2 | Command execution with timeout (Bun.spawn) | 0.5 days |
| 4.3 | Composite verification, grep-absent logic | 0.5 days |

---

## 5. Results Database

### Schema

Results are stored in a separate SQLite database (not the intelligence
file). This keeps eval data portable and shareable.

```sql
CREATE TABLE IF NOT EXISTS results (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  task_id TEXT NOT NULL,
  runner TEXT NOT NULL,
  trial INTEGER NOT NULL,
  passed INTEGER NOT NULL,
  cost_usd REAL NOT NULL DEFAULT 0.0,
  wall_time_ms INTEGER NOT NULL DEFAULT 0,
  turns INTEGER NOT NULL DEFAULT 0,
  input_tokens INTEGER NOT NULL DEFAULT 0,
  output_tokens INTEGER NOT NULL DEFAULT 0,
  cached_tokens INTEGER NOT NULL DEFAULT 0,
  tool_calls INTEGER NOT NULL DEFAULT 0,
  model TEXT NOT NULL,
  session_number INTEGER,          -- populated for multi-session sequences
  error TEXT,
  recall REAL,                     -- IR metrics
  mrr REAL,
  ndcg REAL,
  hit_rate REAL,
  timestamp INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_results_task ON results(task_id, runner);
CREATE INDEX IF NOT EXISTS idx_results_runner ON results(runner);
CREATE INDEX IF NOT EXISTS idx_results_session ON results(task_id, runner, session_number);
```

### RunResult type

```typescript
export interface RunResult {
  taskId: string;
  runner: string;
  trial: number;
  passed: boolean;
  costUsd: number;
  wallTimeMs: number;
  turns: number;
  inputTokens: number;
  outputTokens: number;
  cachedTokens: number;
  toolCalls: number;
  model: string;
  sessionNumber?: number;
  error?: string;
  recall?: number;
  mrr?: number;
  ndcg?: number;
  hitRate?: number;
  timestamp: number;
}
```

### ResultsDB class

```typescript
export class ResultsDB {
  constructor(dbPath: string);

  insert(result: RunResult): void;
  insertBatch(results: RunResult[]): void;

  getResults(opts?: {
    runner?: string;
    taskId?: string;
    model?: string;
  }): RunResult[];

  getReport(): ReportRow[];
  getMultiSessionReport(): SessionReportRow[];

  getRunners(): string[];
  getTasks(): string[];
}

export interface ReportRow {
  taskId: string;
  runner: string;
  passRate: number;
  avgCostUsd: number;
  avgWallTimeMs: number;
  avgTurns: number;
  trials: number;
}

export interface SessionReportRow {
  taskId: string;
  runner: string;
  sessionNumber: number;
  passRate: number;
  avgCostUsd: number;
  avgWallTimeMs: number;
}
```

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 5.1 | Schema, `ResultsDB` class with insert/query | 1 day |
| 5.2 | Report queries (aggregate, multi-session, by-runner) | 1 day |

---

## 6. SWE-bench Importer

### Problem

SWE-bench is the standard benchmark for evaluating coding agents. It ships
as a JSON dataset with problem statements, patches, and test commands.
The importer converts SWE-bench instances into mp-eval task files.

### SWE-bench instance format

```typescript
export interface SweBenchInstance {
  instance_id: string;
  repo: string;
  base_commit: string;
  problem_statement: string;
  hints_text: string;
  test_patch: string;
  patch: string;                  // gold patch (not given to agent)
  version: string;
  FAIL_TO_PASS?: string;         // JSON array of test IDs
  PASS_TO_PASS?: string;
  created_at?: string;
  environment_setup_commit?: string;
}
```

### Import process

```typescript
export interface ImportOptions {
  includeHints: boolean;          // include hints_text in prompt
  difficultyFromPatchSize: boolean;  // estimate difficulty from patch LOC
  maxTasks?: number;              // cap number of imported tasks
}

export function importSweBench(jsonPath: string, opts: ImportOptions): Task[];
export function writeTasksByRepo(tasks: Task[], outputDir: string): number;
```

1. Parse SWE-bench JSON
2. For each instance:
   - Build prompt from `problem_statement` (+ optional `hints_text`)
   - Extract repo name (e.g., `django/django` → `django`)
   - Use `base_commit` as the git ref
   - Build `VerifySpec` from `test_patch` and `FAIL_TO_PASS`
   - Estimate difficulty from patch size (lines changed × files changed)
   - Detect language from repo name
   - Tag with `swe-bench`, difficulty, version
3. Write tasks as YAML grouped by repo

### Difficulty estimation

```typescript
function estimateDifficulty(patch: string): "easy" | "medium" | "hard" {
  const linesChanged = patch
    .split("\n")
    .filter(l => (l.startsWith("+") || l.startsWith("-")) && !l.startsWith("+++") && !l.startsWith("---"))
    .length;
  const filesChanged = patch
    .split("\n")
    .filter(l => l.startsWith("diff --git"))
    .length;

  if (filesChanged <= 1 && linesChanged <= 20) return "easy";
  if (filesChanged <= 3 && linesChanged <= 80) return "medium";
  return "hard";
}
```

### Verify spec generation

For SWE-bench tasks, verification applies the test patch and runs pytest
on the failing tests:

```typescript
function makeVerifySpec(testPatch: string, testFiles: string[]): VerifySpec {
  if (testFiles.length === 0) {
    return { type: "command", command: "python -m pytest --tb=short -q" };
  }
  return {
    type: "pytest",
    testPath: testFiles[0],
    args: testFiles.length > 1 ? testFiles.slice(1) : undefined,
  };
}
```

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 6.1 | SWE-bench JSON parser, instance → task converter | 1.5 days |
| 6.2 | Difficulty estimation, language detection, test file extraction | 0.5 days |
| 6.3 | Write YAML grouped by repo, verify spec generation | 0.5 days |

---

## 7. Statistical Analysis

### Problem

Eval results are noisy. An agent that passes 7/10 trials may or may not be
better than one that passes 6/10. We need proper statistical methods to
distinguish signal from noise.

### Wilson score confidence intervals

For pass rates with small sample sizes (typical eval: 3–10 trials), the
Wilson score interval is more accurate than the normal approximation:

```typescript
export interface ConfidenceInterval {
  pointEstimate: number;
  lower: number;
  upper: number;
  n: number;
}

export function wilsonCI(successes: number, trials: number, confidence?: number): ConfidenceInterval;
```

### McNemar's test for paired comparisons

When comparing two runners on the same tasks, McNemar's test determines
if the difference in pass rates is statistically significant. It uses
the 2×2 contingency table of discordant pairs:

```
                Runner B
                Pass    Fail
Runner A  Pass  [both]  [A wins]
          Fail  [B wins] [both fail]
```

```typescript
export interface McNemarResult {
  runnerA: string;
  runnerB: string;
  aWins: number;                  // A passed, B failed
  bWins: number;                  // B passed, A failed
  bothPass: number;
  bothFail: number;
  chiSquared: number;             // with continuity correction
  pValue: number;
  significantAt05: boolean;
}

export function mcnemarTest(results: RunResult[], runnerA: string, runnerB: string): McNemarResult;
```

### Efficiency metrics

Per-runner aggregate statistics:

```typescript
export interface EfficiencyMetrics {
  runner: string;
  totalTasks: number;
  passed: number;
  passRate: ConfidenceInterval;
  avgCostUsd: number;
  costPerPass: number | null;     // total cost / passed (null if 0 passes)
  avgWallTimeMs: number;
  avgTurns: number;
  totalInputTokens: number;
  totalOutputTokens: number;
  totalCachedTokens: number;
  cacheHitRate: number;           // cached / input
  cacheSavingsPct: number;        // estimated % saved from caching
  avgToolCalls: number;
}

export function computeEfficiency(runnerName: string, results: RunResult[]): EfficiencyMetrics;
```

### Runner comparison

Full A/B comparison combining all statistical methods:

```typescript
export interface RunnerComparison {
  runnerA: EfficiencyMetrics;
  runnerB: EfficiencyMetrics;
  mcnemar: McNemarResult;
  costRatio: number | null;       // A's cost-per-pass / B's cost-per-pass
  timeRatio: number | null;       // A's avg time / B's avg time
}

export function compareRunners(
  results: RunResult[],
  runnerA: string,
  runnerB: string,
): RunnerComparison;
```

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 7.1 | Wilson CI, z-score approximation | 0.5 days |
| 7.2 | McNemar's test with continuity correction, chi-squared p-value | 1 day |
| 7.3 | Efficiency metrics, cache analysis | 0.5 days |
| 7.4 | `compareRunners` full A/B comparison | 0.5 days |
| 7.5 | Unit tests (replicating moneypenny-rs test cases) | 0.5 days |

---

## 8. Reporting

### Output formats

| Format | Description | Use case |
|--------|------------|----------|
| `table` | ASCII table with pass rates, costs, timing | Terminal output |
| `comparison` | Side-by-side runner comparison with stats | A/B analysis |
| `leaderboard` | Ranked runners sorted by configurable metric | Ranking |
| `json` | Machine-readable results | CI integration |
| `html` | Rich HTML report with charts | Sharing |
| `stats` | Statistical analysis (McNemar, CI) | Research |

### Table format

```
┌──────────────────┬─────────┬──────────┬──────────┬──────────┬────────┐
│ Task             │ Runner  │ Pass Rate│ Avg Cost │ Avg Time │ Trials │
├──────────────────┼─────────┼──────────┼──────────┼──────────┼────────┤
│ fix-null-check   │ mp-agent│ 100%     │ $0.023   │ 12.3s    │ 3      │
│ fix-null-check   │ claude  │ 67%      │ $0.031   │ 18.7s    │ 3      │
│ add-validation   │ mp-agent│ 100%     │ $0.045   │ 22.1s    │ 3      │
│ add-validation   │ claude  │ 33%      │ $0.052   │ 35.2s    │ 3      │
└──────────────────┴─────────┴──────────┴──────────┴──────────┴────────┘
```

### Comparison format

```
═══════════════════════════════════════════════════════════════
                   mp-agent  vs  claude
═══════════════════════════════════════════════════════════════
Pass rate:         85.0%         62.5%
                   [72-93% CI]   [48-75% CI]
───────────────────────────────────────────────────────────────
Cost/pass:         $0.034        $0.058     (0.59x)
Avg time:          17.2s         28.4s      (0.61x)
Avg turns:         4.2           6.8
Cache hit rate:    42.3%         0.0%
Cache savings:     38.1%         0.0%
───────────────────────────────────────────────────────────────
McNemar χ²:        4.17          p = 0.041  *significant*
  mp-agent wins:   8 tasks
  claude wins:     2 tasks
  Both pass:       12 tasks
  Both fail:       3 tasks
═══════════════════════════════════════════════════════════════
```

### Leaderboard format

```
# Agent Leaderboard (sorted by pass rate)
┌────┬──────────┬──────────┬──────────┬───────────┬────────────┐
│ #  │ Runner   │ Pass Rate│ Cost/Pass│ Avg Time  │ Cache Svgs │
├────┼──────────┼──────────┼──────────┼───────────┼────────────┤
│ 1  │ mp-agent │ 85.0%    │ $0.034   │ 17.2s     │ 38.1%      │
│ 2  │ claude-mp│ 77.5%    │ $0.041   │ 22.8s     │ 31.2%      │
│ 3  │ claude   │ 62.5%    │ $0.058   │ 28.4s     │ 0.0%       │
│ 4  │ aider    │ 55.0%    │ $0.072   │ 34.1s     │ 0.0%       │
│ 5  │ codex    │ 47.5%    │ $0.089   │ 41.6s     │ 0.0%       │
└────┴──────────┴──────────┴──────────┴───────────┴────────────┘
```

### HTML report

A self-contained HTML file with:
- Summary table
- Per-task pass/fail heatmap
- Cost and timing charts
- Statistical comparison results
- Multi-session progression charts (if applicable)
- Filterable by difficulty, language, tags

### Report generation

```typescript
export function generateReport(
  resultsDb: string,
  format: "table" | "comparison" | "leaderboard" | "json" | "html" | "stats",
  opts?: {
    compare?: [string, string];   // for comparison/stats formats
    multiSession?: boolean;
    sortBy?: "pass_rate" | "cost" | "time" | "efficiency";
  },
): string;
```

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 8.1 | Table formatter (ASCII tables for terminal) | 1 day |
| 8.2 | Comparison formatter (side-by-side with stats) | 1 day |
| 8.3 | Leaderboard formatter (sortable ranking) | 0.5 days |
| 8.4 | JSON output | 0.5 days |
| 8.5 | HTML report (self-contained, with charts) | 2 days |
| 8.6 | Multi-session specific reporting | 1 day |

---

## 9. Docker Isolation

### Problem

Eval trials can interfere with each other: residual files, modified git
state, environment variable leaks. Docker provides clean isolation per
trial, especially important for SWE-bench tasks that install dependencies.

### DockerConfig

```typescript
export interface DockerConfig {
  image: string;                  // default: "mp-eval-sandbox"
  timeoutMs: number;              // container timeout
  mountRepos: boolean;            // mount repo as /workspace
  networkAccess: boolean;         // default: false (--network=none)
  memoryLimit: string;            // default: "4g"
  cpuLimit: string;               // default: "2"
}
```

### Container execution

```typescript
export async function runInContainer(
  config: DockerConfig,
  workdir: string,
  command: string,
): Promise<{ stdout: string; stderr: string; exitCode: number }>;

export async function checkDocker(config: DockerConfig): Promise<void>;
```

Each trial runs in a fresh container:
- `--rm` for automatic cleanup
- `--network=none` by default (agents shouldn't need network in eval)
- `--memory=4g --cpus=2` resource limits
- Repo mounted as `/workspace:rw`

### Dockerfile

Generated by `mp eval setup --dockerfile`:

```dockerfile
FROM python:3.12-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    git curl build-essential && \
    rm -rf /var/lib/apt/lists/*

RUN pip install --no-cache-dir pytest pytest-timeout

# Node.js for TypeScript tasks
RUN curl -fsSL https://deb.nodesource.com/setup_22.x | bash - && \
    apt-get install -y nodejs && \
    npm install -g jest typescript

# Bun for moneypenny runner
RUN curl -fsSL https://bun.sh/install | bash
ENV PATH="/root/.bun/bin:${PATH}"

WORKDIR /workspace
```

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 9.1 | `DockerConfig`, `checkDocker`, `runInContainer` | 1 day |
| 9.2 | Dockerfile generation, build command | 0.5 days |
| 9.3 | Integration with harness (--docker flag) | 0.5 days |

---

## 10. Parallel Execution

### Problem

Running 10 tasks × 3 runners × 3 trials = 90 trials sequentially at 5 min
each = 7.5 hours. Parallelism brings this to under an hour.

### Design

```typescript
export async function runParallel<T>(
  tasks: Array<() => Promise<T>>,
  parallelism: number,
): Promise<T[]>;
```

A simple bounded-concurrency executor using a semaphore pattern.
Order of results is preserved (matches input order).

### Harness integration

```typescript
// In runEval, when parallelism > 1:
const trialTasks = allTrials.map(({ task, runner, trial }) =>
  async () => {
    const workdir = docker
      ? await prepareDockerWorkdir(task, config)
      : await prepareWorkdir(task, config);

    const metrics = await runner.run(task, { workdir, model, timeoutMs });
    const passed = task.verify ? await runVerify(task.verify, workdir, timeoutMs) : metrics.error == null;

    return { task, runner: runner.name, trial, metrics, passed };
  }
);

const results = await runParallel(trialTasks, parallelism);
```

When Docker is enabled, each parallel trial gets its own container, so
there's no state leakage. Without Docker, each trial gets a copy of the
repo (shallow clone) to avoid git state conflicts.

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 10.1 | `runParallel` bounded-concurrency executor | 0.5 days |
| 10.2 | Workdir isolation for non-Docker parallel runs | 1 day |
| 10.3 | Integration with harness and Docker | 0.5 days |

---

## 11. CI Regression Gate

### Problem

You want to ensure that agent changes don't cause regressions. The CI gate
compares current eval results against a baseline and fails the build if
pass rates drop beyond a threshold.

### Design

```typescript
export interface CiGateConfig {
  baselinePath: string;           // path to baseline results.sqlite
  currentPath: string;            // path to current results.sqlite
  threshold: number;              // regression threshold (default 0.05 = 5%)
}

export interface CiGateResult {
  passed: boolean;
  regressions: Regression[];
  improvements: Improvement[];
  unchanged: string[];
}

export interface Regression {
  taskId: string;
  runner: string;
  baselinePassRate: number;
  currentPassRate: number;
  delta: number;
}

export interface Improvement {
  taskId: string;
  runner: string;
  baselinePassRate: number;
  currentPassRate: number;
  delta: number;
}
```

### Gate logic

```typescript
export function runCiGate(config: CiGateConfig): CiGateResult {
  const baseline = new ResultsDB(config.baselinePath);
  const current = new ResultsDB(config.currentPath);

  const baselineReport = baseline.getReport();
  const currentReport = current.getReport();

  const regressions: Regression[] = [];
  const improvements: Improvement[] = [];
  const unchanged: string[] = [];

  for (const br of baselineReport) {
    const cr = currentReport.find(r => r.taskId === br.taskId && r.runner === br.runner);
    if (!cr) continue;

    const delta = cr.passRate - br.passRate;
    if (delta < -config.threshold) {
      regressions.push({
        taskId: br.taskId,
        runner: br.runner,
        baselinePassRate: br.passRate,
        currentPassRate: cr.passRate,
        delta,
      });
    } else if (delta > config.threshold) {
      improvements.push({
        taskId: br.taskId,
        runner: br.runner,
        baselinePassRate: br.passRate,
        currentPassRate: cr.passRate,
        delta,
      });
    } else {
      unchanged.push(`${br.taskId}/${br.runner}`);
    }
  }

  return {
    passed: regressions.length === 0,
    regressions,
    improvements,
    unchanged,
  };
}
```

### CI output

```
mp eval ci --baseline baseline.sqlite --current results.sqlite --threshold 0.05

CI Regression Gate
══════════════════
Threshold: 5%

✗ REGRESSIONS (2):
  fix-null-check / mp-agent: 100% → 67% (Δ -33%)
  add-auth / mp-agent: 67% → 33% (Δ -33%)

✓ IMPROVEMENTS (1):
  parse-csv / mp-agent: 33% → 100% (Δ +67%)

─ UNCHANGED (15 task/runner pairs)

RESULT: FAIL (2 regressions exceed 5% threshold)
```

Exit code: 0 on pass, 1 on regression.

### GitHub Actions integration

```yaml
# .github/workflows/eval.yml
name: Agent Eval
on:
  pull_request:
    paths:
      - "packages/loop/**"
      - "packages/ctx/**"
      - "packages/tools/**"
      - "packages/agents/**"

jobs:
  eval:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - uses: oven-sh/setup-bun@v2

      - run: bun install

      - name: Setup eval repos
        run: bun run mp eval setup --repos-dir ./repos --repos my-app

      - name: Run eval
        run: |
          bun run mp eval run \
            --tasks ./eval/tasks \
            --runners mp-agent \
            --model claude-sonnet-4-6 \
            --trials 3 \
            --repos-dir ./repos \
            --results-db ./results.sqlite

      - name: Regression gate
        run: |
          bun run mp eval ci \
            --baseline ./eval/baseline.sqlite \
            --current ./results.sqlite \
            --threshold 0.05

      - uses: actions/upload-artifact@v4
        if: always()
        with:
          name: eval-results
          path: ./results.sqlite
```

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 11.1 | `runCiGate` comparison logic | 1 day |
| 11.2 | Terminal output formatting | 0.5 days |
| 11.3 | GitHub Actions workflow template | 0.5 days |

---

## 12. CLI Integration

### `mp eval` subcommands

Mirror the moneypenny-rs CLI interface:

```
mp eval run          Run end-to-end evaluation trials
mp eval sequence     Run a multi-session sequence evaluation
mp eval context      Run context-quality evaluation (no LLM)
mp eval report       Generate evaluation reports
mp eval import       Import tasks from SWE-bench JSON dataset
mp eval setup        Clone test repos and build Docker image
mp eval list-runners List available runners
mp eval leaderboard  Generate a ranked leaderboard
mp eval ci           CI regression gate
```

### Common options

```
mp eval run
  --tasks <dir>          Directory containing task YAML/JSON files
  --runners <names...>   Runner names to evaluate
  --model <model>        Model to use (default: claude-sonnet-4-6)
  --trials <n>           Number of trials per task per runner (default: 3)
  --repos-dir <dir>      Directory containing cloned repos (default: ./repos)
  --results-db <path>    Path to results SQLite database (default: ./results.sqlite)
  --timeout <ms>         Timeout per trial in milliseconds (default: 300000)
  --filter <regex>       Only run tasks matching this regex
  --difficulty <level>   Only run tasks with this difficulty
  --parallelism <n>      Number of parallel trials (default: 1)
  --docker               Run each trial in a Docker container
  --docker-image <name>  Docker image (default: mp-eval-sandbox)
  --shell-command <cmd>  Command template for the shell runner
  --mp-binary <path>     Path to mp binary for claude-mp runner
  --http-endpoint <url>  HTTP endpoint for the http runner
```

### Makefile targets

```makefile
eval:
	bun run mp eval run --tasks ./eval/tasks --runners mp-agent claude --trials 3

eval-ab:
	bun run mp eval run --tasks ./eval/tasks --runners mp-agent claude --trials 5
	bun run mp eval report --format comparison --compare mp-agent claude

eval-context:
	bun run mp eval context --tasks ./eval/tasks --repo-path .

eval-swe-bench:
	bun run mp eval import --input ./eval/swe-bench-verified.json --output ./eval/tasks/swe-bench
	bun run mp eval setup --repos-dir ./repos
	bun run mp eval run --tasks ./eval/tasks/swe-bench --runners mp-agent claude --trials 3 --docker

eval-report:
	bun run mp eval report --format html > eval-report.html

eval-ci:
	bun run mp eval ci --baseline ./eval/baseline.sqlite --current ./results.sqlite --threshold 0.05
```

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 12.1 | CLI arg parsing (all subcommands) | 1.5 days |
| 12.2 | Wire subcommands to harness/reporter/importer | 1 day |
| 12.3 | Makefile targets + documentation | 0.5 days |

---

## Package Structure

```
packages/eval/
├── package.json
├── src/
│   ├── index.ts                 Entry point (exports all public API)
│   ├── task.ts                  Task, TaskSequence, loadTasks, loadSequence
│   ├── runner/
│   │   ├── index.ts             Runner interface, getRunner, listRunners
│   │   ├── mp-agent.ts          MpAgentRunner
│   │   ├── claude-code.ts       ClaudeCodeRunner (vanilla + augmented)
│   │   ├── cursor.ts            CursorRunner
│   │   ├── aider.ts             AiderRunner
│   │   ├── codex.ts             CodexRunner
│   │   ├── shell.ts             ShellRunner
│   │   ├── http.ts              HttpRunner
│   │   └── context.ts           ContextRunner (IR metrics)
│   ├── harness.ts               runEval, runMultiSession, git operations
│   ├── verify.ts                VerifySpec, runVerify
│   ├── db.ts                    ResultsDB
│   ├── swe-bench.ts             SWE-bench importer
│   ├── stats.ts                 Wilson CI, McNemar, efficiency, comparison
│   ├── report.ts                All output formats
│   ├── docker.ts                DockerConfig, runInContainer, Dockerfile
│   └── parallel.ts              runParallel bounded concurrency
└── tests/
    ├── stats.test.ts            Statistical function tests
    ├── verify.test.ts           Verification tests
    ├── task.test.ts             Task loading tests
    └── swe-bench.test.ts        Importer tests
```

---

## Implementation Order

```
Phase 1: Foundation
  ├── §1 Task format + loading
  ├── §4 Verification system
  └── §5 Results database

Phase 2: Core harness
  ├── §2 Runner interface + mp-agent runner
  ├── §3 Harness (single-task + multi-session)
  └── §10 Parallel execution

Phase 3: Runners
  ├── §2.3 Claude Code runner
  ├── §2.4 Shell + HTTP runners
  ├── §2.5 Cursor + Aider + Codex runners
  └── §2.6 Context runner (IR)

Phase 4: Analysis + reporting
  ├── §7 Statistical analysis
  └── §8 Reporting (all formats)

Phase 5: SWE-bench + isolation
  ├── §6 SWE-bench importer
  └── §9 Docker isolation

Phase 6: CI + CLI
  ├── §11 CI regression gate
  └── §12 CLI integration
```

Phases 1–2 are sequential (foundation then harness). Phases 3–5 can
proceed in parallel. Phase 6 (CI + CLI) ties everything together.

---

## What we deliberately skip

- **GPU-accelerated eval** (CUDA containers, embedding model eval) — out
  of scope for an agent eval harness.
- **Distributed execution** (farm out trials to multiple machines) — the
  single-machine parallel approach is sufficient for solo dev use.
- **Live eval dashboard** (real-time streaming of trial results to web UI) —
  desirable but not essential for the first release. Results are available
  after completion via reports.
- **Custom scoring functions** (beyond pass/fail) — the `VerifySpec` is
  binary. Numeric scoring (e.g., code quality metrics) could be added
  later as a `score` verify type.
