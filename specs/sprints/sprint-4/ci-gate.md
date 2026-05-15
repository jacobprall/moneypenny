# CI Regression Gate

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
