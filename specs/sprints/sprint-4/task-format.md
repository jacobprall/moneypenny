# Task Format and Loading

### Task definition

```typescript
export interface Task {
  id: string;
  repo: string;
  ref: string;                    // git ref to reset to (default: "HEAD")
  prompt: string;
  verify?: VerifySpec;
  contextGroundTruth?: RelevantDoc[];
  description?: string;
  tags?: string[];
  difficulty?: "easy" | "medium" | "hard";
  language?: string;
  timeoutMs?: number;
  environmentSetup?: EnvironmentSetup;  // NEW: per-task environment requirements
}

export interface TaskSequence {
  repo: string;
  ref: string;
  tasks: Task[];
}

export interface EnvironmentSetup {
  pythonVersion?: string;         // e.g., "3.10"
  nodeVersion?: string;           // e.g., "22"
  installCommand?: string;        // e.g., "pip install -e .[dev]"
  envVars?: Record<string, string>;
  dockerImage?: string;           // override default Docker image
}
```

### Task file formats

A task file can contain a single task, an array of tasks, or a
`{ tasks: [...] }` wrapper. All three are auto-detected during loading.

### Loading

```typescript
export function loadTasks(dir: string): Task[];
export function loadSequence(path: string): TaskSequence;
```

`loadTasks` recursively walks a directory for `*.yaml`, `*.yml`, and
`*.json` files. Each file is parsed and normalized (default `ref` to
`"HEAD"`, inherit `repo` from parent directory name if not specified).

### Example task

```yaml
id: fix-null-check
repo: my-app
ref: abc123
prompt: |
  The `getUserName` function in `src/users.ts` throws a TypeError when the
  user object is null. Fix the null check and add a test.
verify:
  type: command
  command: "npx jest src/users.test.ts --no-coverage"
difficulty: easy
language: typescript
tags:
  - bug-fix
  - null-safety
```

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 1.1 | `Task`, `TaskSequence`, `EnvironmentSetup` types, YAML/JSON parser | 1 day |
| 1.2 | Recursive directory walker, normalization, filtering | 0.5 days |
