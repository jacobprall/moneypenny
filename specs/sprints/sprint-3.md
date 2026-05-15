# Sprint 3 — Channels, Reactivity, and Self-Evolution

> The sprint that opens moneypenny to the outside world and gives it
> reflexes. Multi-channel I/O (Telegram, webhooks, embeddable JS SDK),
> a reactive event layer driven by SQLite write hooks, self-evolving
> agent prompts informed by usage data, and a stable SQL query surface
> for external tools.

**Prerequisites:** Sprint 2 complete (embeddings, parallel tools, unified
query, compaction, gardener)

---

## Existing foundations (already implemented)

| Component | Location | Status |
|-----------|----------|--------|
| `AgentBridge` event protocol | `@moneypenny/bridge` (sprint 1) | Production. Channels plug into this. |
| WebSocket streaming | `@moneypenny/http` (sprint 1) | Production. Embed SDK reuses WS protocol. |
| `DbWriter.exclusive()` + `defer()` | `@moneypenny/db/writer.ts` | Production. Reactive layer hooks into `flushDeferredSync`. |
| `DbWriter.flushDeferredSync()` | `@moneypenny/db/writer.ts` | Runs deferred batch in IMMEDIATE transaction. |
| `appendEvent` (uses `writer.defer`) | `@moneypenny/db/events.ts` | Production. Events are deferred writes. |
| `EventBus` is **not** built | — | Sprint 3 builds it. |
| Prompt refinements are **not** built | — | Sprint 3 builds them. |
| Channel adapters are **not** built | — | Sprint 3 builds them. |

---

## Overview

| # | Workstream | Packages touched |
|---|-----------|-----------------|
| 1 | Channel adapters (Telegram, webhook, embeddable SDK) | new `@moneypenny/channels`, new `@moneypenny/embed` |
| 2 | Reactive event layer | `@moneypenny/db`, new `@moneypenny/events` |
| 3 | Self-evolving prompts | `@moneypenny/ctx`, `@moneypenny/loop` |
| 4 | Stable SQL query surface | `@moneypenny/db` |

---

## 1. Channel Adapters

### Problem

Moneypenny can only be reached via CLI (`mp chat`) and web UI (`mp serve`).
Solo developers want to interact with their agent from Telegram while AFK,
receive webhook notifications when jobs complete, and embed a chat widget
in their own apps.

### Design

A `ChannelAdapter` interface that normalizes inbound/outbound messages
across transports. The agent loop is already channel-agnostic thanks to the
`AgentBridge` from sprint 1 — channels translate their wire format into
bridge calls and stream `AgentEvent`s back.

```typescript
// @moneypenny/channels

export interface ChannelAdapter {
  readonly name: string;
  start(bridge: AgentBridge, config: ChannelConfig): Promise<void>;
  stop(): Promise<void>;
  status(): ChannelStatus;
}

export interface ChannelConfig {
  enabled: boolean;
  credentials: Record<string, string>;
  options: Record<string, unknown>;
}

export type ChannelStatus = {
  connected: boolean;
  lastMessageAt: number | null;
  messageCount: number;
  errors: string[];
};
```

### Inbound message normalization

```typescript
export interface ChannelMessage {
  channelName: string;
  senderId: string;
  senderName: string;
  text: string;
  attachments: ChannelAttachment[];
  replyToMessageId: string | null;
  metadata: Record<string, unknown>;
}

export interface ChannelAttachment {
  type: "image" | "file" | "code" | "url";
  name: string;
  content: string | Buffer;
  mimeType: string | null;
}
```

### Outbound event formatting

Each channel adapter implements its own formatting for `AgentEvent`s:

| Event | Telegram | Webhook | Embed SDK |
|-------|----------|---------|-----------|
| `stream_token` | Batched edit (every 1s) | Not sent | Direct callback |
| `tool_call_start` | Italic status: "_Using code_search..._" | JSON payload | JSON event |
| `tool_call_result` | Collapsed summary if long | JSON payload | JSON event |
| `turn_complete` | Final message with cost footer | JSON payload | JSON event |
| `error` | Error message with retry button | JSON payload with error code | Error callback |

### Concurrency under channel load

**The critical question:** what happens when a Telegram message and a web
UI message arrive simultaneously for the same session?

**Design:** `AgentBridge.run()` is **not safe** for concurrent calls on the
same session (the agent loop maintains conversation state, LLM context,
and writes to the same message history). The bridge uses a per-session
mutex:

```typescript
// Inside AgentBridge
private readonly sessionLocks = new Map<string, Promise<void>>();

async run(prompt: string, opts: { sessionId: string }): AsyncGenerator<AgentEvent> {
  // Wait for any in-flight run on this session to complete
  const existing = this.sessionLocks.get(opts.sessionId);
  if (existing) {
    await existing;
  }

  let resolve: () => void;
  this.sessionLocks.set(opts.sessionId, new Promise(r => { resolve = r; }));

  try {
    yield* this.executeRun(prompt, opts);
  } finally {
    this.sessionLocks.delete(opts.sessionId);
    resolve!();
  }
}
```

**Behavior when contention occurs:**

| Scenario | Behavior |
|----------|----------|
| Same session, different channels | Second request queues, runs after first completes |
| Same session, same channel | Same — sequential within session |
| Different sessions | Fully concurrent, no contention |
| Queue depth > 3 for a session | Reject with "session busy" error, channel-specific message |

Different sessions run fully concurrently — there is no global lock.

### Telegram adapter

