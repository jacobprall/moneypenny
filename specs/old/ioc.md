# Refactoring Plan: Inversion of Control (IoC) for Tool Services

**Status:** Implemented (see codebase; dual-mode phase skipped — direct `services`-only `ToolContext`)  
**Scope:** `packages/tools`, `packages/loop`, `packages/search` (minor), `packages/db` (provider layer)  
**Estimated effort:** Medium-large (touches 12 files directly, 4 orchestration files)

---

## 1. Problem Statement

Every tool in Moneypenny receives `ToolContext`, which contains the full `AgentDB` object:

```typescript
// packages/tools/src/types.ts (current)
export interface ToolContext {
  services: ToolServices;
  repoPath: string;
  workingDir: string;
  signal?: AbortSignal;
  childLoopFactory?: ChildLoopFactory;
}
```

`AgentDB` is a God Object: it exposes the raw `bun:sqlite` `Database` handle (`db.db`), the
workspace index DB (`db.workspace`), CRDT sync state (`db.syncLoaded`, `db.siteId`), and model
paths. Any tool can run arbitrary SQL, mutate any table, or trigger side effects outside its
domain. This creates three categories of problems:

### 1.1 Testability
Unit testing a tool like `code_search` requires constructing a full `AgentDB` with FTS5 tables,
optional vector extensions, workspace schema migrations, and exclude-pattern seeds. A mock
`SearchService` with `hybridSearch: () => [...]` would reduce this to three lines.

### 1.2 Boundary Enforcement
`query_db` uses `context.services.query.executeReadOnlyQuery(...)`, which enforces SELECT/WITH,
injects a default `LIMIT`, and runs on a dedicated read-only connection (`ensureAgentQueryReadDb`).

### 1.3 Portability
Tools are bound to `bun:sqlite` and native C extensions (`sqlite-vector`, `sqlite-ai`,
`sqlite-sync`). If Moneypenny ever needs tools to run in a worker, browser, or against a remote
backend, every tool file would need rewriting. With injected service interfaces, only the
concrete provider implementation changes.

---

## 2. Current State: Full Dependency Audit

### 2.1 Tool-by-Tool AgentDB Usage (pre-IoC audit, kept for reference)

| Tool | `context.db` access | What it actually needs |
|------|---------------------|----------------------|
| `code_search` | `hybridSearch(context.db, ...)`, `validateAndRefreshResults(context.db.workspace, ...)`, `getExcludePatterns(context.db)` | Search service (hybrid search + exclude patterns) |
| `file_write` | `tryWriteThrough(context.db, relPath, content)` → reads `db.workspace`, calls `reindexFile(ws, ...)` | Workspace indexing service |
| `file_edit` | Same as `file_write` via `tryWriteThrough` | Workspace indexing service |
| `git_commit` | `reindexFiles(context.db.workspace, changedPaths)` after successful commit | Workspace indexing service |
| `compact_conversation` | `compactConversation(context.db, upToTurn, summary)` | Conversation service |
| `delegate` | `getSubagentDef(context.db, name)`, `getSkill(context.db, skillName)`, passes `context.db` to child loop via `ChildLoopParams.db` | Subagent + skill lookups; DB pass-through to child |
| `read_skill` | `getSkill(context.db, ...)`, `listSkillFiles(context.db, ...)`, `getSkillFile(context.db, ...)` | Skill service |
| `query_db` | **Raw SQL** on `context.db.db` (historical) | `QueryService` + read-only handle (see `parallel-queries.md`) |
| `file_search` | None | — |
| `file_read` | None | — |
| `bash` | None | — |
| `git_status` | None | — |
| `git_diff` | None | — |
| `web_fetch` | None (ignores `context` entirely) | — |
| `web_search` | None (ignores `context` entirely) | — |

**Summary:** 8 of 15 tools used `context.db` in this audit. Of those, 7 mapped cleanly to service
functions. `query_db` needed SQL execution (today: `QueryService` on a read-only handle; see
[`parallel-queries.md`](./parallel-queries.md)).

### 2.2 Orchestration Layer Usage

The tool executor (`packages/loop/src/tool-executor.ts`) receives `db: AgentDB` as a **separate
parameter** alongside `toolContext: ToolContext`. That `db` is used exclusively for persistence:

- `appendMessage(db, ...)` — logging tool results back to the conversation
- `appendEvent(db, ...)` — recording `tool.called`, `tool.complete`, `tool.error`, `turn.paused`

These are orchestration concerns, not tool concerns. They should continue using `AgentDB`
directly. The `db` parameter on the executor does **not** need to go through IoC.

The loop (`packages/loop/src/loop.ts`) builds `ToolContext` with `services` (from
`createToolServices(db)` once per `runAfterUserMessage`) and passes `db` separately to the tool
executor for persistence only.

