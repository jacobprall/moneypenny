# SWE-bench Importer

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