```typescript
export class TelegramAdapter implements ChannelAdapter {
  readonly name = "telegram";
  private bot: TelegramBot;
  private rateLimiter: TelegramRateLimiter;

  async start(bridge: AgentBridge, config: ChannelConfig): Promise<void> {
    this.bot = new TelegramBot(config.credentials.botToken);
    this.rateLimiter = new TelegramRateLimiter();
    const allowedUsers = (config.options.allowedUsers as string[]) ?? [];

    this.bot.on("message", async (msg) => {
      if (allowedUsers.length > 0 && !allowedUsers.includes(String(msg.from?.id))) {
        await this.rateLimiter.send(() =>
          this.bot.sendMessage(msg.chat.id, "Unauthorized.")
        );
        return;
      }

      const channelMsg = this.normalize(msg);
      const sessionId = this.sessionForChat(msg.chat.id);

      let responseText = "";
      try {
        for await (const event of bridge.run(channelMsg.text, { sessionId })) {
          if (event.type === "stream_token") {
            responseText += event.text;
            await this.rateLimiter.debouncedEdit(msg.chat.id, responseText);
          } else if (event.type === "tool_call_start") {
            await this.rateLimiter.send(() =>
              this.bot.sendChatAction(msg.chat.id, "typing")
            );
          } else if (event.type === "turn_complete") {
            await this.rateLimiter.send(() =>
              this.sendFinal(msg.chat.id, responseText, event)
            );
          } else if (event.type === "error") {
            await this.rateLimiter.send(() =>
              this.bot.sendMessage(msg.chat.id, `Error: ${event.message}`)
            );
          }
        }
      } catch (err) {
        if (err instanceof SessionBusyError) {
          await this.rateLimiter.send(() =>
            this.bot.sendMessage(msg.chat.id, "I'm still working on your previous request.")
          );
        }
      }
    });

    await this.bot.startPolling();
  }
}
```

### Telegram rate limiter

Telegram's Bot API has aggressive rate limits: 30 messages/second globally,
1 message/second per chat for edits. The streaming "batched edit" approach
must respect these limits.

```typescript
class TelegramRateLimiter {
  private readonly perChatMinInterval = 1000;  // 1 msg/sec/chat
  private readonly globalMinInterval = 34;     // ~30 msg/sec global
  private lastGlobal = 0;
  private lastPerChat = new Map<number, number>();
  private queue: Array<{ fn: () => Promise<void>; resolve: () => void }> = [];
  private draining = false;

  async send(fn: () => Promise<void>): Promise<void> {
    return new Promise<void>((resolve) => {
      this.queue.push({ fn, resolve });
      if (!this.draining) this.drain();
    });
  }

  async debouncedEdit(chatId: number, text: string): Promise<void> {
    const last = this.lastPerChat.get(chatId) ?? 0;
    const now = Date.now();
    if (now - last < this.perChatMinInterval) return; // skip, next token batch will catch up
    this.lastPerChat.set(chatId, now);
    await this.send(() => this.bot.editMessageText(chatId, text));
  }

  private async drain(): Promise<void> {
    this.draining = true;
    while (this.queue.length > 0) {
      const now = Date.now();
      const wait = Math.max(0, this.globalMinInterval - (now - this.lastGlobal));
      if (wait > 0) await Bun.sleep(wait);
      const item = this.queue.shift()!;
      this.lastGlobal = Date.now();
      try { await item.fn(); } catch { /* logged */ }
      item.resolve();
    }
    this.draining = false;
  }
}
```

### Configuration

```yaml
# .mp/config.yaml
channels:
  telegram:
    enabled: true
    bot_token: "${TELEGRAM_BOT_TOKEN}"
    allowed_users:
      - "123456789"
    default_blueprint: "default"
    session_mode: per_chat
```

### Webhook adapter

Outbound-only: sends events to a configured URL when specific triggers
fire.

```typescript
export class WebhookAdapter implements ChannelAdapter {
  readonly name = "webhook";

  async start(bridge: AgentBridge, config: ChannelConfig): Promise<void> {
    const url = config.credentials.webhookUrl;
    const events = (config.options.events as string[]) ?? ["job_complete", "error", "cost_alert"];
    const secret = config.credentials.webhookSecret;

    bridge.on("job_complete", async (event) => {
      if (events.includes("job_complete")) {
        await this.send(url, secret, { type: "job_complete", ...event });
      }
    });

    bridge.on("cost_alert", async (event) => {
      if (events.includes("cost_alert")) {
        await this.send(url, secret, { type: "cost_alert", ...event });
      }
    });
  }

  private async send(url: string, secret: string, payload: unknown): Promise<void> {
    const body = JSON.stringify(payload);
    const signature = this.sign(body, secret);

    const maxRetries = 3;
    for (let attempt = 0; attempt <= maxRetries; attempt++) {
      try {
        const res = await fetch(url, {
          method: "POST",
          headers: {
            "Content-Type": "application/json",
            "X-MP-Signature": signature,
            "X-MP-Delivery": crypto.randomUUID(),
          },
          body,
          signal: AbortSignal.timeout(10_000),
        });
        if (res.ok) return;
        if (res.status >= 500 && attempt < maxRetries) {
          await Bun.sleep(1000 * 2 ** attempt);
          continue;
        }
        console.warn(`[mp] webhook ${res.status}: ${await res.text()}`);
        return;
      } catch (err) {
        if (attempt < maxRetries) {
          await Bun.sleep(1000 * 2 ** attempt);
          continue;
        }
        console.warn(`[mp] webhook failed after ${maxRetries} retries: ${err}`);
      }
    }
  }

  private sign(body: string, secret: string): string {
    const encoder = new TextEncoder();
    const hmac = new Bun.CryptoHasher("sha256", encoder.encode(secret));
    hmac.update(encoder.encode(body));
    return `sha256=${hmac.digest("hex")}`;
  }
}
```

