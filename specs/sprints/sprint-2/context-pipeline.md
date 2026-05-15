# Context Pipeline

> Replaces the original "Unified Query Engine" spec with a composable
> pipeline architecture. The retrieval goals are the same — multi-surface
> search, RRF ranking, per-surface token budgets — but the implementation
> is structured as a pipeline of independent, testable stages rather than
> a monolithic class.

### Problem

Searching across moneypenny's knowledge surfaces requires calling
different functions in different packages. The agent's `code_search`
tool only reaches code chunks. A developer asking "what do I know about
authentication?" should get results from code, memories, skills, and
sessions.

The original spec proposed a `UnifiedQuery` class that hardcodes four
surfaces, bakes in RRF ranking, and owns the budget logic. That design
works but is difficult to extend: adding a new surface means modifying
the class internals, swapping the ranking strategy requires forking the
class, and testing any individual concern requires instantiating the
whole thing.

### Design: composable pipeline

A context pipeline is a sequence of **stages**. Each stage receives a
`ContextFrame` and returns a (possibly modified) `ContextFrame`. Stages
are composed via a `pipeline()` function that runs them in order.

```typescript
// @moneypenny/ctx

export interface ContextFrame {
  /** The user's query / current turn text. */
  query: string;

  /** Accumulated retrieval results across all gather stages. */
  results: ScoredResult[];

  /** Token budget tracking — stages decrement as they consume. */
  budget: TokenBudget;

  /** Assembled system prompt blocks (populated by format stages). */
  system: ContentBlock[];

  /** Conversation messages (populated by conversation stage). */
  messages: Message[];

  /** Arbitrary metadata for inter-stage communication. */
  metadata: Map<string, unknown>;
}

export interface ScoredResult {
  surface: string;
  content: string;
  score: number;
  metadata: Record<string, unknown>;
}

export interface TokenBudget {
  total: number;
  reserved: number;          // system prompt + conversation estimate
  consumed: number;
  remaining(): number;       // total - reserved - consumed
}

export type ContextStage = (frame: ContextFrame) => Promise<ContextFrame>;
```

A pipeline is just an array of stages applied in sequence:

```typescript
export function pipeline(stages: ContextStage[]): ContextStage {
  return async (frame: ContextFrame) => {
    let current = frame;
    for (const stage of stages) {
      current = await stage(current);
    }
    return current;
  };
}
```

No middleware / `next()` pattern needed — stages are pure transforms on
the frame. If a stage needs to short-circuit (e.g., governance denial),
it throws a typed error that the pipeline runner catches.

---

## Core abstractions

### `ContextProvider` — pluggable retrieval surfaces

Each knowledge surface implements a simple provider interface:

```typescript
export interface ContextProvider {
  readonly surface: string;
  search(query: string, limit: number): Promise<ScoredResult[]>;
}
```

Providers are registered, not hardcoded:

```typescript
export function gather(provider: ContextProvider, limit?: number): ContextStage {
  return async (frame) => {
    const results = await provider.search(frame.query, limit ?? 10);
    return {
      ...frame,
      results: [...frame.results, ...results],
    };
  };
}
```

### Built-in providers

| Provider | Surface name | Source DB | Search method |
|----------|-------------|----------|---------------|
| `CodeProvider` | `"code"` | workspace.db | BM25 + vector hybrid (existing `hybridSearch`) |
| `MemoryProvider` | `"memory"` | session (mp.db) | FTS5 on `knowledge_fts` + vector |
| `SkillProvider` | `"skill"` | session (mp.db) | FTS5 on `skills_fts` |
| `SessionProvider` | `"session"` | session (mp.db) | FTS5 + vector on `session_summaries_fts` |

Each provider is constructed with its database handle(s):

