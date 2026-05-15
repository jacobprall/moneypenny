# Channel Adapters

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
