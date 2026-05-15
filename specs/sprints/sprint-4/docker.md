# Docker Isolation

### DockerConfig

```typescript
export interface DockerConfig {
  image: string;                  // default: "mp-eval-sandbox"
  timeoutMs: number;
  mountRepos: boolean;
  networkAccess: boolean;         // default: false
  memoryLimit: string;            // default: "4g"
  cpuLimit: string;               // default: "2"
}
```

### Container execution

```typescript
export async function runInContainer(
  config: DockerConfig,
  workdir: string,
  command: string,
): Promise<ExecResult>;

export async function checkDocker(config: DockerConfig): Promise<void>;
```

### Per-repo Docker images for SWE-bench

The `EnvironmentSetup.dockerImage` field allows per-repo Docker images
with pre-installed dependencies:

```bash
# Generate per-repo Dockerfiles
mp eval setup --dockerfile --repos django scikit-learn

# Produces:
# eval/docker/Dockerfile.django    (Python 3.10 + Django dev deps)
# eval/docker/Dockerfile.sklearn   (Python 3.10 + numpy + scipy)
# eval/docker/Dockerfile.base      (generic Python + Node + Bun)
```

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 9.1 | `DockerConfig`, `checkDocker`, `runInContainer` | 1 day |
| 9.2 | Dockerfile generation (base + per-repo) | 1 day |
| 9.3 | Integration with harness (--docker flag) | 0.5 days |