```typescript
export function createCodeProvider(
  workspaceReaders: DbReadPool | null,
): ContextProvider | null {
  if (!workspaceReaders) return null;
  return {
    surface: "code",
    async search(query, limit) {
      return workspaceReaders.read((db) => {
        const raw = hybridSearch(db, query, { limit });
        return raw.map((r) => ({
          surface: "code",
          content: r.content,
          score: r.score,
          metadata: { filePath: r.filePath, startLine: r.startLine },
        }));
      });
    },
  };
}

export function createMemoryProvider(
  sessionReaders: DbReadPool,
): ContextProvider {
  return {
    surface: "memory",
    async search(query, limit) {
      return sessionReaders.read((db) => {
        const rows = db.prepare(`
          SELECT k.rowid, k.content, k.context, kf.rank
          FROM knowledge_fts kf
          JOIN knowledge k ON k.rowid = kf.rowid
          WHERE knowledge_fts MATCH ?
          ORDER BY kf.rank
          LIMIT ?
        `).all(query, limit);
        return rows.map((r, i) => ({
          surface: "memory",
          content: r.content,
          score: -r.rank,   // FTS5 rank is negative (lower = better)
          metadata: { context: r.context },
        }));
      });
    },
  };
}
```

### Two-database challenge

Moneypenny has two databases:

- **Session DB** (`mp.db`): sessions, messages, agents, skills, knowledge,
  policies, gov_events, config
- **Workspace DB** (`workspace.db`): code_chunks, code_fts, file_tree

Providers handle this transparently — each is constructed with the
appropriate read pool. The pipeline doesn't need to know which DB a
provider uses. If the workspace DB doesn't exist (user hasn't run
`mp index`), `createCodeProvider` returns `null` and is simply not
added to the pipeline.

---

## Built-in stages

### `gather` — retrieval

Calls a `ContextProvider` and appends results to the frame. One gather
stage per surface.

```typescript
export function gather(provider: ContextProvider, limit?: number): ContextStage;
```

### `rank` — re-scoring

Takes all accumulated results and re-scores them using a ranking strategy.
The default is Reciprocal Rank Fusion, matching the existing `hybridSearch`
approach:

```typescript
export interface RankingStrategy {
  rank(results: ScoredResult[]): ScoredResult[];
}

export function rank(strategy: RankingStrategy): ContextStage {
  return async (frame) => ({
    ...frame,
    results: strategy.rank(frame.results),
  });
}
```

#### Built-in strategies

```typescript
export function rrf(k = 60): RankingStrategy {
  return {
    rank(results) {
      const bySurface = groupBy(results, (r) => r.surface);
      const scored: ScoredResult[] = [];

      for (const [surface, surfaceResults] of bySurface) {
        const sorted = [...surfaceResults].sort((a, b) => b.score - a.score);
        sorted.forEach((r, i) => {
          scored.push({ ...r, score: 1 / (k + i + 1) });
        });
      }

      return scored.sort((a, b) => b.score - a.score);
    },
  };
}

export function passthrough(): RankingStrategy {
  return { rank: (results) => results };
}
```

### `budget` — token trimming

Enforces per-surface token budgets. Surfaces are trimmed in priority
order (lowest priority first):

```typescript
export interface BudgetConfig {
  surfacePriority: string[];    // highest priority first
  // e.g., ["code", "memory", "skill", "session"]
}

export function budget(config: BudgetConfig): ContextStage {
  return async (frame) => {
    const remaining = frame.budget.remaining();
    if (remaining <= 0) return { ...frame, results: [] };

    const bySurface = groupBy(frame.results, (r) => r.surface);
    const prioritized = config.surfacePriority;
    const kept: ScoredResult[] = [];
    let used = 0;

    // Allocate from highest to lowest priority
    for (const surface of prioritized) {
      const surfaceResults = bySurface.get(surface) ?? [];
      for (const result of surfaceResults) {
        const tokens = estimateTokens(result.content);
        if (used + tokens > remaining) break;
        kept.push(result);
        used += tokens;
      }
    }

    return {
      ...frame,
      results: kept,
      budget: { ...frame.budget, consumed: frame.budget.consumed + used },
    };
  };
}
```

### `format` — render results into system prompt blocks

Converts the budgeted results into `ContentBlock[]` for the system prompt:

```typescript
export function format(
  formatter?: (results: ScoredResult[]) => ContentBlock[],
): ContextStage {
  return async (frame) => {
    const fn = formatter ?? defaultFormatter;
    const blocks = fn(frame.results);
    return {
      ...frame,
      system: [...frame.system, ...blocks],
    };
  };
}

function defaultFormatter(results: ScoredResult[]): ContentBlock[] {
  const bySurface = groupBy(results, (r) => r.surface);
  const blocks: ContentBlock[] = [];

  for (const [surface, surfaceResults] of bySurface) {
    const header = surfaceLabel(surface);
    const body = surfaceResults.map((r) => r.content).join("\n\n");
    blocks.push({ type: "text", text: `## ${header}\n\n${body}` });
  }

  return blocks;
}
```

### `conversation` — load message history

Loads the conversation into the frame. Separated from retrieval so it
can be positioned independently in the pipeline:

```typescript
export function conversation(
  resolver: ConversationResolver,
): ContextStage {
  return async (frame) => {
    const messages = await resolver(frame);
    return { ...frame, messages };
  };
}
```

### `deduplicate` — optional cross-surface dedup

Removes near-duplicate results across surfaces (e.g., a memory that
quotes code verbatim):

```typescript
export function deduplicate(
  similarity?: (a: ScoredResult, b: ScoredResult) => number,
  threshold = 0.85,
): ContextStage {
  return async (frame) => {
    const kept: ScoredResult[] = [];
    for (const result of frame.results) {
      const isDupe = kept.some(
        (existing) => (similarity ?? jaccardSimilarity)(existing, result) > threshold
      );
      if (!isDupe) kept.push(result);
    }
    return { ...frame, results: kept };
  };
}
```

---

## Composing pipelines

### Default pipeline (matches today's behavior)

The simplest pipeline — just code search, no multi-surface:

```typescript
const defaultPipeline = pipeline([
  gather(codeProvider),
  rank(rrf()),
  budget({ surfacePriority: ["code"] }),
  format(),
  conversation(loadConversation),
]);
```

### Full intelligence pipeline

The target state for sprint 2:

```typescript
const intelligencePipeline = pipeline([
  gather(codeProvider, 15),
  gather(memoryProvider, 10),
  gather(skillProvider, 5),
  gather(sessionProvider, 5),
  deduplicate(),
  rank(rrf({ k: 60 })),
  budget({
    surfacePriority: ["code", "memory", "skill", "session"],
  }),
  format(),
  conversation(loadConversation),
]);
```

### Custom user pipeline

Users could compose their own via blueprint config in the future:

```yaml
context:
  surfaces:
    - code: { limit: 20 }
    - memory: { limit: 10 }
  ranking: rrf
  budget:
    total_tokens: 8000
    priority: [code, memory]
```

---

## Integration with existing `definePrompt`

The pipeline doesn't replace `definePrompt` — it replaces the *retrieval
and assembly* that currently lives inside prompt sections. A pipeline
becomes a dynamic section:

```typescript
function createContextSection(
  pipeline: ContextStage,
  providers: { session: DbReadPool; workspace: DbReadPool | null },
): Section {
  return {
    name: "context",
    placement: "dynamic",
    priority: 50,
    resolve: async (db, context) => {
      const frame = createEmptyFrame({
        query: context.searchQuery ?? "",
        budget: {
          total: 8000,
          reserved: 3000,
          consumed: 0,
          remaining() { return this.total - this.reserved - this.consumed; },
        },
      });

      const result = await pipeline(frame);
      return result.system;
    },
  };
}
```

Existing static sections (system prompt, skill catalog) continue to work
unchanged. The pipeline just provides the retrieval-powered dynamic
section.

---

## Tool integration

The existing `code_search` tool gains an optional `surfaces` parameter:

```typescript
parameters: z.object({
  query: z.string(),
  surfaces: z.array(z.enum(["code", "memory", "skill", "session"]))
    .optional()
    .describe("Knowledge surfaces to search. Default: code only."),
  limit: z.number().optional(),
})
```

Default behavior (code only) is preserved for backward compatibility.
When `surfaces` is specified, the tool constructs a mini-pipeline with
the requested gather stages:

```typescript
async execute(params, ctx) {
  const surfaces = params.surfaces ?? ["code"];
  const stages: ContextStage[] = [];

  for (const surface of surfaces) {
    const provider = ctx.providers.get(surface);
    if (provider) stages.push(gather(provider, params.limit));
  }
  stages.push(rank(rrf()));

  const frame = createEmptyFrame({ query: params.query, ... });
  const result = await pipeline(stages)(frame);
  return formatToolResults(result.results);
}
```

---

## FTS indexes (new, required by providers)

```sql
-- Added in schema migration v11 (sprint 2)
CREATE VIRTUAL TABLE IF NOT EXISTS knowledge_fts USING fts5(
  content, context,
  content='knowledge', content_rowid='rowid',
  tokenize='porter unicode61'
);