### Embeddable JS SDK (`@moneypenny/embed`)

> **Note:** The previous draft called this a "WASM adapter." It is not
> WASM — it is a plain JavaScript WebSocket client that connects to
> `mp serve`. Renamed for clarity.

A browser-embeddable package that connects to `mp serve` over WebSocket
and provides a JavaScript API for embedding a chat widget in any web app.

```typescript
// @moneypenny/embed

export class MoneypennyChatClient {
  private ws: WebSocket;
  private sessionId: string;
  private reconnectAttempts = 0;
  private maxReconnectAttempts = 5;

  constructor(config: {
    serverUrl: string;
    token: string;
    sessionId?: string;
  }) {
    this.sessionId = config.sessionId ?? crypto.randomUUID();
    this.connect(config.serverUrl, config.token);
  }

  private connect(serverUrl: string, token: string): void {
    this.ws = new WebSocket(
      `${serverUrl.replace(/^http/, "ws")}/api/v1/chat/stream?token=${token}`
    );

    this.ws.onclose = (e) => {
      if (e.code !== 1000 && this.reconnectAttempts < this.maxReconnectAttempts) {
        this.reconnectAttempts++;
        const delay = Math.min(1000 * 2 ** this.reconnectAttempts, 30000);
        setTimeout(() => this.connect(serverUrl, token), delay);
      }
    };

    this.ws.onopen = () => { this.reconnectAttempts = 0; };
  }

  async sendMessage(text: string): Promise<void> {
    this.ws.send(JSON.stringify({
      type: "message",
      sessionId: this.sessionId,
      text,
    }));
  }

  onEvent(handler: (event: AgentEvent) => void): () => void {
    const listener = (e: MessageEvent) => {
      const event = JSON.parse(e.data) as AgentEvent;
      handler(event);
    };
    this.ws.addEventListener("message", listener);
    return () => this.ws.removeEventListener("message", listener);
  }

  destroy(): void {
    this.maxReconnectAttempts = 0;
    this.ws.close(1000);
  }
}
```

Published as `@moneypenny/embed`:

```html
<script type="module">
  import { MoneypennyChatClient } from "@moneypenny/embed";
  const mp = new MoneypennyChatClient({
    serverUrl: "http://localhost:1745",
    token: "your-serve-token",
  });
  mp.onEvent(event => console.log(event));
  mp.sendMessage("Hello from my app!");
</script>
```

The package is ~5 KB minified. It reuses the WebSocket protocol defined
in sprint 1 §2 — no server-side changes required beyond what `mp serve`
already exposes.

### Session bridging

Each channel maps conversations to sessions:

| Mode | Behavior |
|------|----------|
| `per_chat` | Each Telegram chat / webhook source gets its own session |
| `per_user` | Each unique user gets a session across channels |
| `single` | All inbound messages go to one session |
| `ephemeral` | New session per message (stateless) |

Session mapping is stored in a `channel_sessions` table (schema v12, §5):

```sql
CREATE TABLE channel_sessions (
  channel_name TEXT NOT NULL,
  external_id TEXT NOT NULL,        -- Telegram chat_id, webhook source, etc.
  session_id TEXT NOT NULL REFERENCES sessions(id),
  created_at INTEGER NOT NULL DEFAULT (unixepoch()),
  PRIMARY KEY (channel_name, external_id)
);
```

### Channel management API

```
GET    /api/v1/channels              List channels + status
PATCH  /api/v1/channels/:name        Enable/disable, update config
GET    /api/v1/channels/:name/stats  Message count, errors, uptime
```

`mp serve` starts all enabled channels on boot. Channels can be
hot-reloaded via API without restarting the server (stop → update config →
start).

### Acceptance criteria

- [ ] Telegram adapter receives messages, streams responses, handles auth
- [ ] Telegram rate limiter prevents API rate limit errors under load
- [ ] Webhook adapter sends HMAC-signed POST requests with retry on 5xx
- [ ] Embed SDK connects via WebSocket, sends/receives messages, auto-reconnects
- [ ] Concurrent messages on different sessions run in parallel
- [ ] Concurrent messages on the same session queue (second waits for first)
- [ ] Session busy (queue depth > 3) returns a clear error to the channel
- [ ] Channel hot-reload works without server restart
- [ ] `per_chat`, `per_user`, `single`, `ephemeral` session modes all work
- [ ] Channel status endpoint returns connected, messageCount, errors

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 1.1 | `ChannelAdapter` interface, `ChannelMessage` types, channel registry | 1.5 days |
| 1.2 | Session-level mutex in `AgentBridge`, `SessionBusyError` | 1 day |
| 1.3 | Telegram adapter with rate limiter, message normalization, streaming edits | 3 days |
| 1.4 | Webhook adapter with HMAC signing, retry, event filtering | 1.5 days |
| 1.5 | `@moneypenny/embed` SDK (WebSocket client, published as npm package) | 1.5 days |
| 1.6 | Channel management API + web UI Channels page + hot reload | 1.5 days |
| 1.7 | Session bridging (per_chat, per_user, single, ephemeral) + `channel_sessions` table | 1 day |

---

## 2. Reactive Event Layer

### Problem

Today, side-effects are imperative: after an agent writes a memory, the
code explicitly calls the indexer. After a job fails, nothing happens.
The moneypenny-rs spec (sprint-4/self-aware-db.md) envisions a reactive
layer where database writes automatically trigger downstream effects.

### DbWriter integration challenge

The existing `DbWriter` has **synchronous** `exclusive()` and deferred
`defer()` methods. There is no async `write()` that returns a Promise.
The reactive layer must work with this design, not against it.

**Solution:** Hook into `flushDeferredSync()` post-commit. After the
IMMEDIATE transaction succeeds, emit events for the batch:

```typescript
// Extended DbWriter (minimal change)
export class DbWriter {
  private eventCallback: ((events: WriteEvent[]) => void) | null = null;

  /** Register a callback invoked after each successful deferred flush. */
  onFlush(cb: (events: WriteEvent[]) => void): void {
    this.eventCallback = cb;
  }

  flushDeferredSync(): void {
    if (this.closed || this.deferred.length === 0) return;
    this.cancelScheduledFlush();
    const batch = this.deferred.splice(0);

    // Collect write metadata during the transaction
    const writeEvents: WriteEvent[] = [];
    try {
      withImmediateTransaction(this.db, () => {
        for (const f of batch) {
          f(this.db);
        }
      });
    } catch (e) {
      console.warn(`[mp] deferred write batch failed: ${e}`);
      return; // no events on failure
    }

    // Post-commit: emit events
    if (this.eventCallback) {
      this.eventCallback(writeEvents);
    }
  }
}
```

For `exclusive()`, a similar post-return hook:

```typescript
exclusive<T>(fn: (db: Database) => T): T {
  // ... existing logic ...
  try {
    const result = withBusyRetry(() => fn(this.db));
    return result;
  } finally {
    this.exclusiveDepth--;
    // ... existing flush logic ...
    // Post-return: collect changes_count from db.changes()
    // and emit via eventCallback
  }
}
```

### WAL visibility timing

With WAL mode, after a write commits on the writer connection, readers
on separate connections see the data immediately (WAL reads check the
WAL file before the main database file). This means:

- Events emitted after `flushDeferredSync()` returns are safe: any
  reactive handler that reads back the row via `DbReadPool` will see it.
- No checkpoint synchronization is needed.

This is verified by SQLite's WAL documentation: "A read transaction that
is started after a write transaction completes will be able to see the
changes made by the write transaction."

### Event types

```typescript
// @moneypenny/events

export type IntelligenceEvent =
  | { type: "memory_added"; memoryId: string; context: string }
  | { type: "session_completed"; sessionId: string; costUsd: number }
  | { type: "job_completed"; jobId: string; status: "completed" | "failed" }
  | { type: "cost_threshold_crossed"; currentUsd: number; thresholdUsd: number }
  | { type: "skill_discovered"; skillName: string }
  | { type: "index_stale"; staleFileCount: number }
  | { type: "compaction_needed"; sessionId: string; messageCount: number }
  | { type: "governance_violation"; effect: string; toolName: string; policyName: string };
```

### EventBus

```typescript
export class EventBus {
  private listeners = new Map<string, Set<EventHandler>>();
  private inflightHandlers: Promise<void>[] = [];

  on<T extends IntelligenceEvent["type"]>(
    type: T,
    handler: EventHandler<T>,
    opts?: { critical?: boolean; maxRetries?: number },
  ): () => void {
    // Register handler with metadata
    const entry = { handler, critical: opts?.critical ?? false, maxRetries: opts?.maxRetries ?? 0 };
    // ...
    return () => { /* unsubscribe */ };
  }

  emit(event: IntelligenceEvent): void {
    const handlers = this.listeners.get(event.type);
    if (!handlers) return;

    for (const entry of handlers) {
      const promise = this.runHandler(entry, event);
      this.inflightHandlers.push(promise);
      promise.finally(() => {
        const idx = this.inflightHandlers.indexOf(promise);
        if (idx >= 0) this.inflightHandlers.splice(idx, 1);
      });
    }
  }

  /** Wait for all in-flight handlers. Used during graceful shutdown. */
  async drain(timeoutMs = 5000): Promise<void> {
    await Promise.race([
      Promise.allSettled(this.inflightHandlers),
      Bun.sleep(timeoutMs),
    ]);
  }
}
```

### Handler failure isolation

Handlers are classified as **critical** or **non-critical**:

| Handler | Critical? | Retry? | Failure behavior |
|---------|-----------|--------|-----------------|
| Embed new memory | Yes | 2 retries, 1s backoff | Log warning, memory is saved but unsearchable by vector |
| Compaction check | No | No | Skip, next session completion will re-trigger |
| Webhook notifier | No | 3 retries, exponential | Log warning after final failure |
| Cost alert | No | No | Log warning |
| Skill indexer | No | 1 retry | Log warning, skill is saved but uncataloged |

**Critical handler execution:**

```typescript
private async runHandler(entry: HandlerEntry, event: IntelligenceEvent): Promise<void> {
  for (let attempt = 0; attempt <= entry.maxRetries; attempt++) {
    try {
      await Promise.race([
        entry.handler(event),
        Bun.sleep(entry.critical ? 10_000 : 5_000).then(() => {
          throw new Error("handler timeout");
        }),
      ]);
      return;
    } catch (err) {
      if (attempt < entry.maxRetries) {
        await Bun.sleep(1000 * 2 ** attempt);
        continue;
      }
      console.warn(
        `[mp] ${entry.critical ? "CRITICAL" : "non-critical"} handler failed for ${event.type}: ${err}`
      );
    }
  }
}
```

Non-critical handler failures are logged but never block the caller.
Critical handler failures are logged with a `CRITICAL` prefix so they
appear in `mp doctor` output.

### Event routing from DB writes

Rather than using SQLite's raw `update_hook` (which fires per-row and
doesn't carry enough context), we use **explicit event emission** at the
call site. This is more reliable and type-safe:

```typescript
// In knowledge write path:
function addMemory(db: AgentDB, memory: NewMemory): Memory {
  const result = db.writer.exclusive((raw) => {
    // insert into knowledge...
    return row;
  });
  db.eventBus?.emit({ type: "memory_added", memoryId: result.id, context: result.context });
  return result;
}

// In job_runs write path:
function updateJobRun(db: AgentDB, runId: string, status: string): void {
  db.writer.exclusive((raw) => {
    // update job_runs set status = ...
  });
  if (status === "completed" || status === "failed") {
    db.eventBus?.emit({ type: "job_completed", jobId, status });
  }
}
```

**Why explicit over SQLite hooks:**

| Approach | Pros | Cons |
|----------|------|------|
| SQLite `update_hook` | Automatic, catches all writes | No context (only rowid + table), requires read-back, fires during transaction |
| Explicit emission | Type-safe, carries full context, fires post-commit | Must be added at each call site |

We choose explicit emission because:
1. The event carries domain context (not just a rowid)
2. Emission happens post-commit (readers can see the data)
3. No risk of handlers running inside a transaction
4. Type-safe — the compiler catches missing event fields

### Custom handlers via YAML

Users can register custom handlers via `.mp/events/` YAML:

```yaml
# .mp/events/notify-on-failure.yaml
name: notify-on-failure
event: job_completed
condition:
  status: failed
action:
  type: webhook
  url: "${SLACK_WEBHOOK_URL}"
  template: |
    Job "{{jobId}}" failed at {{timestamp}}.
```

Custom handlers are always non-critical with no retries.

### Acceptance criteria

- [ ] `EventBus` emits events after successful DB writes (not during transaction)
- [ ] Critical handlers retry on failure with backoff
- [ ] Non-critical handler failures don't block the write path
- [ ] `drain()` waits for in-flight handlers during shutdown
- [ ] Readers can see committed data when handler runs (WAL visibility)
- [ ] Custom YAML handlers load from `.mp/events/` and fire correctly
- [ ] Memory addition triggers auto-embedding via `memory_added` event
- [ ] Job failure triggers webhook notification via `job_completed` event
- [ ] `mp doctor` reports failed critical handler events

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 2.1 | `EventBus` class with typed listeners, critical/non-critical classification, retry, drain | 1.5 days |
| 2.2 | `DbWriter.onFlush()` hook for post-commit event emission | 1 day |
| 2.3 | Explicit event emission at call sites (knowledge, job_runs, sessions, skills) | 1.5 days |
| 2.4 | Built-in reactive handlers (embed, compaction check, cost alert, webhook) | 2 days |
| 2.5 | Custom handler loading from `.mp/events/*.yaml` | 1 day |
| 2.6 | Integration tests: event ordering, handler isolation, drain | 1 day |

---

## 3. Self-Evolving Prompts

### Problem

Agent prompts are static. A blueprint's system prompt is the same on day 1
as day 100, even though the agent has accumulated context about the user's
preferences, common patterns, and coding style. The moneypenny-rs spec
(sprint-4/self-aware-db.md) describes prompts that evolve from usage data.

### Design

```typescript
// @moneypenny/ctx

export interface PromptEvolver {
  evolve(agentName: string): Promise<PromptRefinement[]>;
  getRefinements(agentName: string): PromptRefinement[];
  setRefinementStatus(refinementId: string, status: "accepted" | "rejected"): void;
}

export interface PromptRefinement {
  id: string;
  agentName: string;
  category: RefinementCategory;
  content: string;
  confidence: number;           // 0..1
  status: "proposed" | "accepted" | "rejected";
  evidence: string;
  sourceSessionIds: string[];
  createdAt: number;
  updatedAt: number;
}

export type RefinementCategory =
  | "user_preference"
  | "common_pattern"
  | "error_prevention"
  | "tool_usage"
  | "style_guide"
  | "domain_knowledge";
```

### Schema (migration v12)

```sql
CREATE TABLE prompt_refinements (
  id TEXT PRIMARY KEY NOT NULL,
  agent_name TEXT NOT NULL,
  category TEXT NOT NULL,
  content TEXT NOT NULL,
  confidence REAL NOT NULL DEFAULT 0.5,
  status TEXT NOT NULL DEFAULT 'proposed',
  evidence TEXT,
  source_sessions TEXT,
  created_at INTEGER NOT NULL DEFAULT (unixepoch()),
  updated_at INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE INDEX idx_refinements_agent ON prompt_refinements(agent_name, status);
```

### Token budget and cap

**Problem identified in gap analysis:** Without a cap, accepted refinements
accumulate indefinitely. 20 refinements at 50 tokens each = 1000 tokens
added to every LLM call. After months, this crowds out code context.

**Solution:** Hard cap of **15 accepted refinements** per agent, with a
**750 token budget**. When a new refinement would exceed either limit,
the lowest-confidence accepted refinement is demoted to `archived`:

```typescript
const MAX_REFINEMENTS = 15;
const MAX_REFINEMENT_TOKENS = 750;

function pruneRefinements(
  refinements: PromptRefinement[],
  newRefinement: PromptRefinement,
): { accept: PromptRefinement[]; archive: PromptRefinement[] } {
  const all = [...refinements, newRefinement].sort(
    (a, b) => b.confidence - a.confidence
  );

  const accept: PromptRefinement[] = [];
  const archive: PromptRefinement[] = [];
  let tokenCount = 0;

  for (const r of all) {
    const tokens = estimateTokens(r.content);
    if (accept.length < MAX_REFINEMENTS && tokenCount + tokens <= MAX_REFINEMENT_TOKENS) {
      accept.push(r);
      tokenCount += tokens;
    } else {
      archive.push(r);
    }
  }

  return { accept, archive };
}
```

### Refinement deduplication

**Problem identified in gap analysis:** `evolve()` runs on the last 20
sessions and will propose the same refinement repeatedly if the pattern
persists.

