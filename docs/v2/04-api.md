# API Surface

## Layers

```
┌────────────────────────────────────────┐
│   actions  (packages/core/src/actions) │  ← single source of truth
└────────────────────┬───────────────────┘
                     │
        ┌────────────┼────────────┐
        ▼            ▼            ▼
   Hono RPC      MCP tools    HTTP (raw)
   (UI)          (Cursor/CD)  (curl, integrations)
```

Every capability is implemented exactly once as an `action` function in `packages/core/src/actions/`. The three transports (Hono RPC, MCP, raw HTTP) are thin adapters.

## Transport Choice: Hono RPC

We use Hono's built-in RPC client (`hc<AppType>`) over tRPC. Rationale:
- Zero extra deps (already on Hono)
- Type inference flows from server → client via `AppType` import
- Direct alignment with HTTP routes (no separate procedure DSL)
- Sufficient for this scope; tRPC's batching/middleware/subscriptions are not needed

## Auth

**None.** Server binds to `127.0.0.1` only. Single-user, single-machine assumption is by design. If `MP_BIND` is set to anything else the process refuses to start (guards against accidental exposure).

## Routers

```typescript
// packages/api/src/router.ts

const app = new Hono()
  .route('/sessions',   sessionsRouter)
  .route('/messages',   messagesRouter)
  .route('/runs',       runsRouter)
  .route('/blueprints', blueprintsRouter)
  .route('/ideas',      ideasRouter)
  .route('/agents',     agentsRouter)
  .route('/tools',      toolsRouter)
  .route('/code',       codeRouter)
  .route('/files',      filesRouter)
  .route('/knowledge',  knowledgeRouter)
  .route('/events',     eventsRouter)
  .route('/tabs',       tabsRouter)
  .route('/system',     systemRouter);

export type AppType = typeof app;
```

### Endpoints

```
sessions:
  GET    /sessions                    list (status, blueprint, label filters; paginated)
  GET    /sessions/:id                get session + recent runs
  POST   /sessions                    create
  POST   /sessions/:id/inject         send message to session
  PATCH  /sessions/:id/config         update config (with config_version)
  POST   /sessions/:id/pause          interrupt running session
  POST   /sessions/:id/resume         resume paused session
  POST   /sessions/:id/complete       mark completed
  POST   /sessions/:id/archive        archive (triggers extraction)
  DELETE /sessions/:id                hard delete

messages:
  GET    /messages                    cross-session FTS search (?q=)
  GET    /messages/by-session/:id     paginated message list (cursor-based)

runs:
  GET    /runs/by-session/:id         list runs for a session
  GET    /runs/:id                    get run detail (incl. messages)

blueprints:
  GET    /blueprints                  list all (registry cache)
  GET    /blueprints/:name            get parsed blueprint
  POST   /blueprints/reload           force registry refresh

ideas:
  GET    /ideas                       list (filters: status, tags)
  GET    /ideas/:filename             get parsed idea
  POST   /ideas                       create new .md file
  PATCH  /ideas/:filename             update body and/or frontmatter
  DELETE /ideas/:filename             delete file

agents:
  POST   /agents/launch               resolve blueprint → create session → start
  GET    /agents/status               runtime pool status
  POST   /agents/kill/:sessionId      abort current run (session → active)

tools:
  GET    /tools                       list registered tools (name, schema, permissions)

code:
  GET    /code/search                 hybrid FTS + semantic search (?q=)
  GET    /code/file?path=             read file from cwd-relative path
  POST   /code/index                  trigger reindex

files:
  GET    /files/list?path=            directory listing (for cwd picker)
  GET    /files/stat?path=            file metadata
  GET    /files/read?path=            read file content (text only)

knowledge:
  GET    /knowledge/skills            list skills
  GET    /knowledge/conventions       list conventions
  GET    /knowledge/pointers          list pointers (filter by session)

events:
  GET    /events                      paginated events (cursor-based, filterable by type/session)

tabs:
  GET    /tabs                        list open tabs (server-persisted)
  POST   /tabs                        open new tab
  PATCH  /tabs/:id                    reorder, mark active
  DELETE /tabs/:id                    close tab

system:
  GET    /system/health               db health, pool status, costs
  GET    /system/config               read config kv
  PATCH  /system/config               update config kv
```

## Action Functions

All routers call into actions:

```typescript
// packages/core/src/actions/sessions.ts

export interface ActionContext {
  writeDb: Database;
  readDb: Database;
  runner: SessionRunner;
  registry: BlueprintRegistry;
  tools: ToolRegistry;
  events: EventBus;
}

export async function createSession(
  ctx: ActionContext,
  input: CreateSessionInput,
): Promise<Session> {
  const bp = input.blueprint
    ? ctx.registry.resolve(input.blueprint, input.cwd)
    : ctx.registry.getDefault();

  const session = createSessionRecord(ctx.writeDb, bp, input);
  ctx.events.emit({ type: 'session.created', sessionId: session.id });

  if (input.task) {
    await ctx.runner.launch(session.id, input.task);
  }

  return session;
}
```

The Hono router calls this with validated input. The MCP tool calls the same function. The raw HTTP route does too. Tests target `actions` directly.

## SSE

Two channels:

### Per-session: `/api/sse/sessions/:id`

```typescript
app.get('/api/sse/sessions/:id', (c) => {
  const id = c.req.param('id');
  const lastEventId = c.req.header('Last-Event-ID');

  return streamSSE(c, async (stream) => {
    if (lastEventId) {
      // replay missed events from `events` table
      await replayEvents(readDb, stream, { sessionId: id, sinceId: parseInt(lastEventId) });
    }
    const sub = events.subscribe({ sessionId: id });
    for await (const event of sub) {
      await stream.writeSSE({ id: String(event.id), event: event.type, data: JSON.stringify(event) });
    }
  });
});
```

### Global: `/api/sse/events`

Same shape, no session filter. Streams every event in `events` table to subscribers (UI uses for tab bar status, overview activity feed).

## MCP Exposure

```typescript
// packages/mcp/src/tools.ts

const tools: McpTool[] = [
  defineMcpTool({
    name: 'launch_agent',
    description: 'Launch an agent session from a blueprint',
    schema: z.object({
      blueprint: z.string(),
      task: z.string(),
      cwd: z.string().optional(),
      label: z.string().optional(),
    }),
    handler: (args) => actions.launchAgent(ctx, args),
  }),
  defineMcpTool({
    name: 'inject_message',
    description: 'Send a message to a running session',
    schema: z.object({ sessionId: z.string(), content: z.string() }),
    handler: (args) => actions.injectMessage(ctx, args),
  }),
  // ... one per significant action
];
```

MCP tool names are bare (`launch_agent`, not `moneypenny_launch_agent`); collision is the consumer's problem to namespace.

## Pagination

Cursor-based for time-ordered lists (messages, events, sessions):

```
GET /messages/by-session/:id?cursor=<seq>&limit=50&direction=before
```

Response:
```json
{
  "items": [...],
  "nextCursor": 1234,
  "hasMore": true
}
```

Limit-clamped at 200; default 50.

## Error Model

All routes return on error:

```typescript
{
  error: {
    code: string,        // e.g. 'SESSION_NOT_FOUND', 'BLUEPRINT_NOT_FOUND', 'BUDGET_EXCEEDED'
    message: string,     // human-readable
    details?: unknown,   // structured (e.g. validation issues)
  }
}
```

HTTP status mapping:
- `400` validation, bad input
- `404` not found
- `409` conflict (config_version mismatch, session in wrong state)
- `422` policy violation (BUDGET_EXCEEDED, PERMISSION_DENIED)
- `500` runtime / unexpected

Error codes are an exhaustive enum exported from `packages/core`:

```typescript
export const ErrorCodes = {
  SESSION_NOT_FOUND: 'SESSION_NOT_FOUND',
  SESSION_WRONG_STATE: 'SESSION_WRONG_STATE',
  CONFIG_VERSION_MISMATCH: 'CONFIG_VERSION_MISMATCH',
  BLUEPRINT_NOT_FOUND: 'BLUEPRINT_NOT_FOUND',
  BLUEPRINT_INVALID: 'BLUEPRINT_INVALID',
  TOOL_NOT_FOUND: 'TOOL_NOT_FOUND',
  PERMISSION_DENIED: 'PERMISSION_DENIED',
  BUDGET_EXCEEDED: 'BUDGET_EXCEEDED',
  IDEA_NOT_FOUND: 'IDEA_NOT_FOUND',
  // ...
} as const;
```

## Versioning

URL prefix `/api/v1/...` is reserved but unused in v2 (single version). When breaking changes ship, mount a parallel `/api/v2/` and announce a deprecation window for `/api/v1/`.

## OpenAPI

Hono can emit OpenAPI from Zod schemas via `@hono/zod-openapi`. Out of scope for v2 launch; revisit if external integrations request it.
