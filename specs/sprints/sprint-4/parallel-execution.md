# Parallel Execution

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
