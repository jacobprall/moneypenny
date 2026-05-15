# web-ui — Agent View & Management Surface

> The primary interface for moneypenny. An agent-view-first web application
> served directly by `mp serve`. Chat with governed agents, stream their
> reasoning and tool calls inline, and manage every resource in the system —
> sessions, blueprints, policies, skills, memories, and scheduled jobs.
>
> Ported from `moneypenny-rs/docs/specs/sprint-3/web-ui.md`, redesigned
> around a Cursor-style agent view as the MVP centerpiece.

**Prerequisites:** Sprint 1 complete (AgentBridge event protocol, job
system, blueprints). Sprint 2 §1 (embeddings) and §3 (context pipeline)
are beneficial but not blocking.

---

## First principles

### The agent view IS the product

Cursor's agent mode proved that a single, linear conversation with
inline tool execution is the right primitive for AI-assisted work. Not a
dashboard with a chat sidebar. Not a code editor with an AI panel. The
conversation itself — user messages, agent reasoning, tool calls rendered
inline, results folded beneath — is the entire interaction surface.

Moneypenny's agent view follows this model but extends it with the
features that make moneypenny different: governance decisions visible
inline, cost tracking ambient in the status bar, session memory
retrievals shown as context blocks, and policy denials feeding back into
the conversation rather than crashing.

### What "no code diffing" means for MVP

The agent view renders tool call results as structured, collapsible
blocks — not as rich editor components. A `file_write` result shows the
file path, a success/failure badge, and the raw content on expand. A
`shell_exec` result shows the command and output. A `code_search` result
shows the matched files and snippets. No syntax-highlighted diff viewer,
no split-pane editor, no inline annotations. These are post-MVP
enhancements.

### Management views are CRUD, not dashboards

Every non-chat view follows one pattern: a searchable list with row
actions, and a detail panel that slides over or expands inline. No
charts, no histograms, no real-time graphs for MVP. The observe page
with cost charts and latency panels is post-MVP. What ships now is the
ability to **find, inspect, create, edit, delete, and trigger** every
resource in the system.

---

## Design philosophy

1. **Agent-view-first.** The chat page is the default route, the largest
   surface, and the most polished component. Everything else is
   secondary navigation.

2. **Zero-install.** `mp serve` opens the UI. The Bun process serves
   static assets from a bundled `dist/` directory. No separate frontend
   deploy.

3. **Minimalist chrome, maximum signal.** Sparse layout, generous
   whitespace, monospace for data. The UI should feel like a well-tuned
   terminal wrapped in a clean shell.

4. **Progressive disclosure.** Tool calls are collapsed by default,
   showing name + status badge. Expand for full input/output. Governance
   decisions show as inline badges on tool calls — expand for the policy
   trail. Cost is a number in the status bar — not a chart.

5. **Keyboard-first.** `Cmd+K` command palette for navigation. `Cmd+N`
   for new session. `Cmd+Enter` to send. `Cmd+.` to abort. Every action
   reachable without a mouse.

6. **Fast by construction.** <300 KB gzipped bundle. WebSocket for
   real-time streaming. Virtual scrolling for long lists. No layout
   shift on token stream.

---

## Technology stack

