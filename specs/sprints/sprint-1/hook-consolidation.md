# Hook System Consolidation

### Problem

Two hook systems exist today:

1. **In-process `HookPipeline`** (`@moneypenny/ctx/builtin/pipeline.ts`):
   Used by the agent loop. Runs `dbPolicyHook`, `credentialRedactor`,
   `costGuard`, `confirmationGate`. This is the production code path.

2. **DB `hooks` table + `operations.execute`** (`@moneypenny/ctx/hooks.ts`,
   `operations.ts`): Stores hook scripts as `Function()` constructor
   strings in the database. Implemented but **not wired** to the main
   agent loop. Dead code on the primary execution path.

### Design: merge with declarative conditions

Replace the `Function()`-based DB hooks with **declarative condition
rules** that load into the same `HookPipeline` at startup. This gives:

- **Single execution path** at runtime (no two-system confusion)
- **Portability** (hooks defined in DB are shareable via cloud sync)
- **No arbitrary code execution** (conditions are data, not eval'd strings)

```typescript
// Declarative hook definition (stored in hooks table)
export interface DeclarativeHook {
  id: string;
  name: string;
  phase: "pre_tool" | "post_tool" | "pre_llm" | "post_llm";
  priority: number;
  condition: HookCondition;
  action: HookAction;
  enabled: boolean;
}

export type HookCondition =
  | { type: "tool_name"; pattern: string }      // glob match
  | { type: "args_match"; jsonpath: string; value: string }
  | { type: "cost_exceeds"; usd: number }
  | { type: "session_turns_exceed"; count: number }
  | { type: "always" };

export type HookAction =
  | { type: "deny"; message: string }
  | { type: "audit"; message: string }
  | { type: "confirm"; message: string }
  | { type: "transform_args"; jsonpath: string; value: string }
  | { type: "inject_context"; content: string };
```

At startup, `createHookPipeline` loads declarative hooks from the DB and
merges them with code-defined hooks (policies, credential redactor, cost
guard), sorted by priority.

### Migration

The existing `hooks` table is **recreated** (not just altered) because:

1. The `phase` CHECK constraint changes from
   `('pre:validation','pre:injection','post:transform')` to
   `('pre_tool','post_tool','pre_llm','post_llm')` — SQLite doesn't
   support `ALTER CONSTRAINT`.
2. The `script TEXT NOT NULL` and `match_pattern TEXT NOT NULL` columns
   are removed entirely, replaced by nullable `condition` and `action`
   JSON columns.

Existing rows are migrated with a phase mapping:
- `pre:validation` → `pre_tool`
- `pre:injection` → `pre_llm`
- `post:transform` → `post_llm`

The `script` content is **not** migrated (it was `Function()` constructor
code). A warning is logged at startup for any hooks that had scripts.
These must be re-created as declarative hooks.

See `schema-v10.md` for the full migration SQL.

### Acceptance criteria

- [ ] Existing code-defined hooks (`costGuard`, `credentialRedactor`, `dbPolicyHook`) work unchanged
- [ ] Declarative hooks from DB are loaded and execute in priority order
- [ ] `Function()` constructor is no longer used anywhere in the hook system
- [ ] Hooks can be created/updated via the API (`POST /api/v1/hooks`)
- [ ] Hook execution is visible in governance events

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 8.1 | `DeclarativeHook` types, condition evaluator, action executor | 1.5 days |
| 8.2 | Load declarative hooks into `HookPipeline` at startup | 1 day |
| 8.3 | Migrate `hooks` table schema, remove `Function()` usage | 0.5 days |
| 8.4 | HTTP API for hook CRUD | 0.5 days |
