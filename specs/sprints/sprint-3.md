# Sprint 3 — Channels, Reactivity, and Self-Evolution

> The sprint that opens moneypenny to the outside world and gives it
> reflexes. Multi-channel I/O (Telegram, webhooks, WASM), a reactive
> event layer driven by SQLite write hooks, self-evolving agent prompts
> informed by usage data, and an embeddable SQL extension that lets
> any SQLite client query the intelligence file.

**Prerequisites:** Sprint 2 complete (read/write separation, unified query,
compaction, gardener)

---

## Overview

Sprint 3 adds four capabilities that transform moneypenny from a local
development tool into a composable intelligence platform.

| # | Workstream | Packages touched |
|---|-----------|-----------------|
| 1 | Channel adapters (Telegram, webhook, WASM) | new `@moneypenny/channels` |
| 2 | Reactive event layer | `@moneypenny/db`, new `@moneypenny/events` |
| 3 | Self-evolving prompts | `@moneypenny/ctx`, `@moneypenny/loop` |
| 4 | Embeddable SQL extension | new `@moneypenny/sql-ext` |

---

## 1. Channel Adapters

### Problem

Moneypenny can only be reached via CLI (`mp chat`) and web UI (`mp serve`).
Solo developers want to interact with their agent from Telegram while AFK,
receive webhook notifications when jobs complete, and embed a chat widget
in their own apps (WASM).

### Design

A `ChannelAdapter` interface that normalizes inbound/outbound messages
across transports. The agent loop is already channel-agnostic thanks to the
`AgentBridge` from sprint 1 — channels just need to translate their wire
format into bridge calls and stream `AgentEvent`s back.

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

| Event | Telegram | Webhook | WASM |
|-------|----------|---------|------|
| `stream_token` | Batched edit (every 500ms) | Not sent | Direct callback |
| `tool_call_start` | Italic status: "_Using code_search..._" | JSON payload | JSON event |
| `tool_call_result` | Collapsed summary if long | JSON payload | JSON event |
| `turn_complete` | Final message with cost footer | JSON payload | JSON event |
| `error` | Error message with retry button | JSON payload with error code | Error callback |

### Telegram adapter

```typescript
export class TelegramAdapter implements ChannelAdapter {
  readonly name = "telegram";
  private bot: TelegramBot;
  private pollingActive: boolean;

  async start(bridge: AgentBridge, config: ChannelConfig): Promise<void> {
    this.bot = new TelegramBot(config.credentials.botToken);
    const allowedUsers = (config.options.allowedUsers as string[]) ?? [];

    this.bot.on("message", async (msg) => {
      if (allowedUsers.length > 0 && !allowedUsers.includes(String(msg.from?.id))) {
        await this.bot.sendMessage(msg.chat.id, "Unauthorized.");
        return;
      }

      const channelMsg = this.normalize(msg);
      const sessionId = this.sessionForChat(msg.chat.id);

      let responseText = "";
      for await (const event of bridge.run(channelMsg.text, { sessionId })) {
        if (event.type === "stream_token") {
          responseText += event.text;
          await this.debouncedEdit(msg.chat.id, responseText);
        } else if (event.type === "tool_call_start") {
          await this.bot.sendChatAction(msg.chat.id, "typing");
        } else if (event.type === "turn_complete") {
          await this.sendFinal(msg.chat.id, responseText, event);
        } else if (event.type === "error") {
          await this.bot.sendMessage(msg.chat.id, `Error: ${event.message}`);
        }
      }
    });

    await this.bot.startPolling();
  }
}
```

Configuration in `.mp/config.yaml`:
```yaml
channels:
  telegram:
    enabled: true
    bot_token: "${TELEGRAM_BOT_TOKEN}"
    allowed_users:
      - "123456789"
    default_blueprint: "default"
    session_mode: per_chat        # per_chat | per_user | single
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
    await fetch(url, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "X-MP-Signature": signature,
      },
      body,
    });
  }
}
```

Configuration:
```yaml
channels:
  webhook:
    enabled: true
    url: "https://your-server.com/mp-events"
    secret: "${WEBHOOK_SECRET}"
    events:
      - job_complete
      - error
      - cost_alert
      - session_complete
```

### WASM adapter

A browser-embeddable build of the moneypenny client that connects to
`mp serve` over WebSocket and provides a JavaScript API for embedding
a chat widget in any web app.