| Layer | Choice | Rationale |
|-------|--------|-----------|
| Framework | **React 19** + **Vite** | Familiar ecosystem, matches sprint-1. React 19 for `use()`, transitions, and concurrent features. |
| Components | **shadcn/ui** (Radix UI + Tailwind) | Copy-paste component library. Full ownership of source. Accessible primitives (Dialog, Dropdown, Command, Sheet, Table, Collapsible, Tooltip, Toggle, Badge, etc.) without external runtime deps. |
| Styling | **Tailwind CSS v4** (build-time) | Utility classes, tree-shaken. shadcn/ui components use Tailwind + `cn()` utility. |
| Server state | **TanStack Query** | Hooks-first data fetching with caching, deduplication, background refetch. Every REST call wrapped in a `useQuery` / `useMutation` hook. |
| Client state | **Zustand** | ~2 KB. Owns WebSocket connection state, active chat stream, UI preferences. Does NOT own server-fetched data (that's TanStack Query). |
| Router | **TanStack Router** | Type-safe routing, loader/action pattern, search param state. Pairs naturally with TanStack Query for route-level data deps. |
| Markdown | **react-markdown** + **rehype-highlight** | Agent responses rendered as markdown with syntax highlighting |
| Icons | **Lucide** (tree-shaken) | Consistent line icons, ~200 bytes each. shadcn/ui default icon set. |
| Fonts | System stack + **JetBrains Mono** (code/data) | No external font loads. Monospace for all tool output. |

### Why shadcn/ui

shadcn/ui is not a dependency — it's a code generation tool. You run
`npx shadcn@latest add button` and get a React component file you own.
No `node_modules` lock-in, no version mismatches, full control over
styling and behavior. The components are built on Radix UI primitives
(accessible, composable, unstyled) with Tailwind classes applied.

For this project, shadcn/ui eliminates the need to build from scratch:
- **Command** (cmdk) — command palette (`Cmd+K`)
- **Dialog / AlertDialog** — confirmations, destructive actions
- **Sheet** — slide-over detail panels
- **Collapsible** — tool call expand/collapse
- **Table** — sortable data tables for management views
- **Badge** — status, cost, effect badges
- **DropdownMenu** — context menus, blueprint selector
- **Tooltip** — keyboard shortcut hints
- **Toggle / Switch** — enable/disable controls
- **Textarea** — auto-growing chat input
- **ScrollArea** — virtual scrolling for message lists
- **Tabs** — tabbed views in detail panels
- **Separator** — visual dividers

### Architecture: hooks-first, Vercel best practices

The frontend follows a strict separation between data and UI:

1. **Server state lives in TanStack Query hooks.** Every REST resource
   gets a pair: `useQuery` for reads, `useMutation` for writes. Hooks
   handle caching, deduplication, stale-while-revalidate, and optimistic
   updates. Components never call `fetch()` directly.

2. **Client state lives in Zustand slices.** WebSocket connection,
   streaming token buffer, expanded tool calls, UI preferences. These
   are ephemeral, not server-derived.

3. **WebSocket state bridges into both.** The `useWebSocket` hook
   manages the connection and dispatches incoming events to either
   Zustand (streaming tokens, tool call state) or TanStack Query
   cache invalidation (session list refresh on new session).

4. **Components are thin.** A page component composes hooks + shadcn/ui
   primitives. Business logic lives in hooks, not in components.
   Components own layout and conditional rendering, nothing else.

5. **Suspense + Error Boundaries.** Each route wrapped in `<Suspense>`
   for loading states and `<ErrorBoundary>` for error handling.
   TanStack Query's `suspense: true` option enables this cleanly.

### Bundle budget

| Asset | Target | Hard limit |
|-------|--------|------------|
| JS (gzip) | 180 KB | 280 KB |
| CSS (gzip) | 18 KB | 30 KB |
| Fonts | 0 KB (system) | 40 KB (JetBrains Mono subset) |
| **Total** | **~200 KB** | **350 KB** |

Slightly larger than the original budget due to TanStack Query (~12 KB)
and Radix primitives (~2-4 KB per component used). Offset by not
building custom implementations of the same functionality.

---

## Visual language

### Color system

Dark theme only for MVP. Light theme is post-MVP.

```
Background ───── neutral-950
Surface ──────── neutral-900
Surface raised ─ neutral-800
Border ────────── neutral-700
Text primary ─── neutral-50
Text secondary ─ neutral-400
Accent ────────── indigo-400
Success ──────── emerald-400
Warning ──────── amber-400
Danger ────────── rose-400
Cost indicator ─ amber-500 (always — visual anchor for money)
```

### Typography

```
Headings ─── system sans, 600 weight, tight tracking
Body ──────── system sans, 400 weight, normal tracking
Code/data ── JetBrains Mono / ui-monospace, 400 weight
Numbers ───── tabular-nums (monospace digits for alignment)
```

---

## Navigation structure

```
┌──────────────────────────────────────────────────────┐
│ ⌘K command palette                                    │
├──────────┬───────────────────────────────────────────┤
│          │                                           │
│  Chat    │   [active page content]                   │
│  Sessions│                                           │
│  ─────── │                                           │
│  Agents  │                                           │
│  Policies│                                           │
│  Skills  │                                           │
│  ─────── │                                           │
│  Schedule│                                           │
│  ─────── │                                           │
│  ⚙ Sys   │                                           │
│          │                                           │
├──────────┴───────────────────────────────────────────┤
│ status bar: ws ● | session: abc | cost: $0.012 | ▸   │
└──────────────────────────────────────────────────────┘
```

### Routes

| Route | Nav label | Purpose |
|-------|-----------|---------|
| `/` | **Chat** | Agent view. The primary interface. |
| `/sessions` | **Sessions** | Browse, search, resume, delete, export sessions. |
| `/agents` | **Agents** | Blueprint catalog. View, enable/disable, quick-launch. |
| `/policies` | **Policies** | Policy list. View, create, edit, toggle. |
| `/skills` | **Skills** | Skill catalog. View, edit, delete extracted skills. |
| `/schedule` | **Schedule** | Cron jobs. Enable/disable, trigger, view run history. |
| `/system` | **Sys** | Config editor, index health, about. |

### Sidebar

- Collapsible. 220px expanded, 48px icon-only.
- Active route highlighted.
- Keyboard: `Cmd+1` through `Cmd+7` jump to each page.

### Command palette (`Cmd+K`)

Fuzzy-searchable list of every action:

- **Navigate:** "Go to sessions", "Go to agents", ...
- **Create:** "New chat", "New session with [blueprint]", ...
- **Actions:** "Abort current run", "Export session", "Trigger job [name]", ...
- **Search:** "Search sessions for [query]", "Search skills for [query]", ...

---

## Page specifications

### 1. Chat (`/`) — The Agent View

The centerpiece. A Cursor-style linear conversation with inline tool
execution. This is where 90% of user time is spent.

#### Layout

```
┌──────────┬───────────────────────────────────────────────┐
│ Sessions │  agent: research-assistant                     │
│          │  model: claude-sonnet-4 · temp 0.7             │
│ ▸ active │ ──────────────────────────────────────────────  │
│   sess-a │                                               │
│   sess-b │  [user message]                               │
│   sess-c │                                               │
│          │  [assistant message streaming...]              │
│          │                                               │
│          │    ┌─ code_search ────────────────────────┐   │
│          │    │  query: "auth middleware"              │   │
│          │    │  ✓ 4 results · 12ms                   │   │
│          │    └───────────────────────────────────────┘   │
│          │                                               │
│          │    ┌─ file_write ─────────────────────────┐   │
│          │    │  src/middleware/auth.ts                │   │
│          │    │  ✓ written · 24 lines                 │   │
│          │    └───────────────────────────────────────┘   │
│          │                                               │
│          │  [assistant continues reasoning...]            │
│          │                                               │
│          │    ┌─ shell_exec ─────────────────── ✗ denied │
│          │    │  rm -rf /tmp/cache                     │   │
│          │    │  Policy: no-destructive-shell           │   │
│          │    └───────────────────────────────────────┘   │
│          │                                               │
│          │  I can't delete the cache — your policy        │
│          │  protects it. Instead, I'll clear individual   │
│          │  entries...                                    │
│          │                                               │
│          │ ──────────────────────────────────────────────  │
│ + new    │  [input area]                      ⌘⏎ Send   │
│          │  blueprint: ▾  abort: ⌘.                      │
└──────────┴───────────────────────────────────────────────┘
```

#### Message rendering

- **User messages:** Left-aligned, plain text, subtle background
  distinction. Minimal chrome.

- **Assistant messages:** Left-aligned, full markdown rendering via
  react-markdown. Code blocks with syntax highlighting. Streamed
  token-by-token with a blinking cursor. The message container grows
  downward, auto-scrolls unless the user has scrolled up.

- **Tool calls (collapsed — default after completion):**
  ```
  ┌─ tool_name ──────────────────────────── status_badge ┐
  │  one-line summary (query text, file path, command)    │
  └──────────────────────────────────────────────────────┘
  ```
  Status badges:
  - `✓` green — success
  - `✗` red — failed
  - `⊘ denied` red — blocked by policy
  - `⚠ audit` yellow — allowed but logged
  - `⏳` amber pulse — running

- **Tool calls (expanded — click to toggle):**
  ```
  ┌─ code_search ──────────────────────────────── ✓ 12ms ┐
  │                                                       │
  │  Input                                                │
  │  ┌──────────────────────────────────────────────────┐ │
  │  │ query: "how does auth middleware work"            │ │
  │  │ limit: 5                                         │ │
  │  └──────────────────────────────────────────────────┘ │
  │                                                       │
  │  Output                                               │
  │  ┌──────────────────────────────────────────────────┐ │
  │  │ src/middleware/auth.ts       0.94 ████            │ │
  │  │   verifyToken(): lines 12-45                     │ │
  │  │ src/routes/login.ts         0.87 ███▊            │ │
  │  │   handleLogin(): lines 8-32                      │ │
  │  └──────────────────────────────────────────────────┘ │
  │                                                       │
  │  Governance: ✓ allowed (no policy matched)            │
  └──────────────────────────────────────────────────────┘
  ```
  Full input as formatted key-value pairs (not raw JSON).
  Full output as structured content (not raw JSON).
  Governance trail shown at the bottom of every expanded tool call.

- **Policy denials inline:** When a tool call is denied, the denial
  reason renders inline in the conversation flow. The agent's next
  message references the denial and adapts. This is the key UX
  differentiator — governance is visible, not hidden.

- **Errors:** Red left-border, monospace content.

#### Session sidebar

- Sorted by last activity (most recent first).
- Each entry: truncated label, agent name badge, relative time, cost.
- Right-click context menu: rename, export, delete.
- Click to switch sessions. Active session highlighted.
- `Cmd+N` creates a new session (opens blueprint picker dropdown).

#### Input area

- Multiline textarea. `Enter` sends, `Shift+Enter` for newline.
- Blueprint selector dropdown — filters available agents.
- Abort button visible during active runs (`Cmd+.` shortcut).
- Token estimate badge: shows estimated input tokens (updated on
  debounce).

#### Real-time behavior

- WebSocket streams tokens, tool calls, governance decisions, and cost
  updates.
- Token-by-token append with cursor indicator. No layout shift.
- Cost accumulator in status bar updates live during streaming.
- Connection status indicator (green dot / red dot).

---

### 2. Sessions (`/sessions`)

A searchable, sortable table for power users who need to find, review,
and manage past sessions.

#### List view

| Column | Content | Sortable | Filterable |
|--------|---------|----------|------------|
| Label | Session name / first message preview | ✓ | Full-text search |
| Agent | Blueprint name | ✓ | Dropdown |
| Messages | Message count | ✓ | — |
| Cost | Total session cost | ✓ | — |
| Last active | Relative timestamp | ✓ | — |
| Status | Active / idle / error | ✓ | Multi-select |

#### Actions

| Action | Trigger | Description |
|--------|---------|-------------|
| Resume | Click row / `Enter` | Opens session in Chat view |
| Rename | Inline edit / `r` | Edit session label |
| Export | `e` / menu | Export as Markdown or JSON |
| Delete | `d` / menu | Delete with confirmation |
| Bulk delete | Checkbox + action | Select multiple, delete |

#### Search

Full-text search across message content (backed by `messages_fts`).
Search input at the top of the table with debounced query.

---

### 3. Agents (`/agents`)

Blueprint catalog with operational controls. Every agent definition
that lives in `.mp/agents/` is listed here.

#### List view

```
┌────────────────────────────────────────────────────────┐
│ research-assistant                       ● enabled      │
│ Claude Sonnet 4 · 5 tools · research strategy           │
│ "Full-stack research assistant with web access..."      │
│                                                         │
│ [Launch chat]  [View definition]  [Enable/Disable]      │
│                                                         │
│ Last 7 days: 14 sessions · $2.34 · 89k tokens          │
├────────────────────────────────────────────────────────┤
│ code-reviewer                            ● enabled      │
│ Claude Sonnet 4 · 3 tools · standard                    │
│ "Code review agent for pull requests..."                │
│                                                         │
│ [Launch chat]  [View definition]  [Enable/Disable]      │
│                                                         │
│ Last 7 days: 8 sessions · $1.12 · 52k tokens           │
└────────────────────────────────────────────────────────┘
```

#### Detail panel (slide-over)

- Full blueprint markdown source (read-only, syntax highlighted).
- **Capability tree:**
  ```
  research-assistant
  ├── model: claude-sonnet-4
  ├── strategy: research (max 5 iterations)
  ├── tools (5)
  │   ├── code_search
  │   ├── file_read
  │   ├── file_write
  │   ├── shell_exec
  │   └── web_fetch
  ├── sub-agents (1)
  │   └── fact-checker (claude-haiku)
  │       └── tools: web_fetch
  ├── guardrails
  │   ├── max cost: $0.50
  │   └── max turns: 15
  └── memory
      ├── context: "research"
      └── extract: true
  ```
- Tool list with descriptions.
- Sub-agent graph (simple tree).
- Policy summary: which policies apply.
- Usage stats: sessions, cost, tokens (7d / 30d / all time).

#### Actions

| Action | Trigger |
|--------|---------|
| Launch chat | Button — creates new session and navigates to `/` |
| View definition | Button — opens slide-over with full blueprint |
| Enable/Disable | Toggle — enables/disables the agent |

---

### 4. Policies (`/policies`)

CRUD interface for the governance policy engine. Policies are YAML files
in `.mp/policies/` — this view provides a UI over them.

#### List view

| Column | Content |
|--------|---------|
| Name | Policy name |
| Effect | `allow` / `deny` / `warn` / `audit` — color-coded |
| Priority | Numeric priority |
| Tool pattern | Which tools this policy matches |
| Path pattern | Which file paths this policy matches |
| Enabled | Toggle |
| Hit count | How many times evaluated (all time) |

#### Detail panel

- Full policy YAML source (syntax highlighted).
- Evaluation history: last N times this policy was triggered, with
  the tool call, session, and outcome.
- Edit capability: modify policy YAML inline and save (writes back
  to the YAML file).

#### Actions

| Action | Trigger |
|--------|---------|
| Create | "New policy" button — YAML editor with template |
| Edit | Click row — opens detail panel with editable YAML |
| Toggle | Enable/disable switch on each row |
| Delete | Menu action with confirmation |

---

### 5. Skills (`/skills`)

Browse and manage extracted skills — the learned knowledge that
persists across sessions.

#### List view

| Column | Content |
|--------|---------|
| Name | Skill name |
| Description | One-line description |
| Source | `learned` / `user` / `detected` |
| Files | Number of associated files |
| Last used | Relative timestamp |
| Usage count | How many sessions loaded this skill |

#### Detail panel

- Full skill instructions (markdown rendered).
- Associated files list (`skill_files`).
- Source session link (if learned).
- Edit capability: modify instructions, add/remove files.

#### Actions

| Action | Trigger |
|--------|---------|
| Edit | Click row — opens detail with editable instructions |
| Delete | Menu action with confirmation |
| View source session | Link to the session that extracted this skill |

---

### 6. Schedule (`/schedule`)

Dashboard for cron-scheduled agent jobs.

#### List view

| Column | Content |
|--------|---------|
| Job name | Blueprint name + description |
| Cron | Cron expression + human-readable ("Every weekday at 9am") |
| Next run | Countdown timer (live) |
| Last run | Timestamp + status (✓/✗) + cost |
| Status | Enabled / disabled / running |

#### Run history (expandable per-job)

Table of past runs: start time, duration, status, cost, output preview.
Click a run to open its session in Chat view (read-only replay).

#### Actions

| Action | Trigger |
|--------|---------|
| Trigger now | Button — runs the job immediately |
| Enable/Disable | Toggle switch |
| View history | Expand row to see past runs |
| Open run session | Click a run → opens in Chat |

---

### 7. System (`/system`)

Low-level configuration and health.

#### Config editor

Key-value editor for the `config` table. Shows all entries grouped by
namespace (`llm.*`, `index.*`, `ui.*`, etc.). Editable inline with type
validation.

#### Index health

| Metric | Value |
|--------|-------|
| Total files indexed | 1,247 |
| Total chunks | 8,934 |
| Stale chunks | 12 |
| Last full index | 2 hours ago |
| Last incremental index | 3 minutes ago |
| Index size on disk | 24.3 MB |

Actions: re-index now, prune stale chunks.

---

## WebSocket protocol

Single WebSocket at `/api/v1/ws` multiplexing chat streaming and
system events.

### Client → Server

```jsonc
// Send a message to the agent
{"type": "message", "session_id": "...", "blueprint": "...", "text": "..."}

// Abort the current agent run
{"type": "abort"}

// Subscribe to event categories (for observe features, post-MVP)
{"type": "subscribe", "channels": ["events", "costs"]}

// Keepalive
{"type": "ping"}
```

### Server → Client

```jsonc
// Streaming tokens from the agent
{"type": "token", "data": "..."}

// Tool call lifecycle
{"type": "tool_call_start", "id": "...", "name": "...", "args": {}}
{"type": "tool_call_result", "id": "...", "name": "...", "result": {}, "success": true, "duration_ms": 144}

// Governance decisions (attached to tool calls)
{"type": "governance", "tool_call_id": "...", "effect": "deny", "policy_name": "no-destructive-shell", "reason": "Destructive shell commands require approval"}

// Cost tracking
{"type": "cost_update", "session_cost_usd": 0.042, "turn_cost_usd": 0.003}

// Turn lifecycle
{"type": "turn_complete", "usage": {"input_tokens": 1200, "output_tokens": 847}, "cost_usd": 0.003, "duration_ms": 2100}

// Session management
{"type": "session_loaded", "session": {}}

// Errors
{"type": "error", "code": "...", "message": "..."}

// Keepalive
{"type": "pong"}
```

### Reconnection protocol

Client reconnects with exponential backoff (1s, 2s, 4s, max 30s). On
reconnect, sends `{"type": "resume", "last_event_id": "..."}`. Server
replays missed events from the event log if available, or sends
`session_loaded` to resync.

---

## REST API surface

All endpoints under `/api/v1/`. Used by the management views for CRUD
operations. The chat interface uses WebSocket exclusively.

### Sessions

```
GET    /api/v1/sessions                  List sessions (paginated, filterable)
POST   /api/v1/sessions                  Create new session
GET    /api/v1/sessions/:id              Get session details
PATCH  /api/v1/sessions/:id              Update session (rename)
DELETE /api/v1/sessions/:id              Delete session
GET    /api/v1/sessions/:id/messages     Get session messages (paginated)
POST   /api/v1/sessions/:id/export       Export session (markdown/JSON)
```

### Blueprints

```
GET    /api/v1/blueprints                List blueprints
GET    /api/v1/blueprints/:name          Get blueprint details + stats
PATCH  /api/v1/blueprints/:name          Update (enable/disable)
```

### Policies

```
GET    /api/v1/policies                  List all policies
GET    /api/v1/policies/:id              Get policy details + hit count
POST   /api/v1/policies                  Create new policy
PUT    /api/v1/policies/:id              Update policy
DELETE /api/v1/policies/:id              Delete policy
PATCH  /api/v1/policies/:id              Toggle enabled/disabled
```

### Skills

```
GET    /api/v1/skills                    List skills
GET    /api/v1/skills/:name              Get skill details + files
PUT    /api/v1/skills/:name              Update skill instructions/files
DELETE /api/v1/skills/:name              Delete skill
```

### Schedule

```
GET    /api/v1/schedule/jobs             List scheduled jobs
POST   /api/v1/schedule/jobs/:id/trigger Trigger job now
PATCH  /api/v1/schedule/jobs/:id         Enable/disable job
GET    /api/v1/schedule/jobs/:id/runs    List job run history
```

### Config

```
GET    /api/v1/config                    List all config entries
GET    /api/v1/config/:key               Get config value
PUT    /api/v1/config/:key               Set config value
DELETE /api/v1/config/:key               Delete config entry
```

### Search (for code index)

```
POST   /api/v1/search                    Hybrid search (BM25 + vector)
GET    /api/v1/index/health              Index health stats
POST   /api/v1/index/reindex             Trigger re-index
POST   /api/v1/index/prune              Prune stale chunks
```

---

## Authentication

### Localhost mode (default)

On `mp serve` startup, a random bearer token is generated and printed
to the terminal. Also written to `.mp/serve-token`.

```
$ mp serve
  moneypenny web UI → http://127.0.0.1:1745
  auth token → mp_sk_a1b2c3d4e5f6...
  (saved to .mp/serve-token)
```

Browser stores the token in `localStorage` after the user pastes it
into a one-time login prompt. WebSocket authenticates via the first
message: `{"type": "auth", "token": "..."}`.

### Auth bypass

`--no-auth` disables authentication. For trusted networks and local
development.

---

## Frontend directory structure

```
apps/web/
├── package.json
├── tsconfig.json
├── vite.config.ts
├── components.json           shadcn/ui configuration
├── index.html
├── src/
│   ├── main.tsx              Entry point, providers, router
│   ├── routes.tsx            TanStack Router route tree
│   │
│   ├── lib/
│   │   ├── utils.ts          cn() utility, formatters, constants
│   │   ├── api-client.ts     Typed fetch wrapper (base URL, auth headers, error handling)
│   │   └── ws-client.ts      WebSocket client class (connect, reconnect, parse, dispatch)
│   │
│   ├── hooks/
│   │   ├── use-websocket.ts      WebSocket lifecycle (connect, reconnect, auth, event dispatch)
│   │   ├── use-chat-stream.ts    Streaming state: token buffer, tool calls, cost, abort
│   │   ├── use-sessions.ts       useQuery/useMutation for sessions CRUD + search
│   │   ├── use-blueprints.ts     useQuery/useMutation for blueprints list + toggle
│   │   ├── use-policies.ts       useQuery/useMutation for policies CRUD
│   │   ├── use-skills.ts         useQuery/useMutation for skills CRUD
│   │   ├── use-schedule.ts       useQuery/useMutation for jobs CRUD + trigger
│   │   ├── use-config.ts         useQuery/useMutation for config key-value CRUD
│   │   ├── use-index-health.ts   useQuery for index health stats + reindex/prune mutations
│   │   ├── use-hotkeys.ts        Keyboard shortcut registration + context-aware dispatch
│   │   └── use-command.ts        Command palette action registry + fuzzy search
│   │
│   ├── stores/
│   │   ├── chat-store.ts     Zustand: streaming tokens, active session, tool call UI state
│   │   ├── ui-store.ts       Zustand: sidebar collapsed, theme, density preferences
│   │   └── ws-store.ts       Zustand: connection status, reconnect state
│   │
│   ├── pages/
│   │   ├── chat.tsx          Agent view — composes hooks + chat components
│   │   ├── sessions.tsx      Sessions CRUD — useSessions + DataTable
│   │   ├── agents.tsx        Blueprint catalog — useBlueprints + cards
│   │   ├── policies.tsx      Policy management — usePolicies + DataTable + Sheet editor
│   │   ├── skills.tsx        Skill catalog — useSkills + DataTable + Sheet editor
│   │   ├── schedule.tsx      Cron jobs — useSchedule + DataTable + run history
│   │   └── system.tsx        Config + health — useConfig + useIndexHealth
│   │
│   ├── components/
│   │   ├── ui/               shadcn/ui generated components (owned source)
│   │   │   ├── badge.tsx
│   │   │   ├── button.tsx
│   │   │   ├── collapsible.tsx
│   │   │   ├── command.tsx       (cmdk-based command palette)
│   │   │   ├── dialog.tsx
│   │   │   ├── alert-dialog.tsx
│   │   │   ├── dropdown-menu.tsx
│   │   │   ├── input.tsx
│   │   │   ├── scroll-area.tsx
│   │   │   ├── separator.tsx
│   │   │   ├── sheet.tsx         (slide-over panel)
│   │   │   ├── switch.tsx
│   │   │   ├── table.tsx
│   │   │   ├── tabs.tsx
│   │   │   ├── textarea.tsx
│   │   │   └── tooltip.tsx
│   │   │
│   │   ├── layout/
│   │   │   ├── app-shell.tsx     Root layout: sidebar + main + status bar
│   │   │   ├── sidebar.tsx       Navigation sidebar (uses shadcn Button, Separator, Tooltip)
│   │   │   ├── status-bar.tsx    Bottom status bar (ws dot, session, cost, iteration)
│   │   │   └── command-palette.tsx  Cmd+K palette (uses shadcn Command)
│   │   │
│   │   ├── chat/
│   │   │   ├── message-list.tsx  Virtualized message list (uses ScrollArea)
│   │   │   ├── user-message.tsx  User message bubble
│   │   │   ├── agent-message.tsx Agent message with markdown rendering
│   │   │   ├── tool-call.tsx     Collapsible tool call block (uses Collapsible, Badge)
│   │   │   ├── governance-badge.tsx  Policy decision badge on tool calls
│   │   │   ├── chat-input.tsx    Auto-growing textarea + blueprint selector + abort (uses Textarea, DropdownMenu)
│   │   │   ├── session-sidebar.tsx  Left panel session list (uses ScrollArea)
│   │   │   └── streaming-cursor.tsx Blinking cursor during token stream
│   │   │
│   │   └── manage/
│   │       ├── data-table.tsx    Reusable sortable/filterable table (uses shadcn Table)
│   │       ├── detail-sheet.tsx  Slide-over detail panel (uses Sheet)
│   │       ├── confirm-delete.tsx Destructive action dialog (uses AlertDialog)
│   │       ├── empty-state.tsx   Empty state placeholder
│   │       └── resource-search.tsx Debounced search input for any resource list
│   │
│   └── styles/
│       └── globals.css       Tailwind directives + shadcn/ui CSS variables
│
└── dist/                     Build output (served by mp serve)
```

### Key architectural decisions

**`hooks/` is the brain.** Every hook is a self-contained unit that owns
a data domain. `useSessions()` returns `{ sessions, isLoading, create,
rename, remove, search }`. A page component calls the hook, destructures
what it needs, and passes data into shadcn/ui primitives. The hook
handles caching, optimistic updates, and error states internally.

**`stores/` is minimal.** Zustand only owns state that is purely
client-side and ephemeral: the WebSocket connection, the streaming token
buffer, UI preferences. Anything that comes from the server goes through
TanStack Query hooks — no Zustand for server state.

**`components/ui/` is shadcn/ui output.** These files are generated by
`npx shadcn@latest add <component>` and committed to the repo. They are
the only files in the project that follow shadcn's conventions (Radix +
`cn()` + CSS variables). Customizations happen in these files directly.

