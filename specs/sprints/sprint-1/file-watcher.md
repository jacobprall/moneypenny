# File Watcher (Extended)

### What exists today

`@moneypenny/agents/loader.ts` exports `startWatcher()` which watches
`.mp/agents/` for blueprint changes via chokidar. This sprint extends
the scope to source files, policies, skills, and gitignore changes.

### Design

New package `@moneypenny/watch` that coordinates multiple watchers:

```typescript
export interface WatcherConfig {
  repoPath: string;
  db: AgentDB;
  debounceMs?: number;         // default 300
  excludePatterns?: string[];  // merged with .gitignore + .mpignore
}

export function startWatcher(config: WatcherConfig): WatcherHandle;

export interface WatcherHandle {
  stop(): void;
  stats(): WatcherStats;
}
```

### Watcher backend

Use Bun's built-in `fs.watch` (recursive mode) with a debounce layer.
The existing chokidar-based blueprint watcher in `loader.ts` is replaced
by a handler registered with the new unified watcher.

**Debounce strategy:** Per-file 300ms debounce window. Batch operations
(git checkout touching 50 files) are detected by counting events within
a 100ms burst window; if > 10 files change within 100ms, batch them into
a single re-index call rather than 50 individual operations.

### Event routing

| Path pattern | Handler | Action |
|-------------|---------|--------|
| Source files (configured extensions) | `@moneypenny/search` indexer | Incremental re-chunk + re-embed via `reindexFile()` |
| `.mp/policies/*.yaml` | Policy sync | Re-parse, sync to `policies` table |
| `.mp/agents/*.md` | Blueprint loader | Re-parse frontmatter, upsert `agents` table |
| `.mp/skills/**/*.md` | Skill scanner | Re-scan via `scanSkillDirs()` |
| `.mp/jobs/*.yaml` | Job loader | Re-parse, sync to `jobs` table |
| `.gitignore`, `.mpignore` | Watcher itself | Re-compute exclude patterns, re-filter watch list |
| File deletions | Indexer | Mark chunks as stale (don't delete — custodian handles) |

### Acceptance criteria

- [ ] `mp serve` starts watcher automatically; `--no-watch` disables
- [ ] Source file save triggers re-index within 1s (after debounce)
- [ ] Git checkout (50+ files) batches into single re-index operation
- [ ] Blueprint change triggers agent table upsert within 500ms
- [ ] Policy YAML change takes effect on next tool call (no restart)
- [ ] `GET /api/v1/observe/watcher` returns `WatcherStats`
- [ ] Watcher ignores files matching `.gitignore` + `.mpignore`

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 3.1 | Watcher core: `fs.watch` + debounce + batch detection + exclude filtering | 1.5 days |
| 3.2 | Source file handler: incremental re-index via `reindexFile()` | 1 day |
| 3.3 | Policy/agent/skill/job handlers: re-parse and sync | 1 day |
| 3.4 | Wire into `mp serve`, stats endpoint, `--no-watch` flag | 0.5 days |
