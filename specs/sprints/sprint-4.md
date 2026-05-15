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

## 1. Task Format and Loading

### Task definition

```typescript
export interface Task {
  id: string;
  repo: string;
  ref: string;                    // git ref to reset to (default: "HEAD")
  prompt: string;
  verify?: VerifySpec;
  contextGroundTruth?: RelevantDoc[];
  description?: string;
  tags?: string[];
  difficulty?: "easy" | "medium" | "hard";
  language?: string;
  timeoutMs?: number;
  environmentSetup?: EnvironmentSetup;  // NEW: per-task environment requirements
}

export interface TaskSequence {
  repo: string;
  ref: string;
  tasks: Task[];
}

export interface EnvironmentSetup {
  pythonVersion?: string;         // e.g., "3.10"
  nodeVersion?: string;           // e.g., "22"
  installCommand?: string;        // e.g., "pip install -e .[dev]"
  envVars?: Record<string, string>;
  dockerImage?: string;           // override default Docker image
}
```

### Task file formats

A task file can contain a single task, an array of tasks, or a
`{ tasks: [...] }` wrapper. All three are auto-detected during loading.

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

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 1.1 | `Task`, `TaskSequence`, `EnvironmentSetup` types, YAML/JSON parser | 1 day |
| 1.2 | Recursive directory walker, normalization, filtering | 0.5 days |

---

## 2. Runner Interface

### Core interface

```typescript
export interface Runner {
  readonly name: string;
  run(task: Task, opts: RunOptions): Promise<RunMetrics>;
  supportsSessionPersistence?: boolean;
  sessionStart?(workdir: string, model: string): Promise<void>;
  sessionEnd?(): Promise<void>;
}

export interface RunOptions {
  workdir: string;
  model: string;
  timeoutMs: number;
  tools?: string[];               // NEW: tool allowlist for mp-agent runner
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
  recall?: number;
  mrr?: number;
  ndcg?: number;
  hitRate?: number;
}
```

### `execWithTimeout` — shared utility

Multiple runners and the verification module need subprocess execution
with timeout. A single shared utility:

```typescript
export interface ExecResult {
  stdout: string;
  stderr: string;
  exitCode: number;
  timedOut: boolean;
}

export async function execWithTimeout(
  command: string,
  args: string[],
  opts: {
    cwd: string;
    timeoutMs: number;
    env?: Record<string, string>;
  },
): Promise<ExecResult> {
  const proc = Bun.spawn([command, ...args], {
    cwd: opts.cwd,
    env: { ...process.env, ...opts.env },
    stdout: "pipe",
    stderr: "pipe",
  });

  const timer = setTimeout(() => proc.kill(), opts.timeoutMs);

  try {
    const [stdout, stderr] = await Promise.all([
      new Response(proc.stdout).text(),
      new Response(proc.stderr).text(),
    ]);
    const exitCode = await proc.exited;

    return {
      stdout,
      stderr,
      exitCode,
      timedOut: proc.killed,
    };
  } finally {
    clearTimeout(timer);
  }
}
```

### Built-in runners

| Runner name | Description | Session persistence | Cost tracking |
|------------|-------------|:---:|:---:|
| `mp-agent` | Moneypenny's own agent loop via `AgentBridge` | Yes | Full (tokens, cost, cache) |
| `claude` | Claude Code CLI (vanilla, no MCP) | No | Partial (no cached_tokens) |
| `claude-mp` | Claude Code CLI with moneypenny MCP server | Yes | Partial (no cached_tokens) |
| `cursor` | Cursor CLI agent | No | None |
| `aider` | Aider CLI | No | None |
| `codex` | OpenAI Codex CLI | No | Partial |
| `shell` | Arbitrary shell command | No | None |
| `http` | HTTP endpoint (for remote agents) | No | Depends on endpoint |
| `context` | Context-quality only (no LLM, measures IR) | No | N/A |

### Cost tracking limitations for external runners

External runners report metrics inconsistently:

| Runner | Tokens | Cost | Cache | Source |
|--------|:------:|:----:|:-----:|--------|
| `mp-agent` | Yes | Yes | Yes | `AgentBridge` events |
| `claude` | Yes | Yes | No | `--output-format json` (no `cached_tokens` field) |
| `aider` | No | No | No | No machine-readable output |
| `codex` | Partial | Partial | No | JSON output varies by version |
| `cursor` | No | No | No | No machine-readable output |

**Mitigation:** The efficiency report clearly labels which metrics are
unavailable per runner. Missing values are shown as `—` not `0`.
Cost-per-pass comparisons are only valid between runners with full cost
tracking.

```typescript
// In EfficiencyMetrics:
export interface EfficiencyMetrics {
  runner: string;
  // ...
  costTracked: boolean;           // false for runners with no cost data
  tokenTracked: boolean;          // false for runners with no token data
  cacheTracked: boolean;          // false for runners with no cache data
}
```