**`components/chat/` and `components/manage/` are composed.** They
import from `components/ui/` and wire up domain-specific behavior.
`tool-call.tsx` uses `Collapsible` + `Badge` from ui/. `data-table.tsx`
uses `Table` from ui/. No component in these directories imports Radix
directly.

---

## State management

### Principle: server state vs client state

| Concern | Owner | Why |
|---------|-------|-----|
| Session list, session messages | TanStack Query (`useSessions`) | Server-derived, cacheable, benefits from stale-while-revalidate |
| Blueprint catalog | TanStack Query (`useBlueprints`) | Server-derived |
| Policies, skills, config | TanStack Query (`usePolicies`, `useSkills`, `useConfig`) | Server-derived CRUD |
| Schedule + run history | TanStack Query (`useSchedule`) | Server-derived |
| WebSocket connection | Zustand (`ws-store`) | Ephemeral client state |
| Streaming tokens + tool calls | Zustand (`chat-store`) | Real-time, write-heavy, not REST-fetched |
| UI preferences (sidebar, theme) | Zustand (`ui-store`) | Client-only, persisted to localStorage |

### Zustand stores (client-only state)

```typescript
// stores/ws-store.ts
interface WsStore {
  status: 'connecting' | 'connected' | 'disconnected';
  reconnectAttempt: number;
  setStatus: (status: WsStore['status']) => void;
}

// stores/chat-store.ts
interface ChatStore {
  activeSessionId: string | null;
  isAgentRunning: boolean;
  streamingContent: string;
  sessionCostUsd: number;
  turnCostUsd: number;
  iteration: number;
  maxIterations: number;

  messages: ChatMessage[];
  toolCalls: Map<string, ToolCallState>;
  expandedToolCalls: Set<string>;

  // Actions
  appendToken: (token: string) => void;
  startToolCall: (id: string, name: string, args: Record<string, unknown>) => void;
  completeToolCall: (id: string, result: unknown, success: boolean, durationMs: number) => void;
  attachGovernance: (toolCallId: string, decision: GovernanceDecision) => void;
  completeTurn: (usage: TokenUsage, costUsd: number) => void;
  toggleToolCall: (id: string) => void;
  setActiveSession: (id: string | null) => void;
  reset: () => void;
}

// stores/ui-store.ts
interface UiStore {
  sidebarCollapsed: boolean;
  toggleSidebar: () => void;
}
```