```typescript
// @moneypenny/channels/wasm

export class MoneypennyChatWidget {
  private ws: WebSocket;
  private sessionId: string;

  constructor(config: { serverUrl: string; token: string; containerId: string }) {
    this.ws = new WebSocket(`${config.serverUrl}/api/v1/chat/stream`);
  }

  async sendMessage(text: string): Promise<void> {
    this.ws.send(JSON.stringify({
      type: "message",
      sessionId: this.sessionId,
      text,
    }));
  }

  onEvent(handler: (event: AgentEvent) => void): void;
  destroy(): void;
}
```

The WASM package is a thin WebSocket client (not the full moneypenny
runtime compiled to WASM). It's published as `@moneypenny/embed` and can
be used via:

```html
<script type="module">
  import { MoneypennyChatWidget } from "@moneypenny/embed";
  const mp = new MoneypennyChatWidget({
    serverUrl: "http://localhost:1745",
    token: "your-serve-token",
    containerId: "mp-chat",
  });
</script>
```

### Channel management

Channels are registered and managed through the HTTP API and web UI:

```
GET    /api/v1/channels              List channels + status
PATCH  /api/v1/channels/:name        Enable/disable, update config
GET    /api/v1/channels/:name/stats  Message count, errors, uptime
```

`mp serve` starts all enabled channels on boot. Channels can be
hot-reloaded via API without restarting the server.

### Session bridging

Each channel maps conversations to sessions:

| Mode | Behavior |
|------|----------|
| `per_chat` | Each Telegram chat / webhook source gets its own session |
| `per_user` | Each unique user gets a session across channels |
| `single` | All inbound messages go to one session |
| `ephemeral` | New session per message (stateless) |

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 1.1 | `ChannelAdapter` interface, `ChannelMessage` types, channel registry | 1.5 days |
| 1.2 | Telegram adapter with polling, message normalization, streaming edits | 3 days |
| 1.3 | Webhook adapter with HMAC signing, event filtering | 1.5 days |
| 1.4 | WASM/embed adapter (WebSocket client, published as `@moneypenny/embed`) | 2 days |
| 1.5 | Channel management API + web UI page + hot reload | 1.5 days |
| 1.6 | Session bridging (per_chat, per_user, single, ephemeral) | 1 day |

---

## 2. Reactive Event Layer

### Problem

Today, side-effects are imperative: after an agent writes a memory, the
code explicitly calls the indexer. After a job fails, nothing happens. The
moneypenny-rs spec (sprint-4/self-aware-db.md) envisions a reactive layer
where database writes automatically trigger downstream effects through an
event bus.

### Design

Use Bun's SQLite `update_hook` (or a polled WAL watcher as fallback) to
detect writes, then route events through a typed event bus.

```typescript
// @moneypenny/events

export type DbEvent =
  | { type: "row_insert"; table: string; rowid: number }
  | { type: "row_update"; table: string; rowid: number }
  | { type: "row_delete"; table: string; rowid: number };

export type IntelligenceEvent =
  | { type: "memory_added"; memoryId: string; context: string }
  | { type: "session_completed"; sessionId: string; costUsd: number }
  | { type: "job_completed"; jobId: string; status: "completed" | "failed" }
  | { type: "cost_threshold_crossed"; currentUsd: number; thresholdUsd: number }
  | { type: "skill_discovered"; skillName: string }
  | { type: "index_stale"; staleFileCount: number }
  | { type: "compaction_needed"; sessionId: string; messageCount: number }
  | { type: "governance_violation"; effect: string; toolName: string; policyName: string };

export class EventBus {
  private listeners: Map<string, Set<EventHandler>>;

  /** Register a listener for a specific event type. */
  on<T extends IntelligenceEvent["type"]>(
    type: T,
    handler: (event: Extract<IntelligenceEvent, { type: T }>) => void | Promise<void>,
  ): Unsubscribe;

  /** Emit an event, dispatching to all registered listeners. */
  emit(event: IntelligenceEvent): void;

  /** Drain: wait for all async handlers to complete. For graceful shutdown. */
  drain(): Promise<void>;
}
```

### Event routing from DB writes

