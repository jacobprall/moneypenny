# Harness

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
