# Runtime

The runtime is the engine that executes sessions: launching agents, streaming LLM output, dispatching tool calls, persisting state, and emitting events. Everything runs on the Bun event loop.

## Components

```
SessionRunner    — orchestrates session loops; the swarm scheduler
AgentLoop        — one session's run loop; LLM call → tool calls → emit
EventBus         — in-process event fanout (see 06-events.md)
BlueprintRegistry — file-watched blueprint cache (see 02-blueprints.md)
ToolRegistry     — built-in tool catalog (see 07-tools.md)
Custodian        — periodic maintenance (compaction, pruning, extraction)
Scheduler        — cron-based blueprint launches
WorkLoop         — background work_queue processor (label, embed, detect)
Watcher          — chokidar file watching for code index + blueprints + ideas + policies
```

Each component has a single responsibility and a constructor that takes its dependencies (no globals, no singletons, no DI framework).

## SessionRunner

The orchestrator. Holds a map of active loops, exposes launch/inject/pause/resume/kill.

```typescript
class SessionRunner {
  private loops = new Map<string, AgentLoop>();

  constructor(
    private writeDb: Database,
    private readDb: Database,
    private events: EventBus,
    private registry: BlueprintRegistry,
    private tools: ToolRegistry,
  ) {}

  async launch(sessionId: string, initialMessage?: string): Promise<void> {
    if (this.loops.has(sessionId)) return;
    if (initialMessage) {
      insertUserMessage(this.writeDb, sessionId, initialMessage);
    }
    const loop = new AgentLoop(sessionId, this.deps());
    this.loops.set(sessionId, loop);
    loop.run().finally(() => this.loops.delete(sessionId));
  }

  async inject(sessionId: string, content: string): Promise<void> {
    insertPendingMessage(this.writeDb, sessionId, content);
    const session = getSession(this.readDb, sessionId);
    if (session.status === 'paused' || session.status === 'active') {
      await this.launch(sessionId); // wakes a new loop; loop drains pending
    }
    // if running, the in-flight loop will pick up pending messages between turns
  }

  async pause(sessionId: string): Promise<void> {
    const loop = this.loops.get(sessionId);
    if (loop) await loop.pause();
  }

  async resume(sessionId: string): Promise<void> {
    await this.launch(sessionId);
  }

  async kill(sessionId: string): Promise<void> {
    const loop = this.loops.get(sessionId);
    if (loop) await loop.abort();
  }
}
```

The runner does NOT manage concurrency caps. There's no `maxConcurrent`. Every launched session gets its own loop; loops interleave naturally on the event loop. A user with 50 simultaneous sessions running 50 LLM calls is bound only by API rate limits, which propagate as errors.

## AgentLoop

One per active session. Implements the run loop:

```
loop:
  read pending user messages → mark consumed
  if no messages and run completed: yield; status=active; break
  start run; status=running; emit run.started
  build prompt from session config + blueprint + knowledge + messages
  call LLM (streaming); for each chunk:
    persist tokens to message; emit message.assistant.token
  if assistant produced tool calls:
    for each tool call:
      validate args (schema)
      check pause for HITL marker in text
      emit tool.started
      execute(tool, args, ctx)
      persist tool result message; emit tool.completed/failed
    continue loop (LLM may emit more after tool results)
  if HITL marker fired or request_human_input invoked:
    status=paused; emit run.completed; break
  if max_turns hit: status=paused (max_turns_exceeded); break
  finish run; emit run.completed
goto loop
```

The loop is a single async function. It owns its session's state during execution. Cooperative aborting via `AbortController` plumbed into LLM streams and tool execution.