```typescript
// Bridge between raw SQLite hooks and typed events

const TABLE_EVENT_MAP: Record<string, (rowid: number, readers: DbReadPool) => IntelligenceEvent | null> = {
  knowledge: (rowid, readers) => {
    const row = readers.read(db =>
      db.prepare("SELECT id, context FROM knowledge WHERE rowid = ?").get(rowid)
    );
    return row ? { type: "memory_added", memoryId: row.id, context: row.context } : null;
  },

  job_runs: (rowid, readers) => {
    const row = readers.read(db =>
      db.prepare("SELECT job_id, status FROM job_runs WHERE rowid = ?").get(rowid)
    );
    if (!row || row.status === "running" || row.status === "pending") return null;
    return { type: "job_completed", jobId: row.job_id, status: row.status };
  },

  sessions: (rowid, readers) => {
    // check if session just ended (status changed to completed)
    // ...
  },
};
```

### Reactive handlers (built-in)

| Event | Handler | Effect |
|-------|---------|--------|
| `memory_added` | Embed handler | Generate embedding for new memory, upsert to vector index |
| `session_completed` | Compaction check | If message count > threshold, schedule compaction |
| `job_completed` (failed) | Webhook notifier | Send to configured webhook channels |
| `cost_threshold_crossed` | Cost alert | Notify via webhook, log warning |
| `index_stale` | Watcher hint | Mark files for re-index on next watcher tick |
| `skill_discovered` | Skill indexer | Scan and catalog new skill |

### Custom handlers

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

### Integration with the event loop

The event bus is initialized with `mp serve` and wired into the `DbWriter`:

```typescript
// In DbWriter.write(), after successful commit:
for (const event of pendingEvents) {
  eventBus.emit(event);
}
```

The bus is fire-and-forget for non-critical handlers. Critical handlers
(like embedding) are awaited with a timeout.

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 2.1 | `EventBus` class with typed listeners, emit, drain | 1 day |
| 2.2 | SQLite update hook → `DbEvent` → `IntelligenceEvent` routing | 2 days |
| 2.3 | Built-in reactive handlers (embed, compaction check, cost alert) | 2 days |
| 2.4 | Custom handler loading from `.mp/events/*.yaml` | 1.5 days |
| 2.5 | Wire into `DbWriter`, integration tests | 1 day |

---

## 3. Self-Evolving Prompts

### Problem

Agent prompts are static. A blueprint's system prompt is the same on day 1
as day 100, even though the agent has accumulated context about the user's
preferences, common patterns, frequent errors, and coding style. The
moneypenny-rs spec (sprint-4/self-aware-db.md) describes a system where
prompts evolve based on usage data.

### Design

A `PromptEvolver` that periodically analyzes agent usage patterns and
generates prompt refinements. These refinements are stored in the database
and injected into the system prompt alongside the static blueprint prompt.

```typescript
// @moneypenny/ctx

export interface PromptEvolver {
  /**
   * Analyze usage patterns for an agent and generate prompt refinements.
   * Typically run by the gardener agent or on schedule.
   */
  evolve(agentName: string): Promise<PromptRefinement[]>;

  /**
   * Get active refinements for an agent (injected into system prompt).
   */
  getRefinements(agentName: string): PromptRefinement[];

  /**
   * Accept or reject a refinement (user feedback loop).
   */
  setRefinementStatus(refinementId: string, status: "accepted" | "rejected"): void;
}

export interface PromptRefinement {
  id: string;
  agentName: string;
  category: RefinementCategory;
  content: string;
  confidence: number;           // 0..1
  status: "proposed" | "accepted" | "rejected";
  evidence: string;             // what data led to this refinement
  createdAt: number;
}

export type RefinementCategory =
  | "user_preference"           // "User prefers functional style over classes"
  | "common_pattern"            // "This codebase uses Zod for validation"
  | "error_prevention"          // "Always check for null before accessing .name"
  | "tool_usage"                // "User prefers code_search over file_read for discovery"
  | "style_guide"               // "Use single quotes, 2-space indent"
  | "domain_knowledge";         // "The billing module uses Stripe's API v2023-10-16"
```

### Schema

```sql
CREATE TABLE prompt_refinements (
  id TEXT PRIMARY KEY NOT NULL,
  agent_name TEXT NOT NULL,
  category TEXT NOT NULL,
  content TEXT NOT NULL,
  confidence REAL NOT NULL DEFAULT 0.5,
  status TEXT NOT NULL DEFAULT 'proposed',   -- proposed | accepted | rejected
  evidence TEXT,
  source_sessions TEXT,                       -- JSON array of session IDs
  created_at INTEGER NOT NULL DEFAULT (unixepoch()),
  updated_at INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE INDEX idx_refinements_agent ON prompt_refinements(agent_name, status);
```