**Solution:** Before proposing a new refinement, check existing refinements
(all statuses) for semantic overlap. Use a two-stage check:

1. **Exact substring match** — if the new content contains an existing
   refinement's content (or vice versa), treat as duplicate.
2. **LLM dedup check** — include existing refinements in the evolution
   prompt so the LLM avoids reproposing them:

```
## Existing refinements (do NOT repropose these)
{{#each existingRefinements}}
- [{{status}}] {{content}}
{{/each}}

Only propose NEW patterns not already covered above.
```

This eliminates the need for embedding-based similarity (which would add
complexity and cost). The LLM is already being called for evolution — the
dedup check is free context.

### Evolution analysis

The `evolve()` method:

1. Loads the last N sessions for the agent (default 20)
2. Loads all existing refinements (proposed, accepted, rejected)
3. Sends to LLM with the evolution prompt
4. LLM returns new refinements, avoiding duplicates of existing ones
5. New refinements are inserted as `proposed`
6. Confidence of existing accepted refinements is updated if the LLM
   confirms the pattern is still consistent

### Evolution prompt

```
Analyze these recent coding sessions for agent "{{agentName}}".

## Existing refinements (do NOT repropose these)
{{#each existingRefinements}}
- [{{status}}] (confidence: {{confidence}}) {{content}}
{{/each}}

## Sessions to analyze
{{#each sessions}}
### Session: {{label}} ({{messageCount}} messages)
{{compactedSummary || firstUserMessage}}
{{/each}}

## Task
Identify NEW recurring patterns in these categories:
1. User preferences (coding style, naming, architecture choices)
2. Common patterns (frameworks, libraries, APIs used repeatedly)
3. Error prevention (mistakes the agent made that the user corrected)
4. Tool usage patterns (which tools the user prefers for what tasks)
5. Style guides (formatting, conventions observed in accepted code)
6. Domain knowledge (business logic, API details, architecture decisions)

For each NEW pattern (not already in existing refinements), provide:
- category: one of the above
- content: a concise instruction for the agent's system prompt (max 50 words)
- confidence: 0..1 based on how consistent the pattern is across sessions
- evidence: specific session excerpts that support this

Only propose refinements with confidence >= 0.5.
Do NOT propose patterns that overlap with existing refinements.
```

### Injection into system prompt

```typescript
function buildSystemPrompt(
  blueprint: AgentConfig,
  refinements: PromptRefinement[],
): string {
  const accepted = refinements
    .filter(r => r.status === "accepted")
    .sort((a, b) => b.confidence - a.confidence);

  if (accepted.length === 0) return blueprint.systemPrompt;

  const refinementBlock = accepted
    .map(r => `- ${r.content}`)
    .join("\n");

  return `${blueprint.systemPrompt}

## Learned preferences

Based on our previous interactions, I've learned:
${refinementBlock}`;
}
```

### Auto-accept threshold

Refinements with confidence >= 0.9 and evidence from 5+ sessions are
auto-accepted. All others require explicit user acceptance via the Tune
page or `context_curate`:

```
context_curate({ action: "list_refinements", params: { agent: "default" } })
context_curate({ action: "accept_refinement", params: { id: "ref_123" } })
context_curate({ action: "reject_refinement", params: { id: "ref_123" } })
```

### User feedback via Tune page

The web UI Tune page includes a "Learned Preferences" section:

- Lists all refinements grouped by status (proposed, accepted, rejected)
- User can accept/reject proposed refinements with one click
- Shows confidence score and evidence excerpt
- Rejected refinements are excluded from future proposals
- Accepted refinements show their injection position in the system prompt

### Reactive trigger

The `session_completed` event (from §2) triggers an evolution check:

```typescript
eventBus.on("session_completed", async (event) => {
  const sessionCount = getSessionCount(db, agentName);
  const lastEvolution = getLastEvolutionRun(db, agentName);

  // Evolve every 10 sessions or every 7 days, whichever comes first
  if (sessionCount - lastEvolution.sessionCount >= 10 ||
      Date.now() / 1000 - lastEvolution.timestamp > 604800) {
    await evolver.evolve(agentName);
  }
});
```

### Acceptance criteria

- [ ] `evolve()` analyzes recent sessions and proposes new refinements
- [ ] Existing refinements are included in the prompt to prevent duplicates
- [ ] Accepted refinements appear in the system prompt as "Learned preferences"
- [ ] Max 15 accepted refinements per agent, within 750 token budget
- [ ] Lowest-confidence refinement is archived when budget is exceeded
- [ ] Auto-accept works for confidence >= 0.9 with 5+ session evidence
- [ ] Rejected refinements are excluded from future proposals
- [ ] `context_curate` exposes refinement management actions
- [ ] Tune page shows refinements grouped by status with accept/reject buttons
- [ ] Evolution triggers every 10 sessions or 7 days via reactive event

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 3.1 | `prompt_refinements` schema, `PromptRefinement` types, CRUD | 1 day |
| 3.2 | `PromptEvolver.evolve()` — session analysis, dedup, LLM extraction | 3 days |
| 3.3 | Token budget pruning, auto-accept logic | 1 day |
| 3.4 | System prompt injection with accepted refinements | 0.5 days |
| 3.5 | User feedback: Tune page section + `context_curate` integration | 1.5 days |
| 3.6 | Reactive trigger: `session_completed` → evolution check | 0.5 days |
| 3.7 | Gardener integration (scheduled evolution runs) | 0.5 days |

---

## 4. Stable SQL Query Surface

> **Renamed from "Embeddable SQL Extension"** — the previous draft
> described custom SQL functions registered via `db.function()`, but
> these only work when the DB is opened through Bun. External tools
> (Datasette, DBeaver, `sqlite3` CLI) won't have access to custom
> functions. The spec now clearly separates what's portable (views)
> from what's Bun-only (functions).

