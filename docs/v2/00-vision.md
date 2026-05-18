# Moneypenny v2 — Vision

## What

A local-first agent workspace. Not an IDE, not a dashboard — a **session manager** for parallel AI agents that you launch, observe, direct, and mine for knowledge.

Multiple agents running as tabs. A backlog of ideas that evolve into specs that become sessions. Blueprints that define agent behavior. One SQLite database, one Bun process, one port.

## Who

A single developer running a swarm of agents across multiple repos — managing ideas, launching implementations, reviewing results — without fighting the tool.

## Differentiators

Not "Cursor's agent view, but yours." Specifically:

- **Swarm-native** — N parallel sessions are first-class, not an afterthought
- **Filesystem-native definitions** — blueprints and ideas are markdown files you author and version-control; nothing locks you in
- **Knowledge mining across sessions** — skills, conventions, and pointers persist beyond any single conversation
- **HITL is first-class** — sessions can pause for you, not just complete without you
- **Local + portable** — no server, no auth surface, no cloud dependency; copy the data dir to move

## Core Principles

1. **Session-first** — every interaction is a session. Ideas become sessions. Blueprints configure sessions. The UI is a session manager.

2. **Filesystem-backed where it matters** — blueprints, ideas, policies are `.md`/`.toml` files. Read at runtime, never consumed into SQLite.

3. **DB-backed for runtime state** — sessions, messages, runs, events, knowledge, code index. Generated and ephemeral state.

4. **Mutable context, fixed safety** — cwd shifts, tools change, agents swap mid-session. Permissions and policies are the boundary that doesn't move.

5. **Launch and let go** — sessions run in background by default. Interactive mode is "connecting into" a running session.

6. **HITL on demand** — agents can pause for human direction at declared checkpoints or via explicit request. Resume by typing.

7. **Cheap to create, valuable to mine** — sessions are semi-disposable; the knowledge graph persists across them.

8. **The event loop is the swarm** — agents are I/O-bound (waiting on LLM APIs). No subprocesses, no workers. Async functions interleaving on one event loop.

## Key Concepts

| Concept | What It Is | Where It Lives |
|---------|------------|----------------|
| **Session** | The atomic unit of work. A conversation stream + execution context (cwd, blueprint, tools, permissions). Displayed as a tab. | SQLite |
| **Message** | The building block of a session. A single role-tagged entry: user input, assistant output, or tool result. | SQLite |
| **Run** | One agent invocation that may produce multiple messages (assistant text + tool calls + tool results). Groups messages for rendering and accounting. | SQLite |
| **Tool** | A typed capability an agent can invoke (`search_code`, `write_file`, `spawn_agent`). Registered at startup. | Code |
| **Tool Call** | A specific invocation of a tool within a run. Has args, result, status, duration. | Inside messages (JSON) |
| **Blueprint** | A markdown file with YAML frontmatter defining an agent's prompt, tool whitelist, permissions, strategy. Read at session creation. | Filesystem (`~/.moneypenny/blueprints/`) |
| **Idea** | A markdown file with YAML frontmatter. A backlog item that can evolve into a spec, then a session. | Filesystem (`~/.moneypenny/ideas/`) |
| **Event** | A typed audit record of something that happened (session created, run started, tool failed). Drives the activity feed and global SSE. | SQLite |
| **Permission** | A coarse capability flag on a session (filesystem, network, shell). Inherited by children, narrowable, never expandable. | Session config |
| **Policy** | A rule with effect (allow/warn/deny) and conditions (max daily spend, denied paths). Enforced at runtime. | Filesystem → SQLite |
| **Pointer** | A short reference to a key moment in a session ("decided JWT over sessions"). Aggregated across sessions for context assembly. | SQLite |
| **Skill** | A reusable procedure extracted from successful sessions ("how to add a new tRPC route"). | SQLite |
| **Convention** | A detected project pattern ("uses pnpm workspaces", "prefers named exports"). | SQLite |
| **Knowledge** | The composite of pointers + skills + conventions. The persistent memory across sessions. | SQLite |
| **Scope** | A session's `config.cwd`. The directory the agent is "thinking in." Single value in v2; arrays revisited later. | Session config |

## Architecture (One Sentence)

Hono serves a React SPA (shadcn + TanStack Router + TanStack Table) talking to a Hono RPC backend; agents run as async loops on the Bun event loop, writing through a single shared connection; per-session SSE streams agent output, a global SSE channel streams cross-session events; all capabilities exposed as both HTTP and MCP.
