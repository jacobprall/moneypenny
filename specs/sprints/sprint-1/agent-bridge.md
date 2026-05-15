# AgentBridge Event Protocol

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