### TanStack Query hooks (server state)

Each hook encapsulates a complete data domain. Components never call
`fetch()` or manage loading/error states manually.

```typescript
// hooks/use-sessions.ts
function useSessions(filters?: SessionFilters) {
  return useQuery({
    queryKey: ['sessions', filters],
    queryFn: () => api.listSessions(filters),
  });
}

function useSession(id: string) {
  return useQuery({
    queryKey: ['sessions', id],
    queryFn: () => api.getSession(id),
  });
}

function useSessionMessages(id: string) {
  return useQuery({
    queryKey: ['sessions', id, 'messages'],
    queryFn: () => api.getSessionMessages(id),
  });
}

function useCreateSession() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (blueprint: string) => api.createSession(blueprint),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['sessions'] }),
  });
}

function useDeleteSession() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => api.deleteSession(id),
    onSettled: () => queryClient.invalidateQueries({ queryKey: ['sessions'] }),
  });
}

function useRenameSession() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ id, label }: { id: string; label: string }) =>
      api.renameSession(id, label),
    onMutate: async ({ id, label }) => {
      // Optimistic update
      await queryClient.cancelQueries({ queryKey: ['sessions'] });
      queryClient.setQueryData(['sessions'], (old: Session[]) =>
        old?.map(s => s.id === id ? { ...s, label } : s)
      );
    },
  });
}

function useExportSession() {
  return useMutation({
    mutationFn: ({ id, format }: { id: string; format: 'markdown' | 'json' }) =>
      api.exportSession(id, format),
  });
}
```