CREATE VIRTUAL TABLE IF NOT EXISTS skills_fts USING fts5(
  name, description, instructions,
  content='skills', content_rowid='rowid',
  tokenize='porter unicode61'
);

CREATE VIRTUAL TABLE IF NOT EXISTS session_summaries_fts USING fts5(
  summary,
  content='session_summaries', content_rowid='rowid',
  tokenize='porter unicode61'
);

-- Sync triggers (see schema-v11.md for full trigger definitions)
```

---

## Why this architecture

| Concern | Monolithic `UnifiedQuery` | Composable pipeline |
|---------|--------------------------|-------------------|
| Add a surface | Modify class internals | Add a `gather()` call |
| Swap ranking | Fork the class or add a flag | Replace `rank(rrf())` with `rank(other())` |
| Test retrieval | Instantiate full class + both DBs | Unit test the provider in isolation |
| Test ranking | Can't — baked into retrieval | `rrf().rank([...testData])` |
| Test budgeting | Can't — coupled to ranking output | `budget(config)(testFrame)` |
| Governance | Bolted on externally | Insert a governance stage anywhere |
| Per-blueprint config | Constructor options | Different pipeline composition per blueprint |
| Debugging | Log inside class methods | Wrap any stage with a `tap()` logger |

---

## Acceptance criteria

- [ ] `ContextFrame`, `ContextStage`, `ContextProvider` types are exported from `@moneypenny/ctx`
- [ ] `pipeline()` composes stages correctly (order preserved, frame threaded)
- [ ] `CodeProvider` returns results from workspace DB via `hybridSearch`
- [ ] `MemoryProvider` returns results from `knowledge_fts`
- [ ] `SkillProvider` returns results from `skills_fts`
- [ ] `SessionProvider` returns results from `session_summaries_fts`
- [ ] Missing workspace DB (no `mp index` run) doesn't crash — code provider is `null`, skipped
- [ ] `rrf()` ranking produces correct cross-surface scores
- [ ] `budget()` trims lowest-priority surfaces first
- [ ] Code surface is never crowded out by lower-priority surfaces
- [ ] `deduplicate()` removes near-identical results across surfaces
- [ ] `code_search` tool with no `surfaces` param returns code results only (backward compatible)
- [ ] `code_search` tool with `surfaces: ["code", "memory"]` returns cross-surface results
- [ ] Pipeline integrates with existing `definePrompt` as a dynamic section
- [ ] Each stage is independently unit-testable

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 3.1 | `ContextFrame`, `ContextStage`, `pipeline()`, `TokenBudget` types and runner | 1 day |
| 3.2 | `ContextProvider` interface + `CodeProvider` (wraps existing `hybridSearch`) | 1 day |
| 3.3 | `MemoryProvider`, `SkillProvider`, `SessionProvider` + FTS indexes | 1.5 days |
| 3.4 | `rank()` stage with `rrf()` and `passthrough()` strategies | 0.5 days |
| 3.5 | `budget()` stage with per-surface priority trimming | 0.5 days |
| 3.6 | `format()` stage + `conversation()` stage + `deduplicate()` stage | 0.5 days |
| 3.7 | Integration: pipeline as `definePrompt` dynamic section | 0.5 days |
| 3.8 | Extend `code_search` tool with `surfaces` parameter | 0.5 days |
| 3.9 | Unit tests for each stage in isolation + integration test for full pipeline | 1 day |