### `mp-agent` runner — tool availability

The `mp-agent` runner must use the same tools as production `mp chat`
sessions. The tool set is configured via `RunOptions.tools`:

```typescript
export class MpAgentRunner implements Runner {
  readonly name = "mp-agent";
  readonly supportsSessionPersistence = true;

  private bridge: AgentBridge | null = null;
  private sessionId: string | null = null;

  async run(task: Task, opts: RunOptions): Promise<RunMetrics> {
    const bridge = this.bridge ?? await this.createBridge(opts);

    // Default tools match production: file_read, file_write, bash, code_search,
    // memory_add, context_curate. Can be overridden via opts.tools.
    const tools = opts.tools ?? [
      "file_read", "file_write", "bash", "code_search",
      "memory_add", "context_curate",
    ];

    let inputTokens = 0, outputTokens = 0, cachedTokens = 0;
    let toolCalls = 0, costUsd = 0, turns = 0;
    let error: string | undefined;

    try {
      for await (const event of bridge.run(task.prompt, {
        sessionId: this.sessionId ?? undefined,
        blueprint: "default",
        tools,
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
      turns, inputTokens, outputTokens, cachedTokens,
      toolCalls, model: opts.model, error,
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
}
```

### `claude` runner (vanilla Claude Code)

```typescript
export class ClaudeCodeRunner implements Runner {
  readonly name: string;
  private augmented: boolean;

  constructor(opts?: { augmented?: boolean }) {
    this.augmented = opts?.augmented ?? false;
    this.name = this.augmented ? "claude-mp" : "claude";
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

    const result = await execWithTimeout("claude", args, {
      cwd: opts.workdir,
      timeoutMs: opts.timeoutMs,
    });

    return this.parseMetrics(result, opts.model, Date.now() - startTime);
  }

  private parseMetrics(result: ExecResult, model: string, wallTimeMs: number): RunMetrics {
    if (result.timedOut) {
      return { ...RunMetrics.empty(model), wallTimeMs, error: `timed out` };
    }

    try {
      const json = JSON.parse(result.stdout);
      return {
        costUsd: json.cost_usd ?? 0,
        wallTimeMs,
        turns: json.num_turns ?? 0,
        inputTokens: json.input_tokens ?? 0,
        outputTokens: json.output_tokens ?? 0,
        cachedTokens: 0,              // Claude Code JSON doesn't report cached tokens
        toolCalls: json.tool_calls ?? 0,
        model,
        error: result.exitCode !== 0 ? result.stderr.slice(0, 500) : undefined,
      };
    } catch {
      return {
        ...RunMetrics.empty(model),
        wallTimeMs,
        error: result.exitCode !== 0 ? result.stderr.slice(0, 500) : "failed to parse output",
      };
    }
  }
}
```

### `shell` runner

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
      .replace("{{prompt}}", task.prompt.replace(/"/g, '\\"'))
      .replace("{{workdir}}", opts.workdir)
      .replace("{{model}}", opts.model);

    const result = await execWithTimeout("bash", ["-c", command], {
      cwd: opts.workdir,
      timeoutMs: opts.timeoutMs,
    });

    return {
      costUsd: 0, wallTimeMs: Date.now() - startTime,
      turns: 0, inputTokens: 0, outputTokens: 0, cachedTokens: 0,
      toolCalls: 0, model: opts.model,
      error: result.exitCode !== 0 ? result.stderr.slice(0, 500) : undefined,
    };
  }
}
```

### `context` runner (IR evaluation, no LLM)

```typescript
export class ContextRunner implements Runner {
  readonly name = "context";

