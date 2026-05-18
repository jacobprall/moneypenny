# Events

Events are typed audit records. They drive the activity feed, SSE streams, debugging, and any future analytics. Append-only.

## Storage

`events` table (see `05-data.md`):

```sql
events(id, type, session_id, run_id, blueprint, detail JSON, created_at)
```

`detail` is a JSON column; its shape depends on `type`. The `type` taxonomy below defines each shape.

## EventBus

Single in-process bus, subscribed to by SSE handlers and the custodian:

```typescript
interface EventBus {
  emit(input: Omit<Event, 'id' | 'created_at'>): void;
  subscribe(filter?: { sessionId?: string; types?: string[] }): AsyncIterable<Event>;
}
```

`emit` synchronously inserts into `events` table and pushes to subscribers. Subscribers iterate via async generators backed by an internal queue per subscription (drops if subscriber lags > 1000 events).

## Type Taxonomy

Naming: `<entity>.<verb>` (lowercase, dot-separated).

### Session lifecycle

| Type | When | Detail |
|------|------|--------|
| `session.created` | new session record inserted | `{ blueprint, cwd, parent_id }` |
| `session.status_changed` | status transition | `{ from, to, reason? }` |
| `session.config_changed` | config mutated | `{ keys: string[] }` |
| `session.completed` | status → completed | `{ run_count, total_cost }` |
| `session.failed` | status → failed | `{ error, last_run_id }` |
| `session.archived` | status → archived | `{ extracted: { skills, pointers, conventions } }` |
| `session.deleted` | hard delete | `{}` |

### Run lifecycle

| Type | When | Detail |
|------|------|--------|
| `run.started` | new run begins | `{ model, blueprint }` |
| `run.completed` | run ends successfully | `{ tokens_in, tokens_out, cost_usd, duration_ms }` |
| `run.failed` | run ends with error | `{ error, duration_ms }` |
| `run.aborted` | user/budget interrupted | `{ reason }` |

### Messages

| Type | When | Detail |
|------|------|--------|
| `message.user` | user message inserted (UI inject or first prompt) | `{ message_id }` |
| `message.assistant.started` | assistant message begins streaming | `{ message_id }` |
| `message.assistant.token` | streaming chunk | `{ message_id, content }` |
| `message.assistant.completed` | assistant message done | `{ message_id, has_tool_calls }` |
| `message.tool.result` | tool result message inserted | `{ message_id, tool_call_id, tool_name }` |

### Tool calls

| Type | When | Detail |
|------|------|--------|
| `tool.started` | tool invocation begins | `{ message_id, tool_call_id, name, args }` |
| `tool.completed` | tool returned | `{ tool_call_id, duration_ms, result_size }` |
| `tool.failed` | tool threw | `{ tool_call_id, error }` |

### Children

| Type | When | Detail |
|------|------|--------|
| `child.spawned` | child session created | `{ child_id, blueprint, parent_run_id }` |
| `child.completed` | child reached completed | (emitted on parent's session_id) `{ child_id }` |
| `child.failed` | child reached failed | `{ child_id, error }` |

### HITL

| Type | When | Detail |
|------|------|--------|
| `hitl.checkpoint` | declared checkpoint hit | `{ checkpoint_name }` |
| `hitl.requested` | `request_human_input` tool called | `{ reason, options? }` |
| `hitl.resumed` | user provided input | `{ via: 'message' }` |

### Knowledge

| Type | When | Detail |
|------|------|--------|
| `knowledge.skill_extracted` | new skill recorded | `{ skill_id, name, confidence }` |
| `knowledge.convention_detected` | new convention | `{ convention_id, name }` |
| `knowledge.pointer_created` | pointer added | `{ pointer_id, key }` |

### Blueprint registry

| Type | When | Detail |
|------|------|--------|
| `blueprint.loaded` | registry parsed a blueprint | `{ name, path }` |
| `blueprint.invalid` | parse failed | `{ path, errors }` |
| `blueprint.removed` | file deleted from registry | `{ name, path }` |

### Schedules

| Type | When | Detail |
|------|------|--------|
| `schedule.fired` | scheduler launched a session | `{ blueprint, session_id }` |
| `schedule.skipped` | overrun, prior run still active | `{ blueprint, reason }` |

### Cwd / safety

| Type | When | Detail |
|------|------|--------|
| `cwd.missing` | session resumed but cwd doesn't exist | `{ cwd }` |
| `permission.denied` | tool call rejected | `{ tool, reason }` |
| `policy.warned` | warn-effect policy matched | `{ policy_name }` |
| `policy.blocked` | deny-effect policy matched | `{ policy_name }` |
| `budget.warned` | spend approaching limit | `{ scope, current, limit }` |
| `budget.exceeded` | spend over limit, action taken | `{ scope, action }` |

### System

| Type | When | Detail |
|------|------|--------|
| `system.started` | process boot complete | `{ version }` |
| `system.shutdown` | clean shutdown | `{}` |
| `index.completed` | code reindex finished | `{ files, chunks, duration_ms }` |
| `tab.opened` | UI opened a tab | `{ tab_id, kind, session_id? }` |
| `tab.closed` | UI closed a tab | `{ tab_id }` |

## Channels

Two SSE channels (see `04-api.md`):

- **Per-session** `/api/sse/sessions/:id` — every event with `session_id = :id`
- **Global** `/api/sse/events` — every event

The UI maintains exactly one global connection plus one per-open-session connection. Per-session is for high-fidelity streaming (tokens, tool calls); global is for cross-tab status (badges, activity feed).

## SSE Format

```
id: <events.id>
event: <type>
data: {"id": 1234, "type": "tool.started", "session_id": "...", "run_id": "...", "detail": {...}, "created_at": 1700000000}

```

The `id:` field is the `events.id` row. Clients send `Last-Event-ID: <id>` on reconnect; server replays from `events` table where `id > last_id` (subject to retention).

## Retention

Default 30 days for general events. Pruned by custodian. Configurable via `system.config.events_retention_days`.

`message.assistant.token` events are excluded from `events` table — too high-volume. They are streamed via SSE only (live consumption); historical message content is available via `messages.content`.

## Performance

Per-event insert is < 1ms. EventBus emit is non-blocking from the perspective of the writer (no fanout backpressure). Subscribers that lag are dropped, not slowed.

For very high-volume run windows (many tokens, many tool calls), the runner buffers SSE-only events (tokens) in memory and persists only the start/end milestones to `events`. The full text remains in `messages.content`.