```typescript
// hooks/use-policies.ts — same pattern for every resource
function usePolicies() {
  return useQuery({ queryKey: ['policies'], queryFn: api.listPolicies });
}

function useCreatePolicy() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (yaml: string) => api.createPolicy(yaml),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['policies'] }),
  });
}

function useUpdatePolicy() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, yaml }: { id: string; yaml: string }) =>
      api.updatePolicy(id, yaml),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['policies'] }),
  });
}

function useTogglePolicy() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, enabled }: { id: string; enabled: boolean }) =>
      api.togglePolicy(id, enabled),
    onMutate: async ({ id, enabled }) => {
      queryClient.setQueryData(['policies'], (old: Policy[]) =>
        old?.map(p => p.id === id ? { ...p, enabled } : p)
      );
    },
  });
}

function useDeletePolicy() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => api.deletePolicy(id),
    onSettled: () => qc.invalidateQueries({ queryKey: ['policies'] }),
  });
}
```

### WebSocket ↔ state bridge

The `useWebSocket` hook is the bridge between real-time events and both
state systems:

```typescript
// hooks/use-websocket.ts
function useWebSocket(token: string) {
  const chatStore = useChatStore();
  const wsStore = useWsStore();
  const queryClient = useQueryClient();

  useEffect(() => {
    const ws = new WsClient('/api/v1/ws', token);

    ws.on('connected', () => wsStore.setStatus('connected'));
    ws.on('disconnected', () => wsStore.setStatus('disconnected'));

    ws.on('token', (data) => chatStore.appendToken(data));
    ws.on('tool_call_start', (data) => chatStore.startToolCall(data.id, data.name, data.args));
    ws.on('tool_call_result', (data) => chatStore.completeToolCall(data.id, data.result, data.success, data.duration_ms));
    ws.on('governance', (data) => chatStore.attachGovernance(data.tool_call_id, data));
    ws.on('cost_update', (data) => chatStore.updateCost(data.session_cost_usd, data.turn_cost_usd));
    ws.on('turn_complete', (data) => {
      chatStore.completeTurn(data.usage, data.cost_usd);
      queryClient.invalidateQueries({ queryKey: ['sessions'] });
    });

    ws.connect();
    return () => ws.disconnect();
  }, [token]);
}
```