### 2.3 Child Loop (`delegate` → `child-loop.ts`)

`ChildLoopParams` no longer carries `db`. `createChildLoopFactory` closes over the parent
`AgentDB` and passes it into `childLoop.run(...)` when the delegate tool invokes the factory.

The child loop still needs the full `AgentDB` for conversation resolution, `appendMessage`,
`appendEvent`, and cost tracking. The delegate tool only performs service lookups
(`context.services.*`); it never receives an `AgentDB` on `ToolContext`.

---

## 3. Target Architecture

### 3.1 Service Interface Definitions

Create `packages/tools/src/services.ts`:

```typescript
import type { SearchOptions, SearchResult, Skill, SubagentDef } from "@moneypenny/db/types";

/** Hybrid + fallback code search. */
export interface SearchService {
  hybridSearch(query: string, opts?: SearchOptions): SearchResult[];
  validateAndRefreshResults(results: SearchResult[]): SearchResult[];
  getExcludePatterns(): string[];
}

/** Write-through workspace index updates. */
export interface WorkspaceService {
  reindexFile(relPath: string, opts?: { content?: string }): void;
  reindexFiles(relPaths: string[]): void;
}

/** Skill instructions and supporting files. */
export interface SkillService {
  getSkill(name: string): Skill | null;
  getSkillFile(name: string, path: string): string | undefined;
  listSkillFiles(name: string): string[];
}

/** Subagent definition lookups. */
export interface SubagentService {
  getSubagentDef(name: string): SubagentDef | null;
}

/** Conversation management. */
export interface ConversationService {
  compactConversation(upToTurn: number, summary: string): void;
}

/**
 * Read-only SELECT: validates SELECT/WITH, appends LIMIT when missing; read-only SQLite handle.
 */
export interface QueryService {
  executeReadOnlyQuery(sql: string, params?: (string | number)[]): Record<string, unknown>[];
}
```

### 3.2 Updated `ToolContext`

```typescript
// packages/tools/src/types.ts (current)
export interface ToolServices {
  search: SearchService;
  workspace: WorkspaceService;
  skills: SkillService;
  subagents: SubagentService;
  conversation: ConversationService;
  query: QueryService;
}

export interface ToolContext {
  services: ToolServices;
  repoPath: string;
  workingDir: string;
  signal?: AbortSignal;
  childLoopFactory?: ChildLoopFactory;
}
```

**Shipped:** `context.db` is removed. Tools use `context.services.*` only.

### 3.3 The `delegate` / Child Loop Problem (resolved)

Previously, `delegate` passed `context.db` into `ChildLoopParams`. **Shipped:** child-loop wiring
lives in `ChildLoopFactory`; `CreateChildLoopFactoryConfig` includes `db`, and
`createChildLoopFactory` closes over it for `childLoop.run(config.db, params.task)`.
`ChildLoopParams` has no `db` field.

```typescript
export interface ChildLoopParams {
  repoPath: string;
  workingDir: string;
  signal?: AbortSignal;
  task: string;
  skillInstructions: string;
  allowedTools: string[];
  maxIterations: number;
  maxCostUsd?: number;
}
```

```typescript
export function createChildLoopFactory(config: CreateChildLoopFactoryConfig): ChildLoopFactory {
  return {
    async run(params: ChildLoopParams): Promise<ChildLoopResult> {
      for await (const event of childLoop.run(config.db, params.task)) {
        // config.db from factory config, not from params
      }
    },
  };
}
```

This keeps `delegate.ts` free of any `AgentDB` reference while preserving the child loop's need
for full orchestration capabilities.

### 3.4 Concrete providers (`createToolServices`)

**Location:** `packages/tools/src/create-tool-services.ts` (implementation lives in `tools` so
`@moneypenny/loop` does not depend on `@moneypenny/search`; MCP can depend on `tools` + `search`
without pulling `loop`).

**Loop wiring:** `runAfterUserMessage` calls `createToolServices(db)` once per user turn and
reuses that object for every tool iteration in the turn (not once per assistant message with
tools).

**Workspace failures:** `reindexFile` / `reindexFiles` log with `console.warn` under `[mp] workspace …`
instead of failing silently.

```typescript
import type { AgentDB } from "@moneypenny/db";
import { hybridSearch, getExcludePatterns } from "@moneypenny/search";
import { validateAndRefreshResults, reindexFile, reindexFiles } from "@moneypenny/db/workspace";
import { getSkill, getSkillFile, listSkillFiles, getSubagentDef, compactConversation } from "@moneypenny/db";
import type { ToolServices } from "./services.js";

// … validation helpers for query (SELECT/WITH prefix, LIMIT cap), logWorkspaceReindexFailure …

export function createToolServices(db: AgentDB): ToolServices {
  return {
    search: { /* hybridSearch(db, …), validateAndRefreshResults, getExcludePatterns */ },
    workspace: { /* reindexFile / reindexFiles with try/catch + warn */ },
    skills: { getSkill: (name) => getSkill(db, name) ?? null, /* … */ },
    subagents: { getSubagentDef: (name) => getSubagentDef(db, name) ?? null },
    conversation: { compactConversation: (upToTurn, summary) => compactConversation(db, upToTurn, summary) },
    query: {
      executeReadOnlyQuery(query, params) {
        // validate SELECT/WITH; ensure LIMIT; prepare + all on ensureAgentQueryReadDb(db)
      },
    },
  };
}
```

