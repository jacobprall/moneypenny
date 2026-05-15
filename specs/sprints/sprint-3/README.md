# Sprint 3 — Channels, Reactivity, and Self-Evolution

> The sprint that opens moneypenny to the outside world and gives it
> reflexes. Multi-channel I/O (Telegram, webhooks, embeddable JS SDK),
> a reactive event layer driven by SQLite write hooks, self-evolving
> agent prompts informed by usage data, and a stable SQL query surface
> for external tools.

**Prerequisites:** Sprint 2 complete (embeddings, parallel tools, unified
query, session lifecycle, custodian)

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
