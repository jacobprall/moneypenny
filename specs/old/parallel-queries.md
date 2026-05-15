# Implementation plan: isolated read connection for `query_db` (D + E)

**Status:** Implemented  
**Related:** [`ioc.md`](./ioc.md) (ToolServices / `QueryService`), parallel tool execution in `packages/loop/src/tool-executor.ts`

---

## 1. Goal

When `parallelToolExecution` runs multiple tools via `Promise.allSettled`, any tool that uses the primary `AgentDB.db` connection can interleave with `query_db`. That causes correctness risk (shared transaction state, fixed `SAVEPOINT` names, statement lifecycle) beyond “two `query_db` calls at once.”

**Target behavior:**

- **`query_db` reads** use a **second** `bun:sqlite` `Database` handle opened against the **same file** as `AgentDB.dbPath`, in **read-only** mode (option **D**).
- On that handle, run **validated** `SELECT` / `WITH` + enforced `LIMIT` **without** `SAVEPOINT` fencing (option **E**), because the handle is dedicated to vetted reads and is not the loop’s writer connection.

Non-goals for this change: serializing other tools, mutex around all of `AgentDB`, or changing workspace / hybrid search connection strategy.

---

## 2. Why combine D and E

- **D** removes overlap between `query_db` and everything else on the primary connection.
- **E** (dropping savepoints on the read path) is appropriate **after** D: the original fence mainly protected the **writer** connection’s transaction state. The readonly handle should not run migrations, hooks, or tool side effects; only `QueryService.executeReadOnlyQuery` uses it.

Keep **strict validation** on the read path (prefix, `LIMIT`, deny obvious foot-guns). E is not “trust arbitrary SQL.”

---

## 3. Design

### 3.1 Opening the readonly connection

- Use Bun’s supported API: `new Database(dbPath, { readonly: true })` (and **not** `{ create: true }`; the agent file must already exist when `query_db` runs, which matches normal session use).
- Open against **`AgentDB.dbPath`** (same path as `createAgentDB` in `packages/db/src/database.ts`).
- **Do not** re-run schema application, migrations, extension loading, or blueprint logic on this handle. It is a **thin read mirror** of the already-open writer DB.

### 3.2 Where the handle lives

Preferred: **lazy, memoized handle owned by `AgentDB`**, so lifecycle stays next to the primary DB:

- Extend `AgentDB` in `packages/db/src/types.ts` with an optional field, e.g. `queryReadDb?: Database` (name bikeshed: `readReplica`, `queryConn` — pick one and use consistently).
- Add `ensureAgentQueryReadDb(agent: AgentDB): Database` in `packages/db/src/database.ts` (or a small `query-read.ts` next to it) that:
  - returns `agent.queryReadDb` if set;
  - otherwise opens readonly, assigns `agent.queryReadDb`, sets minimal pragmas if needed (see §3.4), returns it.

**Alternative** (acceptable but easier to leak): memoize inside `createToolServices` on a `WeakMap<AgentDB, Database>`. Prefer extending `AgentDB` so `closeAgentDB` can close both handles in one place.

### 3.3 `QueryService` implementation

In `packages/tools/src/create-tool-services.ts` (or a helper imported by it):

- Resolve the DB handle via `ensureAgentQueryReadDb(db)` instead of `db.db`.
- Keep `ALLOWED_SQL_PREFIX`, `ensureQueryLimit`, bind params as today.
- **Remove** `SAVEPOINT` / `ROLLBACK TO` / `RELEASE` for this path.
- Use `prepare` + `all` (or `query` API if you standardize) on the readonly handle only.

### 3.4 Pragmas on the readonly connection

- Writer already sets `journal_mode=WAL` and `foreign_keys=ON` on the primary connection (`createAgentDB`).
- On readonly open, SQLite typically inherits WAL visibility; still safe to run `PRAGMA foreign_keys=ON` if supported on readonly (verify once in implementation).
- Avoid `PRAGMA journal_mode=...` writes on the readonly handle unless documentation confirms it is allowed; if unclear, skip and rely on the existing writer WAL.

### 3.5 Concurrency and consistency

- WAL mode allows **concurrent readers** while the writer commits; readers may see a **consistent snapshot** per query, not necessarily the single latest row committed mid-flight — acceptable for introspection tools.
- Document for product/UX: `query_db` is **read-your-writes–ish** but not a linearizable global snapshot with the primary connection unless you add explicit `BEGIN` / snapshot semantics later (out of scope).

### 3.6 Shutdown

- Update `closeAgentDB` in `packages/db/src/database.ts` to close `agent.queryReadDb` if present (try/finally order: close read replica, then `agent.db.close()` as today, or reverse if Bun requires — implement defensively with best-effort `try/catch` on each close).

Any other code paths that close only `agent.db` must be audited (grep for `.db.close` / `closeAgentDB`).

---

## 4. Implementation checklist (completed)

1. **Types** — `AgentDB.queryReadDb?: Database` in `packages/db/src/types.ts`.
2. **Open + memoize** — `ensureAgentQueryReadDb` in `packages/db/src/database.ts` (`readonly: true`, `create: false`, `PRAGMA foreign_keys=ON`).
3. **Wire `QueryService`** — `packages/tools/src/create-tool-services.ts` uses `ensureAgentQueryReadDb`; savepoint block removed.
4. **Lifecycle** — `closeAgentDB` closes `queryReadDb` first, then `agent.db`; no ad-hoc `agent.db.close` on `AgentDB` outside `closeAgentDB`.
5. **MCP / CLI** — unchanged; existing `closeAgentDB` callers get both handles closed.
6. **Docs** — `ioc.md` + this file updated.
7. **Tests** — `packages/db/src/__tests__/ensure-agent-query-read-db.test.ts` (memoization + `closeAgentDB` clears read handle).

---

## 5. Risks and mitigations

| Risk | Mitigation |
|------|------------|
| Readonly open fails on some platforms / paths | Clear error message; fall back is **not** required for v1 — failing closed is better than silently using the writer connection. |
| Someone uses `queryReadDb` for writes in future | Keep field package-private convention or document “tools layer must never expose this”; only `ensureAgentQueryReadDb` + `QueryService` use it. |
| Stale reads vs writer | Accept WAL snapshot semantics; document. |
| Double-close | Guard with `undefined` assignment after close or try/catch on second close. |

---

## 6. Out of scope (follow-ups)

- Mutex or tool-classification for other services that still use `db.db` under parallel execution (search, skills, etc.) if those paths prove problematic.
- `ATTACH`, multiple databases, or user SQL beyond strict SELECT/WITH.
- Snapshot / `BEGIN DEFERRED` isolation level tuning for stronger read consistency.

---

## 7. Validation

- `pnpm typecheck` in repo root.
- Manual: enable `parallelToolExecution`, trigger a batch with `query_db` + `code_search` (or another DB user), confirm no SQLITE_BUSY / savepoint errors under light load.
