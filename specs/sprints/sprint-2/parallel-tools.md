# Parallel Tool Execution

### Problem

The loop already supports `parallelToolExecution` as a config flag, and
the existing code uses `Promise.allSettled` for parallel tools. But the
implementation is incomplete: audit events, cost metrics, and governance
decisions all write to the DB, and without proper routing through the
writer, parallel writes can interleave.

### What exists

The loop's tool executor already fans out via `Promise.allSettled` when
`parallelToolExecution` is true. The `DbWriter.exclusive()` serializes
writes. What's missing is categorizing every write in the tool execution
path and ensuring non-critical writes use `defer()`.

### Remaining work

Categorize all writes in the tool execution path:

| Write | Current path | Should be |
|-------|-------------|-----------|
| `appendMessage` (tool result) | `writer.exclusive()` | `writer.exclusive()` (read-your-writes needed) |
| `appendEvent` (tool.called, tool.complete) | `writer.exclusive()` | `writer.defer()` (not read back in loop) |
| `recordTurnMetrics` | `writer.exclusive()` | `writer.defer()` (metrics, not read in loop) |
| `insertGovEvent` | `writer.exclusive()` | `writer.defer()` (audit trail) |
| `updateLastActivity` | `writer.exclusive()` | `writer.defer()` (timestamp) |
| `tool_cache` writes | `writer.exclusive()` | `writer.defer()` (cache, not critical) |
| Search queries (code_search tool) | `writer.exclusive()` on same handle | `readers.read()` (read-only) |
| `memory_add` | `writer.exclusive()` | `writer.exclusive()` (must persist) |

After categorization, parallel tool execution becomes safe: critical
writes go through `exclusive()` (serialized), non-critical writes go
through `defer()` (batched), and reads go through `readers.read()`
(concurrent).

### Acceptance criteria

- [ ] 3 independent read-only tool calls (e.g., 3x `code_search`) run concurrently
- [ ] Tool results arrive in correct order in the message history
- [ ] Governance events are recorded for all parallel tool calls
- [ ] Cost metrics are accurate after parallel execution
- [ ] No `SQLITE_BUSY` errors during parallel tool execution
- [ ] Deferred writes flush within 100ms of batch threshold

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 2.1 | Audit all tool executor writes, categorize as exclusive/defer/read | 1 day |
| 2.2 | Refactor tool executor to use correct write path per category | 1.5 days |
| 2.3 | Route search/read-only tool queries through `readers.read()` | 0.5 days |
| 2.4 | Integration tests: parallel tools, write ordering, busy resilience | 1 day |
