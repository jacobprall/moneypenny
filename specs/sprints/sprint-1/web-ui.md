# Web UI

### Technology

| Layer | Choice | Rationale |
|-------|--------|-----------|
| Framework | **React 19** + **Vite** | Familiar ecosystem, wide component availability |
| Styling | **Tailwind CSS** (build-time) | Utility classes, tree-shaken |
| State | **Zustand** | Minimal, works with WebSocket streams |
| Charts | **uPlot** (~35 KB) | GPU-accelerated, handles cost/latency charts |
| Icons | **Lucide** (tree-shaken) | Consistent line icons |
| Fonts | System stack + JetBrains Mono for code/data | No external font loads |

### Bundle budget (revised)

| Asset | Target | Hard limit |
|-------|--------|------------|
| JS (gzip) | 180 KB | 300 KB |
| CSS (gzip) | 15 KB | 30 KB |
| Total | ~200 KB | 350 KB |

React 19 is ~45 KB gzipped. With Zustand (~2 KB), uPlot (~35 KB), router
(~8 KB), and application code, 180 KB is realistic. The hard limit of
300 KB still delivers sub-second load on 3G.

### WebSocket protocol

Single WebSocket at `/api/v1/ws` multiplexing chat and observe:

```
Client → Server:
  {"type":"message", "sessionId":"...", "blueprint":"...", "text":"..."}
  {"type":"abort"}
  {"type":"subscribe", "channels":["events","costs","latency"]}
  {"type":"unsubscribe", "channels":["events"]}
  {"type":"ping"}

Server → Client:
  AgentEvent (streamed during agent run)
  {"type":"event", "event":{...}}       (observe subscription)
  {"type":"pong"}
```

**Reconnection protocol:** Client reconnects with exponential backoff
(1s, 2s, 4s, max 30s). On reconnect, client sends
`{"type":"resume", "lastEventId":"..."}`. Server replays missed events
from the event log if available, or sends `session_loaded` to resync.

### Pages

(Unchanged: Chat, Sessions, Agents, Jobs, Observe, Tune, System.)

### Tune page: configuration model

Settings on the Tune page are scoped:

| Setting | Scope | Storage |
|---------|-------|---------|
| Temperature, top-p, max tokens | Per-blueprint | Blueprint frontmatter `model_params:` |
| History depth, chunk retrieval count | Per-blueprint | Blueprint frontmatter `context:` |
| Cost cap, warning threshold | Per-blueprint (overridable global) | Blueprint `guardrails:` / global `config` table |
| Max turns, parallel tools | Global | `config` table |
| Sub-agent depth limit | Global | `config` table |

Changes made on the Tune page write to the `config` table (global) or
trigger a blueprint reload (per-blueprint). Changes take effect on the
next agent run, not mid-session.

### Authentication

Localhost mode (default): random bearer token generated on `mp serve`
startup, printed to terminal, saved to `.mp/serve-token`. Browser stores in
`localStorage` after one-time paste. WebSocket authenticates via the
first message: `{"type":"auth", "token":"..."}`.

### Build integration

(Unchanged: `apps/web/` structure, `mp serve` serves `dist/`, `--dev` proxy.)

### Acceptance criteria

- [ ] `mp serve` opens web UI at `http://localhost:1745` with auth flow
- [ ] Chat page streams responses with tool call expansion
- [ ] Sessions page lists, searches, deletes, exports sessions
- [ ] Jobs page shows all job types with run history and trigger buttons
- [ ] Observe page shows live event stream and cost charts
- [ ] Tune page persists settings to config table / blueprint frontmatter
- [ ] Command palette (`Cmd+K`) navigates between pages
- [ ] Total JS bundle < 300 KB gzipped
- [ ] WebSocket reconnects automatically within 5s of disconnect

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 2.0 | App scaffold: Vite + React + Tailwind + router + Zustand + layout shell | 1 day |
| 2.1 | Chat page + WebSocket streaming + tool call display | 3 days |
| 2.2 | Sessions page (data table, search, delete, export) | 2 days |
| 2.3 | Agents page (blueprint catalog, capability tree) | 2 days |
| 2.4 | Jobs page (all job types, run history, trigger/toggle) | 2 days |
| 2.5 | Observe page (event stream, cost tracker, token usage) | 2 days |
| 2.6 | Tune page (model params, context, cost controls, loop config) | 1.5 days |
| 2.7 | System page (config editor, policy viewer, index health, skills) | 1.5 days |
| 2.8 | Command palette, keyboard shortcuts, auth flow | 1 day |
| 2.9 | Static asset serving from `@moneypenny/http`, `--dev` proxy mode | 1 day |