### Shared types

```typescript
// lib/types.ts
interface ChatMessage {
  id: string;
  role: 'user' | 'assistant' | 'error';
  content: string;
  toolCalls?: ToolCallState[];
  timestamp: number;
}

interface ToolCallState {
  id: string;
  name: string;
  args: Record<string, unknown>;
  result?: unknown;
  success?: boolean;
  durationMs?: number;
  governance?: GovernanceDecision;
  status: 'running' | 'complete' | 'denied' | 'error';
}

interface GovernanceDecision {
  effect: 'allow' | 'deny' | 'warn' | 'audit';
  policyName?: string;
  policyPriority?: number;
  matchedPattern?: string;
  reason: string;
}

interface TokenUsage {
  inputTokens: number;
  outputTokens: number;
}

interface SessionSummary {
  id: string;
  label?: string;
  blueprintName: string;
  messageCount: number;
  costUsd: number;
  lastActivity: number;
  status: 'active' | 'idle' | 'error';
}
```

---

## Status bar

Always visible at the bottom of the viewport. Shows ambient system
state without requiring navigation.

```
 ws ●  │  session: auth-refactor  │  research-assistant · sonnet-4  │  $0.042  │  iter 3/25
```

**Segments:**

1. **WebSocket status** — green dot (connected), red dot (disconnected),
   amber dot (reconnecting)