  async run(task: Task, opts: RunOptions): Promise<RunMetrics> {
    if (!task.contextGroundTruth) {
      return RunMetrics.empty("none");
    }

    const retrieved = await queryIndex(task.prompt, opts.workdir);
    const truth = task.contextGroundTruth;

    return {
      costUsd: 0, wallTimeMs: 0, turns: 0,
      inputTokens: 0, outputTokens: 0, cachedTokens: 0,
      toolCalls: 0, model: "none",
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
    case "claude-mp": return new ClaudeCodeRunner({ augmented: true });
    case "cursor": return new CursorRunner();
    case "aider": return new AiderRunner();
    case "codex": return new CodexRunner();
    case "shell": return new ShellRunner(opts?.shellCommand ?? "echo 'no command'");
    case "http": return new HttpRunner(opts?.httpEndpoint ?? "http://localhost:8080");
    case "context": return new ContextRunner();
    default: throw new Error(`Unknown runner: ${name}. Available: ${listRunners().join(", ")}`);
  }
}
```

### Acceptance criteria

- [ ] `mp-agent` runner uses same tools as production `mp chat`
- [ ] Tool set is configurable via `RunOptions.tools`
- [ ] `claude` runner parses JSON output, handles timeout and parse failures
- [ ] `shell` runner executes arbitrary commands with proper escaping
- [ ] All runners use shared `execWithTimeout` for subprocess management
- [ ] `RunMetrics` clearly indicates which metrics are tracked per runner
- [ ] Session persistence works for `mp-agent` and `claude-mp`

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 2.1 | `Runner` interface, `RunMetrics`, `execWithTimeout` utility | 1 day |
| 2.2 | `MpAgentRunner` (uses AgentBridge, configurable tools) | 2 days |
| 2.3 | `ClaudeCodeRunner` (vanilla + augmented, JSON parsing) | 1.5 days |
| 2.4 | `ShellRunner` + `HttpRunner` | 1 day |
| 2.5 | `CursorRunner` + `AiderRunner` + `CodexRunner` | 1.5 days |
| 2.6 | `ContextRunner` with IR metrics (recall, MRR, NDCG, hit rate) | 2 days |
| 2.7 | Runner registry + `listRunners` | 0.5 days |

---

## 3. Harness

### Single-task evaluation

```typescript
export interface EvalConfig {
  tasks: Task[];
  runners: string[];
  trials: number;                 // default 3
  model: string;
  reposDir: string;
  resultsDb: string;
  timeoutMs: number;              // default 300_000
  tools?: string[];               // tool allowlist for mp-agent
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

      1. Prepare workdir (git reset or worktree, §3.1 below)
      2. Run environment setup if task.environmentSetup exists
      3. runner.run(task, opts) → RunMetrics
      4. Run verification (task.verify) → passed
      5. Record RunResult to results DB
      6. callbacks.onTrialEnd(result)