### Problem

The intelligence file is a SQLite database, but its schema is an
implementation detail. External tools can query it, but they need to
understand internal table structures. A stable view layer provides a
documented API.

### Portable layer: SQL views (works everywhere)

These views work in any SQLite client:

```sql
CREATE VIEW IF NOT EXISTS mp_agent_activity AS
SELECT
  a.name AS agent,
  COUNT(DISTINCT s.id) AS sessions,
  SUM(COALESCE(json_extract(s.cost, '$.totalCost'), 0)) AS total_cost_usd,
  SUM(COALESCE(json_extract(s.cost, '$.inputTokens'), 0)) AS total_input_tokens,
  SUM(COALESCE(json_extract(s.cost, '$.outputTokens'), 0)) AS total_output_tokens,
  MAX(s.last_activity_at) AS last_active
FROM agents a
LEFT JOIN sessions s ON s.agent_name = a.name
GROUP BY a.name;

CREATE VIEW IF NOT EXISTS mp_tool_usage AS
SELECT
  json_extract(m.content, '$.name') AS tool_name,
  COUNT(*) AS call_count,
  AVG(COALESCE(json_extract(m.metadata, '$.durationMs'), 0)) AS avg_duration_ms,
  SUM(CASE WHEN json_extract(m.metadata, '$.success') = 1 THEN 1 ELSE 0 END) AS success_count,
  SUM(CASE WHEN json_extract(m.metadata, '$.success') = 0 THEN 1 ELSE 0 END) AS failure_count
FROM messages m
WHERE m.role = 'tool'
GROUP BY tool_name;

CREATE VIEW IF NOT EXISTS mp_daily_cost AS
SELECT
  DATE(last_activity_at, 'unixepoch') AS day,
  COUNT(*) AS sessions,
  SUM(COALESCE(json_extract(cost, '$.totalCost'), 0)) AS cost_usd,
  SUM(COALESCE(json_extract(cost, '$.inputTokens'), 0)) AS input_tokens,
  SUM(COALESCE(json_extract(cost, '$.outputTokens'), 0)) AS output_tokens
FROM sessions
GROUP BY day
ORDER BY day DESC;

CREATE VIEW IF NOT EXISTS mp_governance_log AS
SELECT
  ge.id,
  ge.session_id,
  ge.tool_name,
  ge.effect,
  ge.policy_name,
  ge.reason,
  ge.args_snapshot,
  DATETIME(ge.created_at, 'unixepoch') AS created_at_iso
FROM gov_events ge
ORDER BY ge.created_at DESC;

CREATE VIEW IF NOT EXISTS mp_knowledge AS
SELECT
  k.id,
  k.context,
  k.content,
  k.source,
  DATETIME(k.created_at, 'unixepoch') AS created_at_iso,
  LENGTH(k.embedding) > 0 AS has_embedding
FROM knowledge k
ORDER BY k.created_at DESC;
```

### Bun-only layer: custom SQL functions

These functions are registered via `db.function()` and only work when the
DB is opened through Bun (moneypenny process, `mp query` command):

```typescript
function installFunctions(db: Database): void {
  db.function("mp_token_cost", {
    args: 3,
    handler: (model: string, inputTokens: number, outputTokens: number) => {
      return calculateCost({ model, inputTokens, outputTokens });
    },
  });

  db.function("mp_time_ago", {
    args: 1,
    handler: (ts: number) => {
      const diff = Date.now() / 1000 - ts;
      if (diff < 60) return "just now";
      if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
      if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
      return `${Math.floor(diff / 86400)}d ago`;
    },
  });
}
```

**Excluded:** `mp_search()` as a SQL function. Hybrid search requires
async embedding generation and multi-database access — it doesn't fit
the synchronous SQL function model. Use the `code_search` tool or the
`/api/v1/search` endpoint instead.

### FTS integration

Add FTS5 index for message full-text search:

```sql
CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
  content,
  content='messages',
  content_rowid='rowid',
  tokenize='porter unicode61'
);

CREATE TRIGGER IF NOT EXISTS messages_fts_ai AFTER INSERT ON messages BEGIN
  INSERT INTO messages_fts(rowid, content) VALUES (new.rowid, new.content);
END;

CREATE TRIGGER IF NOT EXISTS messages_fts_ad AFTER DELETE ON messages BEGIN
  INSERT INTO messages_fts(messages_fts, rowid, content) VALUES ('delete', old.rowid, old.content);
END;
```

This allows searching across message history via plain SQL:

```sql
SELECT m.*, mf.rank
FROM messages_fts mf
JOIN messages m ON m.rowid = mf.rowid
WHERE messages_fts MATCH 'authentication'
ORDER BY mf.rank
LIMIT 20;
```

### `mp query` command

A new CLI command to run SQL against the intelligence file with
custom functions pre-loaded:

```bash
# Views work in any tool
mp query "SELECT * FROM mp_agent_activity"

# Custom functions work via mp query
mp query "SELECT agent, mp_time_ago(last_active) FROM mp_agent_activity"

# Export to CSV
mp query --csv "SELECT * FROM mp_daily_cost" > costs.csv

# Pipe-friendly JSON output
mp query --json "SELECT * FROM mp_health"
```

### Documentation: `SCHEMA.md`

The extension ships with a generated `SCHEMA.md` that documents every
view and function. This serves as the API contract:

```markdown
# moneypenny Intelligence File — SQL API

## Portable Views (work in any SQLite client)

### mp_agent_activity
| Column | Type | Description |
| ...

### mp_health (from sprint 2)
...

## Bun-only Functions (require mp query or moneypenny process)

### mp_token_cost(model, input_tokens, output_tokens)
Returns cost in USD. Uses moneypenny's internal pricing table.

### mp_time_ago(unix_timestamp)
Returns human-readable relative time string.
```

### Acceptance criteria

- [ ] All views return correct data when queried from `sqlite3` CLI
- [ ] Views survive schema migrations (created in migration, not at runtime)
- [ ] `mp query` command runs SQL with custom functions loaded
- [ ] `mp query --csv` and `--json` output modes work
- [ ] `SCHEMA.md` is auto-generated and accurate
- [ ] External tools (Datasette) can query views without custom functions

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 4.1 | Stable views (agent_activity, tool_usage, daily_cost, governance_log, knowledge) | 1.5 days |
| 4.2 | Custom SQL functions (mp_token_cost, mp_time_ago) | 0.5 days |
| 4.3 | FTS5 index + sync triggers for messages | 0.5 days |
| 4.4 | `mp query` CLI command with --csv and --json output | 1 day |
| 4.5 | `SCHEMA.md` generation script | 0.5 days |
| 4.6 | Integration tests: views correctness, external tool compatibility | 0.5 days |

---

## 5. Schema additions (migration v12)

```typescript
MIGRATIONS.push({
  version: 12,
  up: (db) => {
    // Channel session mapping
    db.exec(`CREATE TABLE IF NOT EXISTS channel_sessions (
      channel_name TEXT NOT NULL,
      external_id TEXT NOT NULL,
      session_id TEXT NOT NULL REFERENCES sessions(id),
      created_at INTEGER NOT NULL DEFAULT (unixepoch()),
      PRIMARY KEY (channel_name, external_id)
    )`);

    // Prompt refinements
    db.exec(`CREATE TABLE IF NOT EXISTS prompt_refinements (
      id TEXT PRIMARY KEY NOT NULL,
      agent_name TEXT NOT NULL,
      category TEXT NOT NULL,
      content TEXT NOT NULL,
      confidence REAL NOT NULL DEFAULT 0.5,
      status TEXT NOT NULL DEFAULT 'proposed',
      evidence TEXT,
      source_sessions TEXT,
      created_at INTEGER NOT NULL DEFAULT (unixepoch()),
      updated_at INTEGER NOT NULL DEFAULT (unixepoch())
    )`);
    db.exec(`CREATE INDEX IF NOT EXISTS idx_refinements_agent
      ON prompt_refinements(agent_name, status)`);

    // Messages FTS
    db.exec(`CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
      content,
      content='messages', content_rowid='rowid',
      tokenize='porter unicode61'
    )`);
    db.exec(`CREATE TRIGGER IF NOT EXISTS messages_fts_ai AFTER INSERT ON messages BEGIN
      INSERT INTO messages_fts(rowid, content) VALUES (new.rowid, new.content);
    END`);
    db.exec(`CREATE TRIGGER IF NOT EXISTS messages_fts_ad AFTER DELETE ON messages BEGIN
      INSERT INTO messages_fts(messages_fts, rowid, content) VALUES ('delete', old.rowid, old.content);
    END`);

    // Stable views
    db.exec(`CREATE VIEW IF NOT EXISTS mp_agent_activity AS ...`);
    db.exec(`CREATE VIEW IF NOT EXISTS mp_tool_usage AS ...`);
    db.exec(`CREATE VIEW IF NOT EXISTS mp_daily_cost AS ...`);
    db.exec(`CREATE VIEW IF NOT EXISTS mp_governance_log AS ...`);
    db.exec(`CREATE VIEW IF NOT EXISTS mp_knowledge AS ...`);
  },
});
```

---

## 6. Configuration consolidation

By this sprint, configuration lives in 7+ places. Add a `mp config validate`
command that checks consistency across all surfaces:

```bash
mp config validate
# ✓ .mp/config.yaml: valid
# ✓ .mp/agents/default.md: valid blueprint
# ✓ .mp/policies/budget.yaml: valid policy
# ✓ .mp/events/notify-on-failure.yaml: valid event handler
# ✗ .mp/agents/reviewer.md: references tool "lint_check" which is not registered
# ✗ .mp/config.yaml: channel "telegram" enabled but TELEGRAM_BOT_TOKEN not set
```

Implementation: 0.5 days. This is a read-only validation pass over all
config surfaces.

---

## Implementation order

```
Phase 1: Reactive event layer (§2)
  │       ↑ foundation for §1 webhook events and §3 evolution trigger
  │
  ├── Phase 2: Channel adapters (§1) [depends on §2 for webhook events]
  │   Telegram + webhook + embed SDK
  │
  ├── Phase 3: SQL query surface (§4) [independent]
  │   Views, functions, mp query command
  │
  └── Phase 4: Self-evolving prompts (§3) [depends on §2 for session_completed trigger]
      Evolver, refinements, Tune page integration
```

The reactive event layer (§2) should be built first because both channels
and self-evolving prompts depend on it. The SQL query surface (§4) is
independent and can be built in parallel with anything.

---

## What we deliberately skip

- **Bidirectional Telegram** (file upload from agent to user) — can be
  added incrementally after the adapter lands.
- **Discord / Slack adapters** — same `ChannelAdapter` interface, implement
  on demand.
- **Full WASM runtime** (running the agent loop in the browser) — the embed
  package is a WebSocket client only. Full WASM is a separate effort.
- **Multi-agent reactive choreography** (event chains triggering other
  agents) — the event bus supports it, but the UX for defining chains is
  out of scope.
- **`mp_search()` as a SQL function** — hybrid search is async and
  multi-database; it doesn't fit the synchronous SQL function model.