2. **Session** — active session label (truncated)
3. **Agent + Model** — blueprint name and model (only during active chat)
4. **Cost** — running session cost in USD. Color: default < 70% budget,
   amber 70-90%, red > 90%
5. **Iteration** — `iter N/max` (only while agent is running)

---

## Keyboard shortcuts

| Shortcut | Action |
|----------|--------|
| `Cmd+K` | Command palette |
| `Cmd+N` | New chat session |
| `Cmd+.` | Abort current agent run |
| `Cmd+Enter` | Send message |
| `Cmd+1`–`Cmd+7` | Navigate to page |
| `Cmd+[` / `Cmd+]` | Previous / next session |
| `Cmd+E` | Export current session |
| `Cmd+/` | Focus search in current page |
| `Esc` | Close panel / dismiss palette |

---

## Build integration

### Development

```bash
cd apps/web && npm run dev    # Vite dev server on :5173
mp serve --dev                # Proxies /api/* to Bun, serves UI from Vite
```

In dev mode, the `mp serve` HTTP handler adds a proxy layer that
forwards non-API requests to the Vite dev server. Full HMR for the
frontend while the backend runs natively.

### Production

```bash
cd apps/web && npm run build  # Output to apps/web/dist/
mp serve                      # Serves dist/ as static assets
```

---

## CLI integration

```
mp serve                       Start web UI + API server (default :1745)
mp serve --port 8080           Custom port
mp serve --no-auth             Disable auth (trusted network)
mp serve --dev                 Dev mode: proxy frontend to Vite
mp serve --open                Open browser after starting
```

---

## Implementation phases

