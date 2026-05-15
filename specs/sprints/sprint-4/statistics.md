# Statistical Analysis

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
