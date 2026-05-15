# Runner Interface

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