```typescript
class AgentLoop {
  private abort = new AbortController();

  async run(): Promise<void> {
    try {
      while (!this.abort.signal.aborted) {
        const pending = drainPending(this.writeDb, this.sessionId);
        if (pending.length === 0 && !this.hasUnansweredUser()) break;

        const run = startRun(this.writeDb, this.sessionId);
        try {
          await this.executeRun(run);
        } catch (err) {
          failRun(this.writeDb, run.id, err);
          setSessionStatus(this.writeDb, this.sessionId, 'failed');
          this.events.emit({ type: 'session.failed', sessionId: this.sessionId, ... });
          return;
        }

        if (this.lastRunPaused) {
          setSessionStatus(this.writeDb, this.sessionId, 'paused');
          break;
        }
      }
      setSessionStatus(this.writeDb, this.sessionId, 'active');
    } finally {
      this.cleanup();
    }
  }

  async pause() { /* sets a flag; current run finishes its current turn then yields */ }
  async abort() { this.abort.abort(); }
}
```

## Pending Messages

Injected user messages have `pending = 1`. The loop drains them at the start of each iteration:

```typescript
function drainPending(db: Database, sessionId: string): Message[] {
  const messages = db.query(
    `SELECT * FROM messages WHERE session_id = ? AND pending = 1 ORDER BY seq ASC`
  ).all(sessionId);
  if (messages.length > 0) {
    db.query(`UPDATE messages SET pending = 0 WHERE session_id = ? AND pending = 1`).run(sessionId);
  }
  return messages;
}
```

The flag distinguishes "user wrote this and the agent hasn't seen it yet" from "user wrote this and the agent has incorporated it." The loop never loses a message: insertion + flag + drain is the contract.

## Streaming

The runner uses `streamText` from the `ai` SDK. As tokens arrive:

1. Append to in-memory buffer
2. Periodically (every ~50ms or on tool-call boundary) flush buffer to `messages.content` via UPDATE
3. Emit `message.assistant.token` events on each chunk (high-frequency, SSE-only)

When the stream completes:
1. Final UPDATE persists complete content
2. Tool-call requests are detected; tool execution begins
3. Emit `message.assistant.completed`

Concurrent streams (many sessions) all share the writeDb; updates serialize but each is < 1ms.

## Tool Dispatch

```typescript
async function dispatchTool(call: ToolCall, ctx: ToolContext): Promise<ToolResult> {
  const tool = ctx.tools.get(call.name);
  if (!tool) return { error: 'TOOL_NOT_FOUND' };

  const parsed = tool.inputSchema.safeParse(call.args);
  if (!parsed.success) return { error: 'INVALID_ARGS', details: parsed.error };

  ctx.events.emit({ type: 'tool.started', sessionId: ctx.sessionId, runId: ctx.runId, detail: { name: call.name, args: parsed.data }});
  const start = performance.now();

  try {
    const result = await tool.execute(parsed.data, ctx);
    const ms = performance.now() - start;
    ctx.events.emit({ type: 'tool.completed', ..., detail: { tool_call_id: call.id, duration_ms: ms }});
    return { ok: true, value: result };
  } catch (err) {
    ctx.events.emit({ type: 'tool.failed', ..., detail: { tool_call_id: call.id, error: String(err) }});
    return { error: 'TOOL_ERROR', message: String(err) };
  }
}
```

## HITL Signals

The runtime watches each assistant text chunk for the checkpoint marker `[[checkpoint: name]]`. When detected:
- if `name` is in blueprint's `pause_after`, set `lastRunPaused = true` after the current turn finishes
- emit `hitl.checkpoint` event

The `request_human_input` tool's execute function sets `lastRunPaused = true` directly and emits `hitl.requested`.

In both cases, the loop completes the current run, sets session status to `paused`, breaks. Resumes when user injects a message (which calls `runner.launch` again).

## Concurrency

The runtime is single-threaded (Bun event loop). Concurrency is via async interleaving while awaiting LLM/HTTP responses.

Per-session ordering: each session has at most one `AgentLoop` at a time. The runner enforces this via the `Map`. Inject-while-running queues; doesn't fork.

Cross-session ordering: independent. Two sessions writing messages serialize on the writeDb, but each individual write is fast (< 1ms).

## Custodian

Periodic maintenance. Triggered via `setInterval` and on session archive.

Responsibilities:

| Job | Frequency | Action |
|-----|-----------|--------|
| Auto-archive | hourly | sessions in `completed` for > N days → `archived` |
| Compact running session | per-run, when message count > threshold | summarize old half into a `system` message; mark originals compacted |
| Extract on archive | per archive | label, pointers, skills, conventions; emit knowledge events |
| Prune events | daily | events older than retention |
| Prune work queue | daily | processed rows older than 7 days |
| Embed pending chunks | every 5 min | call embedder for any code chunks lacking embeddings |
| Detect conventions | nightly | LLM pass over recent code + sessions |

```typescript
class Custodian {
  constructor(/* deps */) {}
  start() {
    setInterval(() => this.tick(), 60_000); // 1m heartbeat
  }
  async tick() {
    const now = Date.now();
    if (this.dueDaily(now)) await this.daily();
    if (this.dueHourly(now)) await this.hourly();
    if (this.dueEvery5(now)) await this.every5();
  }
}
```

Each task is implemented in its own file (`packages/engine/src/custodian/<task>.ts`) and registered with the custodian. Adding a new periodic task means writing a function and adding one line to register it.

## Scheduler

Reads `schedules` table, fires due rows.

```typescript
class Scheduler {
  start() {
    setInterval(() => this.tick(), 30_000);
  }
  async tick() {
    const due = readDb.query(
      `SELECT * FROM schedules WHERE enabled = 1 AND next_run_at <= ?`
    ).all(unixNow());

    for (const row of due) {
      const session = await actions.launchAgent(this.ctx, {
        blueprint: row.blueprint,
        task: row.action ?? 'Scheduled run',
      });
      writeDb.query(
        `UPDATE schedules SET last_run_at = ?, last_session_id = ?, next_run_at = ? WHERE id = ?`
      ).run(unixNow(), session.id, computeNext(row.cron_expr), row.id);
      this.events.emit({ type: 'schedule.fired', sessionId: session.id, blueprint: row.blueprint });
    }
  }
}
```

If a schedule fires while the prior session for that schedule is still running, the run is **skipped** (not queued). `schedule.skipped` event emitted with reason. Avoids stampede.

## Watcher

Single chokidar instance with function-based ignore (see fix in `apps/cli/src/watcher.ts`):

```typescript
class Watcher {
  start(repoRoot: string) {
    const codeWatcher = chokidar.watch(repoRoot, {
      ignored: makeIgnore(repoRoot),
      ignoreInitial: true,
    });
    codeWatcher.on('add', this.onCode);
    codeWatcher.on('change', this.onCode);
    codeWatcher.on('unlink', this.onCodeRemoved);

    const configWatcher = chokidar.watch([
      `${HOME}/.moneypenny/blueprints`,
      `${HOME}/.moneypenny/ideas`,
      `${HOME}/.moneypenny/policies`,
      `${repoRoot}/.moneypenny`,
    ], { ignoreInitial: false });
    configWatcher.on('all', this.onConfigChange);
  }
}
```

The watcher dispatches to: BlueprintRegistry (blueprints), IdeasRegistry (ideas), PolicySync (policies), CodeIndexer (code).

## WorkLoop

Drains `work_queue` for asynchronous tasks (label, embed, detect, extract). Replaces the v1 inline `processWorkQueue`. Runs on its own interval; respects the write-discipline rules (batch 50–100, yield between batches).

## Startup Order

```
1. openWriteDb / openReadDb
2. apply migrations
3. ToolRegistry.registerBuiltins
4. BlueprintRegistry.start (load + watch)
5. PolicySync.start (load + watch + sync to db)
6. EventBus
7. SessionRunner
8. WorkLoop.start
9. Custodian.start
10. Scheduler.start
11. Watcher.start
12. HTTP server (Hono routes + SSE)
13. emit 'system.started'
14. resume sessions in `running` status that were left running on prior shutdown
```

Step 14 is critical: on a crash/restart, sessions with `status = 'running'` had their loop killed. The runtime sets them to `failed` with reason `runtime_crash` so user can resume manually. (Auto-resume is dangerous — could re-bill duplicate runs.)

## Shutdown

`SIGINT` / `SIGTERM`:
1. Stop accepting new HTTP requests
2. Send abort to all active loops
3. Wait up to 5s for loops to drain
4. Close watchers
5. Emit `system.shutdown`
6. Close DBs
7. Exit