**Parallel reads:** `query_db` uses a separate read-only connection; see [`parallel-queries.md`](./parallel-queries.md).

---

## 4. Execution plan (completed)

Phases below were the original migration plan. **Shipped behavior:** dual-mode `ToolContext` was
skipped; `context.db` was removed in one pass with tools on `context.services` only.
`createToolServices` lives in `@moneypenny/tools` and is re-exported from `@moneypenny/loop` for
callers that already import the factory from the loop package.

### Phase 1: Define Interfaces (no breaking changes)

**Files created:**
- `packages/tools/src/services.ts` — all service interfaces

**Files modified:**
- `packages/tools/src/types.ts` — add `ToolServices` type, import service interfaces
- `packages/tools/src/index.ts` — re-export service types

**Validation:** `pnpm typecheck` passes (additive only).

### Phase 2: Create Concrete Providers (no breaking changes)

**Files created:**
- `packages/tools/src/create-tool-services.ts` — `createToolServices(db: AgentDB): ToolServices`

**Validation:** `pnpm typecheck` passes. No consumers yet.

### Phase 3: Dual-Mode ToolContext (backward-compatible transition)

**Skipped.** No transitional `context.db` + `context.services` period.

### Phase 4: Migrate Tools (one tool per commit)

Original plan grouped migrations by tool; all listed tools now use `context.services`. See git
history for per-file diffs.

#### 4d. Raw query access (as shipped)

| Tool | Change |
|------|--------|
| `query_db` | Tool forwards `query` + `params` to `context.services.query.executeReadOnlyQuery`. Validation (SELECT/WITH, default `LIMIT`) and execution use `ensureAgentQueryReadDb` (read-only handle); see `parallel-queries.md`. |

### Phase 5: Remove `context.db` and `write-through.ts`

Done.

### Phase 6: Add Mock-Based Tests

Future work; not blocking IoC ship.

---

## 5. Migration checklist (completed)

- [x] **Phase 1:** `packages/tools/src/services.ts`, `types.ts`, `index.ts`
- [x] **Phase 2:** `packages/tools/src/create-tool-services.ts` (not under `loop/`)
- [x] **Phase 3:** Dual-mode skipped — straight to `services`-only `ToolContext`
- [x] **Phase 4:** All tools migrated (`query_db` validation inside `QueryService` implementation)
- [x] **Phase 5:** `context.db` removed; `write-through.ts` removed
- [ ] **Phase 6:** Mock-based unit tests per service consumer (optional follow-up)
- [ ] **Phase 6:** Full `pnpm typecheck && pnpm test` in CI when applicable

---

## 6. Risks and Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| `delegate` child loop needs full `AgentDB` | Could leak `db` back into tools layer | Close over `db` in `ChildLoopFactory` constructor, not in `ChildLoopParams`. Tool never sees it. |
| `query_db` and raw SQL | Intentional read surface on agent DB | `QueryService.executeReadOnlyQuery` enforces SELECT/WITH, caps rows with `LIMIT`, and runs on a dedicated read-only connection. |
| Parallel tools + `query_db` | Interleaving on one connection | **Resolved:** [`parallel-queries.md`](./parallel-queries.md) — `ensureAgentQueryReadDb` + no SAVEPOINT on read path. Other tools still share `agent.db` where applicable. |
| Performance of service object construction | Extra allocations in the hot path | `createToolServices(db)` is invoked once per `runAfterUserMessage` (user turn), then reused for every tool iteration in that turn. |
| Third-party/user tools may depend on `context.db` | Breaking change for custom tools | Document `context.services`; `createToolServices` is the reference wiring from `AgentDB`. |

---

## 7. Non-Goals

- **Decoupling the orchestration layer** (`tool-executor.ts`, `loop.ts`) from `AgentDB`. These
  modules are the wiring layer — they *should* know about `AgentDB` to construct services and
  persist events/messages. Only the tool implementations are decoupled.
- **Dependency injection framework**. No IoC container, no decorators, no runtime reflection.
  Just plain TypeScript interfaces and a factory function.
- **Async service interfaces**. Current DB access is synchronous (bun:sqlite). Keep interfaces
  synchronous where possible; mark async only where needed (e.g., future remote backends can
  wrap sync signatures in `Promise.resolve()`).