### Evolution analysis

The `evolve()` method:

1. Loads the last N sessions for the agent (default 20)
2. Extracts patterns using an LLM call:
   - Tool usage frequencies
   - User corrections ("no, I meant..." / "actually, use X instead")
   - Repeated instructions across sessions
   - Error patterns (same error type across sessions)
   - Style preferences (inferred from accepted code)
3. Compares against existing refinements
4. Proposes new refinements or updates confidence on existing ones

### Evolution prompt

```
Analyze these recent coding sessions for agent "{{agentName}}".

Identify recurring patterns in these categories:
1. User preferences (coding style, naming, architecture choices)
2. Common patterns (frameworks, libraries, APIs used repeatedly)
3. Error prevention (mistakes the agent made that the user corrected)
4. Tool usage patterns (which tools the user prefers for what tasks)
5. Style guides (formatting, conventions observed in accepted code)
6. Domain knowledge (business logic, API details, architecture decisions)

For each pattern, provide:
- category: one of the above
- content: a concise instruction for the agent's system prompt
- confidence: 0..1 based on how consistent the pattern is
- evidence: specific session excerpts that support this

Only propose refinements with confidence >= 0.5.
```

### Injection into system prompt

```typescript
// @moneypenny/ctx assembler

function buildSystemPrompt(blueprint: AgentConfig, refinements: PromptRefinement[]): string {
  const accepted = refinements.filter(r => r.status === "accepted");

  if (accepted.length === 0) return blueprint.systemPrompt;

  const refinementBlock = accepted
    .sort((a, b) => b.confidence - a.confidence)
    .map(r => `- ${r.content}`)
    .join("\n");

  return `${blueprint.systemPrompt}

## Learned preferences

Based on our previous interactions, I've learned:
${refinementBlock}`;
}
```

### User feedback loop

The web UI Tune page includes a "Learned Preferences" section:

- Lists all refinements (proposed, accepted, rejected)
- User can accept/reject proposed refinements
- Accepted refinements are injected into the system prompt
- Rejected refinements are excluded from future proposals (same content)

The `context_curate` tool also exposes refinement management:
```
context_curate({ action: "list_refinements", params: { agent: "default" } })
context_curate({ action: "accept_refinement", params: { id: "ref_123" } })
context_curate({ action: "reject_refinement", params: { id: "ref_123" } })
```

### Auto-accept threshold

Refinements with confidence >= 0.9 and consistent evidence across 5+
sessions are auto-accepted. All others require explicit user acceptance.

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 3.1 | `prompt_refinements` schema, `PromptRefinement` types, CRUD | 1 day |
| 3.2 | `PromptEvolver.evolve()` — session analysis, LLM extraction | 3 days |
| 3.3 | System prompt injection with accepted refinements | 1 day |
| 3.4 | User feedback: web UI Tune section + `context_curate` integration | 1.5 days |
| 3.5 | Auto-accept logic, gardener integration (scheduled evolution runs) | 1 day |
| 3.6 | Reactive trigger: `session_completed` → check if evolution is due | 0.5 days |

---

## 4. Embeddable SQL Extension

### Problem

The intelligence file is a SQLite database, but its schema is an
implementation detail. External tools (Datasette, DBeaver, custom scripts)
can query it, but they need to understand the internal schema. The
moneypenny-rs spec envisions an "embeddable SQL intelligence extension" —
a set of views, functions, and virtual tables that make the intelligence
file queryable with a stable, documented API.

### Design

A loadable SQLite extension (or, more practically in the TypeScript world,
a schema layer) that provides:

1. **Stable views** that abstract over internal tables
2. **Custom SQL functions** for common intelligence queries
3. **FTS5 integration** for natural language search from SQL

```typescript
// @moneypenny/sql-ext

export function installIntelligenceExtension(db: Database): void {
  installViews(db);
  installFunctions(db);
  installFTS(db);
}
```

### Views