| Phase | Scope | Effort |
|-------|-------|--------|
| **0** | App scaffold: Vite + React 19 + Tailwind v4 + TanStack Router + TanStack Query + Zustand. Run `npx shadcn@latest init`. Add core ui/ components (Button, Badge, Command, Sheet, Dialog, Table, Collapsible, ScrollArea, Textarea, DropdownMenu, Switch, Separator, Tooltip, Tabs, AlertDialog). Build `app-shell.tsx` layout with sidebar + status bar. | 1 day |
| **1** | `ws-client.ts` + `useWebSocket` hook + `ws-store`. `api-client.ts` with typed fetch wrapper + auth header injection. | 0.5 day |
| **2** | Chat page: `chat-store`, `useChatStream` hook, `message-list.tsx` (ScrollArea), `user-message.tsx`, `agent-message.tsx` (react-markdown + rehype-highlight), `streaming-cursor.tsx`, `chat-input.tsx` (Textarea + DropdownMenu for blueprint selector). End-to-end: type message → WS send → stream tokens → render markdown. | 2 days |
| **3** | Tool calls: `tool-call.tsx` (Collapsible + Badge), `governance-badge.tsx`. Collapsed/expanded views. Governance decisions rendered inline. | 1.5 days |
| **4** | Session sidebar: `session-sidebar.tsx` (ScrollArea), `useSessions` hook (TanStack Query), `useCreateSession` / `useRenameSession` / `useDeleteSession` mutations. Session switching via `chat-store.setActiveSession`. | 1 day |
| **5** | Command palette: `command-palette.tsx` (shadcn Command), `useCommand` hook, `useHotkeys` hook. Wire `Cmd+K`, `Cmd+N`, `Cmd+.`, `Cmd+Enter`, `Cmd+1`–`Cmd+7`. | 1 day |
| **6** | `data-table.tsx` reusable component on shadcn Table (sortable columns, row selection, search). `detail-sheet.tsx` on shadcn Sheet. `confirm-delete.tsx` on AlertDialog. `empty-state.tsx`. | 1 day |
| **7** | Sessions page: `useSessions` + DataTable + search + bulk delete + export. | 1 day |
| **8** | Agents page: `useBlueprints` hook, card layout, capability tree (nested list), launch button, enable/disable Switch. Detail Sheet with full blueprint source. | 1.5 days |
| **9** | Policies page: `usePolicies` hook + DataTable + Sheet with YAML editor + create/edit/toggle/delete. | 1.5 days |
| **10** | Skills page: `useSkills` hook + DataTable + Sheet editor. | 1 day |
| **11** | Schedule page: `useSchedule` hook + DataTable + expandable run history + trigger mutation. | 1 day |
| **12** | System page: `useConfig` hook + key-value editor + `useIndexHealth` hook + health display + reindex/prune mutations. | 0.5 day |
| **13** | REST API endpoints: sessions, blueprints, policies, skills, schedule, config, search, index. Served by `@moneypenny/http`. | 2 days |
| **14** | Auth flow: token paste dialog, localStorage persistence, WebSocket auth message, route guard. | 0.5 day |
| **15** | Polish: Suspense boundaries per route, ErrorBoundary components, loading skeletons, keyboard accessibility audit, status bar wiring. | 1 day |

**Total: ~17 days**

Phases 0–4 produce a working agent view with streaming, tool calls,
governance visibility, and session management — the core product
experience. Phase 5 adds the command palette. Phases 6–12 add
management views. Phases 13–15 are backend wiring and polish.

### Critical path

```
Phase 0 (scaffold + shadcn init)
  │
  ├── Phase 1 (WS client + API client)
  │     │
  │     └── Phase 2 (chat page + streaming)
  │           │
  │           ├── Phase 3 (tool calls + governance)
  │           │
  │           └── Phase 4 (session sidebar + hooks)
  │
  ├── Phase 5 (command palette + hotkeys — independent)
  │
  ├── Phase 6 (DataTable + Sheet + AlertDialog — independent)
  │     │
  │     ├── Phase 7 (sessions page)
  │     ├── Phase 8 (agents page)
  │     ├── Phase 9 (policies page)
  │     ├── Phase 10 (skills page)
  │     ├── Phase 11 (schedule page)
  │     └── Phase 12 (system page)
  │
  ├── Phase 13 (REST API — can start in parallel with all frontend work)
  │
  └── Phase 14–15 (auth + polish)
```

Phases 0–4 are the MVP. Ship the agent view, validate, then build
management views in parallel with REST API work.

---

## What we deliberately skip (for now)

- **Code diffing / syntax-highlighted edits** — tool results are
  structured text blocks, not rich editor components. Post-MVP.
- **Charts and graphs** — no cost charts, latency histograms, or token
  usage graphs. Numbers in tables and status bars are sufficient for MVP.
- **Light theme** — dark only. Light theme is a post-MVP toggle.
- **Real-time event stream (Observe page)** — the streaming event log
  with filters is post-MVP. MVP has the agent view (which shows
  everything relevant inline) and the management CRUD views.
- **Tune page** — model parameter tuning (temperature, top-p, context
  budgets) is configured in blueprint YAML files for MVP. A dedicated
  tuning UI is post-MVP.
- **Notifications / toasts** — errors and status changes are shown
  inline or in the status bar. No toast notification system for MVP.
- **Mobile responsive layout** — desktop-first. Responsive is post-MVP.
- **Collaborative / multi-user** — single user per instance.
- **Blueprint editing in the UI** — blueprints are viewed and launched,
  not authored. Use your editor for markdown/YAML changes.
- **Plugin / extension system** — the component set is fixed.

---

## Relationship to sprint-1 web-ui spec

The sprint-1 `web-ui.md` spec defined the initial technology stack
(React 19 + Vite + Tailwind + Zustand), WebSocket protocol, and page
structure. This sprint-2 spec supersedes it with:

1. **Agent-view-first design** — the chat page is redesigned around
   Cursor's linear agent view with inline tool calls, replacing the
   generic chat + sidebar model.
2. **shadcn/ui component library** — replaces hand-rolled components
   with accessible, Radix-based primitives (Command, Collapsible,
   Sheet, Table, Dialog, etc.) styled with Tailwind.
3. **Hooks-first architecture** — TanStack Query for all server state,
   Zustand only for client-ephemeral state. Every data domain gets a
   custom hook. Components are thin composition layers.
4. **TanStack Router** — type-safe routing with loader/action patterns,
   replacing Wouter.
5. **Governance as a first-class UI element** — policy decisions visible
   inline on every tool call, not in a separate view.
6. **Management CRUD views** — dedicated pages for policies, skills, and
   memories that weren't in sprint-1.
7. **MVP scoping** — explicitly cuts charts, code diffing, tune page,
   and observe page to focus on the agent view + CRUD.

The React 19 + Vite foundation carries forward. Zustand remains but with
a narrower scope (client-only state). The bundle budget increases
slightly to accommodate TanStack Query and Radix primitives.
