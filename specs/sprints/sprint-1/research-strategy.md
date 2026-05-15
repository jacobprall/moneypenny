# Research Iteration Strategy

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