```sql
-- Stable query surface: agent activity
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

-- Stable query surface: tool usage
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

-- Stable query surface: daily cost
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

-- Stable query surface: governance log
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

-- Stable query surface: knowledge base
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

### Custom SQL functions

Registered via Bun's `db.function()`:

```typescript
function installFunctions(db: Database): void {
  db.function("mp_search", {
    args: 2,  // (table, query)
    handler: (table: string, query: string) => {
      // hybrid search across the specified table
      // returns JSON array of {id, score, snippet}
    },
  });

  db.function("mp_token_cost", {
    args: 3,  // (model, input_tokens, output_tokens)
    handler: (model: string, inputTokens: number, outputTokens: number) => {
      return calculateCost({ model, inputTokens, outputTokens });
    },
  });

  db.function("mp_summarize_session", {
    args: 1,  // (session_id)
    handler: (sessionId: string) => {
      // returns compacted summary or first user message
    },
  });

  db.function("mp_time_ago", {
    args: 1,  // (unix_timestamp)
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

### FTS integration

```sql
-- FTS5 index for full-text search across messages
CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
  content,
  content='messages',
  content_rowid='rowid',
  tokenize='porter unicode61'
);

-- FTS5 triggers to keep index in sync
CREATE TRIGGER IF NOT EXISTS messages_ai AFTER INSERT ON messages BEGIN
  INSERT INTO messages_fts(rowid, content) VALUES (new.rowid, new.content);
END;

CREATE TRIGGER IF NOT EXISTS messages_ad AFTER DELETE ON messages BEGIN
  INSERT INTO messages_fts(messages_fts, rowid, content) VALUES ('delete', old.rowid, old.content);
END;

-- FTS5 index for knowledge
CREATE VIRTUAL TABLE IF NOT EXISTS knowledge_fts USING fts5(
  content,
  context,
  content='knowledge',
  content_rowid='rowid',
  tokenize='porter unicode61'
);
```

### Documentation

The extension ships with a `SCHEMA.md` that documents every view, function,
and FTS table. This file is generated from the SQL definitions and serves
as the API contract for external tools:

```markdown
# moneypenny Intelligence File — SQL API

## Views

### mp_agent_activity
Agent-level aggregate statistics.
| Column | Type | Description |
| ...

### mp_tool_usage
Tool call statistics across all sessions.
...

## Functions

### mp_search(table, query)
Hybrid full-text + semantic search. Returns JSON array.
...
```

### Usage examples

```sql
-- What agents cost the most this week?
SELECT agent, total_cost_usd FROM mp_agent_activity ORDER BY total_cost_usd DESC;

-- Which tools fail most often?
SELECT tool_name, failure_count, success_count FROM mp_tool_usage
WHERE failure_count > 0 ORDER BY failure_count DESC;

-- Search memories about authentication
SELECT mp_search('knowledge', 'authentication flow');

-- Daily cost trend
SELECT day, cost_usd FROM mp_daily_cost LIMIT 30;

-- Recent governance denials
SELECT tool_name, reason, created_at_iso FROM mp_governance_log
WHERE effect = 'deny' LIMIT 20;
```

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 4.1 | Stable views (agent_activity, tool_usage, daily_cost, governance_log, knowledge) | 1.5 days |
| 4.2 | Custom SQL functions (mp_search, mp_token_cost, mp_time_ago, mp_summarize_session) | 2 days |
| 4.3 | FTS5 indexes + sync triggers for messages and knowledge | 1 day |
| 4.4 | SCHEMA.md generation, documentation | 0.5 days |
| 4.5 | `installIntelligenceExtension()` entry point, integration tests | 1 day |

---

## Implementation Order

```
Phase 2: Reactive event layer (§2)
  │       ↑ unlocks reactive handlers used by §3
  │
  ├── Phase 1: Channel adapters (§1) [independent]
  │   only needs AgentBridge from sprint 1
  │
  ├── Phase 4: SQL extension (§4) [independent]
  │   pure schema/function work, no runtime dependencies
  │
  └── Phase 3: Self-evolving prompts (§3)
      depends on §2 (reactive layer for session_completed trigger)
      depends on sprint 2 §5 (gardener for scheduled evolution)
```

Channels (§1) and SQL extension (§4) can start immediately. The reactive
layer (§2) should be built before self-evolving prompts (§3) so that
evolution can be triggered reactively on session completion.

---

## What we deliberately skip

- **Bidirectional Telegram** (file upload from agent to user) — can be
  added incrementally after the adapter lands.
- **Discord / Slack adapters** — same ChannelAdapter interface, implement
  on demand.
- **Full WASM runtime** (running the agent loop in the browser) — the embed
  package is a WebSocket client only. Full WASM is a separate effort.
- **Multi-agent reactive choreography** (event chains triggering other
  agents) — the event bus supports it, but the UX for defining chains is
  out of scope.