  callbacks.onTaskEnd(task)
```

### Workspace isolation (non-Docker)

**Problem identified in gap analysis:** Shallow clones don't support
`git reset --hard <arbitrary-commit>` — the commit might not be in the
shallow history.

**Solution:** Use **git worktrees** with a shared object store:

```typescript
export async function prepareWorkdir(
  task: Task,
  reposDir: string,
  trialId: string,
): Promise<string> {
  const repoDir = path.join(reposDir, task.repo);
  const worktreeDir = path.join(reposDir, ".worktrees", `${task.id}-${trialId}`);

  // First trial for this repo: full clone (or verify existing)
  if (!existsSync(repoDir)) {
    await execWithTimeout("git", ["clone", task.repo, repoDir], {
      cwd: reposDir,
      timeoutMs: 120_000,
    });
  }

  // Create a detached worktree at the task's ref
  await execWithTimeout("git", [
    "worktree", "add", "--detach", worktreeDir, task.ref,
  ], {
    cwd: repoDir,
    timeoutMs: 30_000,
  });

  return worktreeDir;
}

export async function cleanupWorkdir(worktreeDir: string, repoDir: string): Promise<void> {
  await execWithTimeout("git", ["worktree", "remove", "--force", worktreeDir], {
    cwd: repoDir,
    timeoutMs: 10_000,
  });
}
```

**Benefits over shallow clone:**
- Full commit history available (any ref works)
- Shared object store (no duplication of git objects)
- Clean isolation per trial (each worktree is independent)
- Proper cleanup via `git worktree remove`

### Multi-session evaluation

Tests agent memory and context accumulation across a sequence of related
tasks:

```typescript
export async function runMultiSession(
  sequence: TaskSequence,
  config: EvalConfig,
  runners: Runner[],
  callbacks?: HarnessCallbacks,
): Promise<RunResult[]>;
```

### Multi-session fairness

**Problem identified in gap analysis:** Persistent runners don't get
`git reset` between tasks, but non-persistent runners do. This confounds
two variables: conversation history and working tree state.

**Solution:** Separate the two dimensions:

| Mode | Working tree | Conversation | Use case |
|------|:------------:|:------------:|----------|
| `persistent` | Accumulates changes | Maintained | Full moneypenny experience |
| `fresh-tree-persistent-context` | Reset between tasks | Maintained | Isolates context value |
| `non-persistent` | Reset between tasks | Fresh each time | Baseline/control |

The default comparison pairs `persistent` (mp-agent) against
`non-persistent` (claude) — this is intentional because it measures the
full value proposition. For scientific isolation of the context benefit,
use `fresh-tree-persistent-context`:

```typescript
export interface MultiSessionConfig extends EvalConfig {
  persistenceMode: "persistent" | "fresh-tree-persistent-context" | "non-persistent";
}
```

### SWE-bench environment setup

**Problem identified in gap analysis:** SWE-bench tasks require specific
Python versions, library versions, and per-repo environment setup.
Without this, most tasks fail at verification, not at the agent step.

**Solution:** The `EnvironmentSetup` field on tasks, combined with a
setup step in the harness:

```typescript
async function setupEnvironment(
  workdir: string,
  setup: EnvironmentSetup | undefined,
): Promise<void> {
  if (!setup) return;

  // Verify Python version if specified
  if (setup.pythonVersion) {
    const result = await execWithTimeout(
      `python${setup.pythonVersion}`, ["--version"],
      { cwd: workdir, timeoutMs: 5_000 },
    );
    if (result.exitCode !== 0) {
      throw new Error(
        `Task requires Python ${setup.pythonVersion} but it's not available. ` +
        `Install it or use --docker with an appropriate image.`
      );
    }
  }

  // Run install command
  if (setup.installCommand) {
    const result = await execWithTimeout(
      "bash", ["-c", setup.installCommand],
      { cwd: workdir, timeoutMs: 300_000, env: setup.envVars },
    );
    if (result.exitCode !== 0) {
      throw new Error(
        `Environment setup failed: ${result.stderr.slice(0, 500)}`
      );
    }
  }
}
```

For Docker mode, the `EnvironmentSetup.dockerImage` field overrides the
default image, allowing per-repo Docker images with pre-installed
dependencies:

```yaml
# SWE-bench Django task
id: django__django-16527
repo: django/django
ref: abc123def
environmentSetup:
  pythonVersion: "3.10"
  installCommand: "pip install -e .[dev]"
  dockerImage: "mp-eval-django:3.10"
```

### Acceptance criteria

- [ ] Git worktrees provide isolated workdirs for parallel trials
- [ ] Worktrees are cleaned up after trial completion (even on failure)
- [ ] `EnvironmentSetup` runs before the agent, fails fast if Python version missing
- [ ] Multi-session persistent mode accumulates working tree changes
- [ ] Multi-session `fresh-tree-persistent-context` resets tree but keeps conversation
- [ ] Harness callbacks fire at correct points (start/end of task, trial)

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 3.1 | `runEval` single-task harness with callbacks and worktree isolation | 2 days |
| 3.2 | `runMultiSession` harness with persistence modes | 2 days |
| 3.3 | Git worktree operations (create, cleanup) | 1 day |
| 3.4 | Environment setup step | 0.5 days |

---

## 4. Verification System

### VerifySpec

```typescript
export type VerifySpec =
  | { type: "command"; command: string; cwd?: string }
  | { type: "pytest"; testPath: string; args?: string[] }
  | { type: "grep-absent"; pattern: string; glob?: string }
  | { type: "composite"; checks: VerifySpec[] }
  | { type: "patch-then-test"; testPatch: string; testCommand: string };  // NEW
```

### SWE-bench test patch application

**Problem identified in gap analysis:** The `makeVerifySpec` function
didn't actually apply the test patch. In real SWE-bench evaluation, the
test patch contains **new tests** that verify the fix. These must be
applied before running the test suite — otherwise you're running the
old tests that already pass.

The new `patch-then-test` verify type handles this:

```typescript
async function verifyPatchThenTest(
  spec: { testPatch: string; testCommand: string },
  workdir: string,
  timeoutMs: number,
): Promise<VerifyResult> {
  // 1. Write the test patch to a temp file
  const patchFile = path.join(workdir, ".mp-eval-test.patch");
  await Bun.write(patchFile, spec.testPatch);

  // 2. Apply the test patch
  const applyResult = await execWithTimeout(
    "git", ["apply", "--check", patchFile],
    { cwd: workdir, timeoutMs: 10_000 },
  );

  if (applyResult.exitCode !== 0) {
    // Patch might conflict with agent's changes — try with 3-way merge
    const apply3way = await execWithTimeout(
      "git", ["apply", "--3way", patchFile],
      { cwd: workdir, timeoutMs: 10_000 },
    );
    if (apply3way.exitCode !== 0) {
      return {
        passed: false,
        output: `Test patch failed to apply: ${apply3way.stderr}`,
      };
    }
  } else {
    await execWithTimeout(
      "git", ["apply", patchFile],
      { cwd: workdir, timeoutMs: 10_000 },
    );
  }

  // 3. Run the test command
  const testResult = await execWithTimeout(
    "bash", ["-c", spec.testCommand],
    { cwd: workdir, timeoutMs },
  );

  // 4. Clean up patch file
  try { unlinkSync(patchFile); } catch {}

  return {
    passed: testResult.exitCode === 0,
    output: testResult.stdout + "\n" + testResult.stderr,
  };
}
```

### Updated `makeVerifySpec` for SWE-bench

```typescript
function makeVerifySpec(instance: SweBenchInstance): VerifySpec {
  const testFiles = extractTestFiles(instance.test_patch);
  const failToPass = instance.FAIL_TO_PASS
    ? JSON.parse(instance.FAIL_TO_PASS) as string[]
    : [];

  let testCommand: string;
  if (failToPass.length > 0) {
    testCommand = `python -m pytest ${failToPass.join(" ")} --tb=short -q`;
  } else if (testFiles.length > 0) {
    testCommand = `python -m pytest ${testFiles.join(" ")} --tb=short -q`;
  } else {
    testCommand = "python -m pytest --tb=short -q";
  }

  return {
    type: "patch-then-test",
    testPatch: instance.test_patch,
    testCommand,
  };
}
```

### Standard verification execution

```typescript
export async function runVerify(
  spec: VerifySpec,
  workdir: string,
  timeoutMs: number,
): Promise<VerifyResult> {
  switch (spec.type) {
    case "command":
      return execCommand(spec.command, spec.cwd ? path.join(workdir, spec.cwd) : workdir, timeoutMs);

    case "pytest": {
      const extra = spec.args?.join(" ") ?? "";
      return execCommand(`python -m pytest ${spec.testPath} ${extra} --tb=short -q`, workdir, timeoutMs);
    }

    case "grep-absent": {
      const glob = spec.glob ?? "**/*";
      const result = await execWithTimeout(
        "rg", ["--count", spec.pattern, "--glob", glob],
        { cwd: workdir, timeoutMs },
      );
      const count = result.stdout
        .split("\n")
        .filter(l => l.includes(":"))
        .reduce((sum, l) => sum + parseInt(l.split(":").pop() ?? "0", 10), 0);
      return { passed: count === 0, output: result.stdout };
    }

    case "composite": {
      const outputs: string[] = [];
      for (const check of spec.checks) {
        const sub = await runVerify(check, workdir, timeoutMs);
        outputs.push(sub.output);
        if (!sub.passed) return { passed: false, output: outputs.join("\n---\n") };
      }
      return { passed: true, output: outputs.join("\n---\n") };
    }

    case "patch-then-test":
      return verifyPatchThenTest(spec, workdir, timeoutMs);
  }
}
```

### Acceptance criteria

- [ ] `command` verify type runs shell commands, passes on exit code 0
- [ ] `pytest` verify type runs pytest on specified test files
- [ ] `grep-absent` verify type correctly detects (or not) patterns in files
- [ ] `composite` verify type runs checks in order, short-circuits on failure
- [ ] `patch-then-test` applies the test patch before running tests
- [ ] Test patch application falls back to 3-way merge on conflict
- [ ] All verification uses `execWithTimeout` with proper cleanup

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 4.1 | `VerifySpec` types, `runVerify` dispatcher, `execCommand` | 1 day |
| 4.2 | `patch-then-test` verify type with 3-way merge fallback | 1 day |
| 4.3 | Composite verification, grep-absent logic | 0.5 days |

---

## 5. Results Database

Results are stored in a separate SQLite database (not the intelligence
file). This keeps eval data portable and shareable.

### Schema

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
  session_number INTEGER,
  error TEXT,
  recall REAL,
  mrr REAL,
  ndcg REAL,
  hit_rate REAL,
  cost_tracked INTEGER NOT NULL DEFAULT 1,   -- NEW: was cost actually reported?
  token_tracked INTEGER NOT NULL DEFAULT 1,  -- NEW: were tokens actually reported?
  timestamp INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_results_task ON results(task_id, runner);
CREATE INDEX IF NOT EXISTS idx_results_runner ON results(runner);

CREATE TABLE IF NOT EXISTS eval_runs (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  started_at INTEGER NOT NULL,
  completed_at INTEGER,
  config TEXT NOT NULL,               -- JSON of EvalConfig
  git_sha TEXT,                       -- HEAD commit of moneypenny at eval time
  notes TEXT
);
```

### `ResultsDB` class

```typescript
export class ResultsDB {
  constructor(dbPath: string);

  insert(result: RunResult): void;
  insertBatch(results: RunResult[]): void;

  getResults(opts?: { runner?: string; taskId?: string; model?: string }): RunResult[];
  getReport(): ReportRow[];
  getMultiSessionReport(): SessionReportRow[];
  getRunners(): string[];
  getTasks(): string[];

  startEvalRun(config: EvalConfig): number;
  completeEvalRun(runId: number): void;
}
```

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 5.1 | Schema, `ResultsDB` class with insert/query | 1 day |
| 5.2 | Report queries, eval_runs tracking | 1 day |

---

## 6. SWE-bench Importer

### Environment setup per repo

**Problem identified in gap analysis:** The previous spec preserved
`environment_setup_commit` but didn't explain how to use it. The
importer now generates `EnvironmentSetup` per task:

```typescript
const REPO_ENVIRONMENTS: Record<string, Partial<EnvironmentSetup>> = {
  "django/django": {
    pythonVersion: "3.10",
    installCommand: "pip install -e .[dev]",
    dockerImage: "mp-eval-django",
  },
  "scikit-learn/scikit-learn": {
    pythonVersion: "3.10",
    installCommand: "pip install -e .[dev] numpy scipy",
    dockerImage: "mp-eval-sklearn",
  },
  "sympy/sympy": {
    pythonVersion: "3.10",
    installCommand: "pip install -e .",
  },
  "psf/requests": {
    pythonVersion: "3.10",
    installCommand: "pip install -e .[dev]",
  },
  // ... other common SWE-bench repos
};

function getEnvironmentSetup(instance: SweBenchInstance): EnvironmentSetup | undefined {
  const repoSetup = REPO_ENVIRONMENTS[instance.repo];
  if (!repoSetup) return undefined;

  return {
    ...repoSetup,
    envVars: instance.environment_setup_commit
      ? { SWE_BENCH_SETUP_COMMIT: instance.environment_setup_commit }
      : undefined,
  };
}
```

### Import process

```typescript
export interface ImportOptions {
  includeHints: boolean;
  difficultyFromPatchSize: boolean;
  maxTasks?: number;
  repos?: string[];               // filter to specific repos
}

export function importSweBench(jsonPath: string, opts: ImportOptions): Task[] {
  const instances = JSON.parse(Bun.file(jsonPath).text()) as SweBenchInstance[];

  return instances
    .filter(i => !opts.repos || opts.repos.includes(i.repo))
    .slice(0, opts.maxTasks)
    .map(instance => ({
      id: instance.instance_id,
      repo: instance.repo.split("/").pop()!,
      ref: instance.base_commit,
      prompt: opts.includeHints && instance.hints_text
        ? `${instance.problem_statement}\n\nHints:\n${instance.hints_text}`
        : instance.problem_statement,
      verify: makeVerifySpec(instance),
      difficulty: opts.difficultyFromPatchSize
        ? estimateDifficulty(instance.patch)
        : undefined,
      language: "python",
      tags: ["swe-bench", instance.version ?? "unknown"],
      environmentSetup: getEnvironmentSetup(instance),
    }));
}
```

### Acceptance criteria

- [ ] Importer parses SWE-bench JSON and produces valid Task objects
- [ ] `verify` spec uses `patch-then-test` with the actual test patch
- [ ] `environmentSetup` is populated for known repos
- [ ] Difficulty estimation works based on patch size
- [ ] `--repos` filter limits import to specific repositories
- [ ] Output YAML files are grouped by repo subdirectory

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 6.1 | JSON parser, instance → task converter with env setup | 1.5 days |
| 6.2 | Difficulty estimation, repo environment map | 0.5 days |
| 6.3 | Write YAML grouped by repo, verify spec with patch-then-test | 0.5 days |

---

## 7. Statistical Analysis

### Wilson score confidence intervals

For pass rates with small sample sizes (3–10 trials):

```typescript
export interface ConfidenceInterval {
  pointEstimate: number;
  lower: number;
  upper: number;
  n: number;
}

export function wilsonCI(
  successes: number,
  trials: number,
  confidence?: number,
): ConfidenceInterval;
```

### McNemar's test for paired comparisons

```typescript
export interface McNemarResult {
  runnerA: string;
  runnerB: string;
  aWins: number;
  bWins: number;
  bothPass: number;
  bothFail: number;
  chiSquared: number;
  pValue: number;
  significantAt05: boolean;
}

export function mcnemarTest(
  results: RunResult[],
  runnerA: string,
  runnerB: string,
): McNemarResult;
```

### Efficiency metrics

```typescript
export interface EfficiencyMetrics {
  runner: string;
  totalTasks: number;
  passed: number;
  passRate: ConfidenceInterval;
  avgCostUsd: number;
  costPerPass: number | null;
  avgWallTimeMs: number;
  avgTurns: number;
  totalInputTokens: number;
  totalOutputTokens: number;
  totalCachedTokens: number;
  cacheHitRate: number;
  cacheSavingsPct: number;
  avgToolCalls: number;
  costTracked: boolean;           // false → cost metrics are unreliable
  tokenTracked: boolean;          // false → token metrics are unreliable
  cacheTracked: boolean;          // false → cache metrics are unreliable
}

export function computeEfficiency(runnerName: string, results: RunResult[]): EfficiencyMetrics;
```

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 7.1 | Wilson CI, z-score approximation | 0.5 days |
| 7.2 | McNemar's test with continuity correction | 1 day |
| 7.3 | Efficiency metrics with tracking flags | 0.5 days |
| 7.4 | `compareRunners` full A/B comparison | 0.5 days |
| 7.5 | Unit tests | 0.5 days |

---

## 8. Reporting

### Output formats

| Format | Description | Use case |
|--------|------------|----------|
| `table` | ASCII table | Terminal |
| `comparison` | Side-by-side with stats | A/B analysis |
| `leaderboard` | Ranked runners | Ranking |
| `json` | Machine-readable | CI |
| `html` | Rich HTML report | Sharing |
| `stats` | Statistical analysis | Research |

### Table format

```
┌──────────────────┬─────────┬──────────┬──────────┬──────────┬────────┐
│ Task             │ Runner  │ Pass Rate│ Avg Cost │ Avg Time │ Trials │
├──────────────────┼─────────┼──────────┼──────────┼──────────┼────────┤
│ fix-null-check   │ mp-agent│ 100%     │ $0.023   │ 12.3s    │ 3      │
│ fix-null-check   │ claude  │ 67%      │ $0.031   │ 18.7s    │ 3      │
│ fix-null-check   │ aider   │ 33%      │    —     │ 22.1s    │ 3      │
└──────────────────┴─────────┴──────────┴──────────┴──────────┴────────┘
                                         ^ "—" for untracked metrics
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
Cache hit rate:    42.3%         — (not tracked)
Cache savings:     38.1%         — (not tracked)
───────────────────────────────────────────────────────────────
McNemar χ²:        4.17          p = 0.041  *significant*
  mp-agent wins:   8 tasks
  claude wins:     2 tasks
  Both pass:       12 tasks
  Both fail:       3 tasks
═══════════════════════════════════════════════════════════════
```

### HTML report

A self-contained HTML file with:
- Summary table
- Per-task pass/fail heatmap
- Cost and timing charts (using embedded Chart.js CDN or inline)
- Statistical comparison results
- Multi-session progression charts (if applicable)
- Filterable by difficulty, language, tags
- Clear labels for untracked metrics

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 8.1 | Table formatter (ASCII) | 1 day |
| 8.2 | Comparison formatter (side-by-side with stats) | 1 day |
| 8.3 | Leaderboard + JSON output | 0.5 days |
| 8.4 | HTML report (self-contained, with charts) | 2 days |
| 8.5 | Multi-session reporting | 1 day |

---

## 9. Docker Isolation

### DockerConfig

```typescript
export interface DockerConfig {
  image: string;                  // default: "mp-eval-sandbox"
  timeoutMs: number;
  mountRepos: boolean;
  networkAccess: boolean;         // default: false
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
): Promise<ExecResult>;

export async function checkDocker(config: DockerConfig): Promise<void>;
```

### Per-repo Docker images for SWE-bench

The `EnvironmentSetup.dockerImage` field allows per-repo Docker images
with pre-installed dependencies:

```bash
# Generate per-repo Dockerfiles
mp eval setup --dockerfile --repos django scikit-learn

# Produces:
# eval/docker/Dockerfile.django    (Python 3.10 + Django dev deps)
# eval/docker/Dockerfile.sklearn   (Python 3.10 + numpy + scipy)
# eval/docker/Dockerfile.base      (generic Python + Node + Bun)
```

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 9.1 | `DockerConfig`, `checkDocker`, `runInContainer` | 1 day |
| 9.2 | Dockerfile generation (base + per-repo) | 1 day |
| 9.3 | Integration with harness (--docker flag) | 0.5 days |

---

## 10. Parallel Execution

### Bounded-concurrency executor

```typescript
export async function runParallel<T>(
  tasks: Array<() => Promise<T>>,
  parallelism: number,
): Promise<T[]>;
```

When Docker is enabled, each parallel trial gets its own container.
Without Docker, each trial gets its own git worktree (§3).

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 10.1 | `runParallel` executor | 0.5 days |
| 10.2 | Worktree-per-trial isolation | 0.5 days |
| 10.3 | Integration with harness and Docker | 0.5 days |

---

## 11. CI Regression Gate

### Baseline management

**Problem identified in gap analysis:** Where does `baseline.sqlite` come
from?

**Lifecycle:**

1. **Create baseline:** Run eval on `main` branch, copy results to
   `eval/baseline.sqlite`, commit to repo.
2. **Update baseline:** After intentional improvements, re-run eval on
   `main`, replace `eval/baseline.sqlite`.
3. **CI comparison:** PR branch runs eval, compares against committed
   baseline.

```bash
# Create/update baseline (run on main branch)
mp eval run --tasks ./eval/tasks --runners mp-agent --trials 5 --results-db ./eval/baseline.sqlite
git add eval/baseline.sqlite
git commit -m "Update eval baseline"

# CI compares PR results against committed baseline
mp eval ci --baseline ./eval/baseline.sqlite --current ./results.sqlite --threshold 0.05
```

The baseline is a committed artifact — versioned, reviewable, and
reproducible.

### Gate logic

```typescript
export interface CiGateConfig {
  baselinePath: string;
  currentPath: string;
  threshold: number;              // default 0.05 (5%)
}

export interface CiGateResult {
  passed: boolean;
  regressions: Regression[];
  improvements: Improvement[];
  unchanged: string[];
}

export function runCiGate(config: CiGateConfig): CiGateResult;
```

Exit code: 0 on pass, 1 on regression.

### GitHub Actions integration

```yaml
name: Agent Eval
on:
  pull_request:
    paths:
      - "packages/loop/**"
      - "packages/ctx/**"
      - "packages/tools/**"

jobs:
  eval:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: oven-sh/setup-bun@v2
      - run: bun install

      - name: Setup eval repos
        run: bun run mp eval setup --repos-dir ./repos

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

### Acceptance criteria

- [ ] `baseline.sqlite` is a committed artifact in the repo
- [ ] `mp eval ci` compares current results against baseline
- [ ] Regressions exceeding threshold cause exit code 1
- [ ] Improvements are reported but don't affect exit code
- [ ] GitHub Actions workflow template is functional

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 11.1 | `runCiGate` comparison logic | 1 day |
| 11.2 | Terminal output formatting | 0.5 days |
| 11.3 | GitHub Actions workflow template | 0.5 days |

---

## 12. Sample Task Suite

**Problem identified in gap analysis:** No sample tasks exist. A
"batteries included" suite against moneypenny's own codebase makes the
harness immediately useful.

### Tasks

Create 8 tasks against the moneypenny codebase covering different
difficulty levels:

```
eval/tasks/moneypenny/
├── easy/
│   ├── fix-typo-in-help.yaml         # Fix a CLI help text typo
│   ├── add-missing-export.yaml       # Add a missing export to index.ts
│   └── update-cost-table.yaml        # Add a new model to cost.ts
├── medium/
│   ├── add-tool-param.yaml           # Add a parameter to an existing tool
│   ├── fix-session-resume.yaml       # Fix a bug in session loading
│   └── add-config-validation.yaml    # Add validation for a config field
└── hard/
    ├── add-new-tool.yaml             # Implement a new tool from scratch
    └── refactor-search.yaml          # Refactor search to support new surface
```

Each task includes:
- A clear prompt that a coding agent can act on
- A `verify` spec (usually `command` type running existing tests)
- `ref` pointing to a specific commit where the task makes sense
- `difficulty` and `tags`

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 12.1 | Write 8 sample task YAML files | 1 day |
| 12.2 | Create baseline by running mp-agent against sample tasks | 0.5 days |

---

## 13. CLI Integration

### `mp eval` subcommands

```
mp eval run          Run end-to-end evaluation trials
mp eval sequence     Run a multi-session sequence evaluation
mp eval context      Run context-quality evaluation (no LLM)
mp eval report       Generate evaluation reports
mp eval import       Import tasks from SWE-bench JSON dataset
mp eval setup        Clone repos, build Docker images, verify environment
mp eval list-runners List available runners with capability info
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
  --timeout <ms>         Timeout per trial in ms (default: 300000)
  --filter <regex>       Only run tasks matching this regex
  --difficulty <level>   Only run tasks with this difficulty
  --parallelism <n>      Number of parallel trials (default: 1)
  --docker               Run each trial in a Docker container
  --docker-image <name>  Docker image (default: mp-eval-sandbox)
  --tools <names...>     Tool allowlist for mp-agent runner
```

### `mp eval list-runners`

Shows capability info per runner:

```
Available Runners
┌────────────┬───────────────┬───────┬────────┬───────┐
│ Runner     │ Persistence   │ Cost  │ Tokens │ Cache │
├────────────┼───────────────┼───────┼────────┼───────┤
│ mp-agent   │ Yes           │ ✓     │ ✓      │ ✓     │
│ claude     │ No            │ ✓     │ ✓      │ ✗     │
│ claude-mp  │ Yes           │ ✓     │ ✓      │ ✗     │
│ cursor     │ No            │ ✗     │ ✗      │ ✗     │
│ aider      │ No            │ ✗     │ ✗      │ ✗     │
│ codex      │ No            │ ~     │ ~      │ ✗     │
│ shell      │ No            │ ✗     │ ✗      │ ✗     │
│ http       │ No            │ ?     │ ?      │ ?     │
│ context    │ N/A           │ N/A   │ N/A    │ N/A   │
└────────────┴───────────────┴───────┴────────┴───────┘
✓ = fully tracked  ~ = partial  ✗ = not available  ? = depends on endpoint
```

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 13.1 | CLI arg parsing (all subcommands) | 1.5 days |
| 13.2 | Wire subcommands to harness/reporter/importer | 1 day |
| 13.3 | `list-runners` with capability table | 0.5 days |
| 13.4 | Makefile targets + documentation | 0.5 days |

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
