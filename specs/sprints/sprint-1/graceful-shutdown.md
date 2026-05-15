# Graceful Shutdown

### Problem

`mp serve` runs HTTP server, WebSocket connections, file watcher, scheduler,
event bus (sprint 3), and potentially active agent sessions. An ungraceful
kill can lose deferred writes, leave stale WebSocket connections, or
corrupt in-progress job runs.

### Design

```typescript
export interface ShutdownManager {
  register(name: string, handler: () => Promise<void>, priority: number): void;
  shutdown(reason: string): Promise<void>;
}
```

Shutdown order (by priority, highest first):

| Priority | Component | Action |
|----------|-----------|--------|
| 100 | Active agent sessions | Signal abort, wait up to 5s for current LLM call to complete |
| 90 | WebSocket connections | Send close frame with "going away" (1001), wait 1s |
| 80 | HTTP server | Stop accepting new connections, drain in-flight requests (5s) |
| 70 | File watcher | Stop watching |
| 60 | Scheduler | Cancel next tick timer |
| 50 | Channel adapters (sprint 3) | Stop polling/listening |
| 40 | Event bus (sprint 3) | Drain pending async handlers (5s timeout) |
| 30 | DbWriter | Flush deferred write queue |
| 20 | DbReadPool | Close read connections |
| 10 | Write connection | Close |

`mp serve` registers a `SIGTERM` and `SIGINT` handler that calls
`shutdownManager.shutdown()`. Total shutdown budget: 15 seconds. If
any component exceeds its timeout, it's force-killed and logged.

### Acceptance criteria

- [ ] `Ctrl+C` on `mp serve` flushes all deferred writes before exit
- [ ] Active WebSocket clients receive close frame before disconnect
- [ ] In-progress agent sessions complete current LLM call or abort within 5s
- [ ] Stale job_runs are marked `failed` with error "server shutdown"
- [ ] Process exits with code 0 on clean shutdown, 1 on timeout

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 10.1 | `ShutdownManager` class with priority-ordered handlers | 0.5 days |
| 10.2 | Wire all `mp serve` components into shutdown manager | 1 day |
