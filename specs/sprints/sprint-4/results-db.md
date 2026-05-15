# Results Database

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
