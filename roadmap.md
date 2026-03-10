# Moneypenny Roadmap

What stands between the product and the vision. Ordered by dependency and impact.

Companion persona analysis: [`../PLATFORM_ENGINEERING_PERSONA_MATRIX.md`](../PLATFORM_ENGINEERING_PERSONA_MATRIX.md)

---

## Capability-First Priorities

Roadmap is optimized for platform-engineering buyers: Security, SRE, DX, and CTO sponsors.

### P0 — Trust and reliability foundations (must-have before scale)

These items directly support policy correctness, audit trust, and runtime reliability.

#### P0.1 Resource format consistency ✓

**Problem:** The tool registry passed raw tool names (`shell_exec`) to policy evaluation, while policy patterns expected `tool:shell_exec`. Policies with `resource_pattern: "tool:shell_*"` silently failed.

**Fix (done):** Added `policy::resource` module with canonical constructors (`resource::tool()`, `resource::job()`, etc.) and constants (`resource::FACT`, `resource::POLICY`, etc.). Updated all relevant callsites and tests (`tool_glob_pattern_blocks_via_execute`). 475 unit tests pass.

**Files:** `crates/mp-core/src/policy.rs`, `crates/mp-core/src/tools/registry.rs`, `crates/mp-core/src/operations.rs`, `crates/mp-core/src/scheduler.rs`, `crates/mp-core/src/extraction.rs`, `crates/mp-core/src/agent.rs`, `crates/mp-core/src/dsl/mod.rs`, `crates/mp/src/main.rs`

#### P0.2 Fix cron scheduling ✓

**Problem:** `update_schedule()` in `scheduler.rs` always advances `next_run_at` by +60 seconds regardless of cron expression.

**Fix (done):** Parse cron expressions to compute true next run time (`croner` or `cron` crate). Keep +60s fallback only on parse failure. Add tests for `0 9 * * *`, `*/5 * * * *`, `0 0 * * 0`.

**Files:** `crates/mp-core/src/scheduler.rs`, `Cargo.toml`

#### P0.3 Audit query time windows ✓

**Problem:** Audit queries support `limit` but not time-range filters, blocking operational and compliance use.

**Fix (done):** Add time-range predicates to audit APIs/queries and expose them in CLI + MCP paths.

**Files:** `crates/mp-core/src/operations.rs`, `crates/mp-core/src/store/log/`, `crates/mp/src/cli.rs`

---

### P1 — Retrieval quality and sidecar parity (must-have for developer trust)

MCP sidecar is primary interface; search quality must match CLI behavior.

#### P1.1 Vector search over MCP ✓

**Problem:** `op_memory_search` in `operations.rs` passes `None` for `query_embedding`, so sidecar search is text-only.

**Fix (done):** Compute query embedding in `op_memory_search` before calling `search::search()`.

**Files:** `crates/mp-core/src/operations.rs`, `crates/mp-core/src/store/embedding.rs`

#### P1.2 FTS5 for all search sources ✓

**Problem:** Only `facts` has FTS5; other sources fall back to `LIKE`, hurting recall and latency.

**Fix:**
1. Add FTS5 virtual tables for `messages`, `tool_calls`, `policy_audit`, and `chunks`.
2. Populate/update via triggers or write-path updates.
3. Update `search.rs` to use FTS5 instead of `LIKE`.

**Files:** `crates/mp-core/src/schema.rs`, `crates/mp-core/src/search.rs`, `crates/mp-core/src/store/log/`

#### P1.3 Ensure `facts_fts` migration ✓

**Problem:** `facts_fts` may be absent depending on initialization path.

**Fix:** Add guaranteed migration for `facts_fts` with backfill.

**Files:** `crates/mp-core/src/schema.rs`

#### P1.4 Add scratch to search ✓

**Problem:** Session scratch is missing from `SEARCH_SOURCES`.

**Fix:** Add scratch source with recency/session weighting.

**Files:** `crates/mp-core/src/search.rs`

---

### P2 — Multi-agent scope correctness and sync completeness (must-have for enterprise rollout)

Without scope-aware sync, fleet behavior is unsafe or inconsistent.

#### P2.1 Fact scoping ✓

**Problem:** `FactScope` exists but `facts` table and sync paths do not enforce it.

**Fix:**
1. Add `scope TEXT NOT NULL DEFAULT 'shared'` to `facts`.
2. Wire `scope` through `NewFact` and extraction.
3. Sync only `shared`/`protected`; keep `private` local.
4. Enforce `can_access_fact()` on reads.

**Files:** `crates/mp-core/src/schema.rs`, `crates/mp-core/src/store/facts.rs`, `crates/mp-core/src/extraction.rs`, `crates/mp-core/src/sync.rs`, `crates/mp-core/src/gateway.rs`

#### P2.2 Knowledge sync ✓

**Problem:** `documents`, `chunks`, `knowledge_list` not in `DEFAULT_SYNC_TABLES`.

**Fix:** Add scoped knowledge sync (likely `documents` + `chunks`, with size and scope policy).

**Files:** `crates/mp-core/src/sync.rs`, `crates/mp-core/src/schema.rs`

#### P2.3 Jobs sync across fleet ✓

**Problem:** `jobs` table missing from sync tables, so "define once, propagate fleet-wide" is not true.

**Fix:** Add `jobs` sync with conflict semantics; keep `job_runs` local.

**Files:** `crates/mp-core/src/sync.rs`

---

### P3 — Fleet operations MVP (high-value monetization surface)

This is the minimum enterprise control plane surface for platform teams.

#### P3.1 `mp fleet init --template`

Provision agents from reusable templates (persona, policies, tools, seed knowledge).

#### P3.2 `mp fleet push-policy`

Push signed policy bundles fleet-wide with rollback support.

#### P3.3 `mp fleet audit`

Aggregate policy/audit across agents with time filters and JSON export.

#### P3.4 `mp fleet list` / `mp fleet status`

Show health, sync status, and config drift.

#### P3.5 Agent grouping/tags

Enable scoped operations (`--scope team:infra`) for policy and knowledge rollout.

**Files (P3):** `crates/mp/src/cli.rs` (`Fleet` subcommand), `crates/mp-core/src/operations.rs`, `crates/mp-core/src/schema.rs`, new `crates/mp-core/src/fleet.rs`

---

### P4 — Enterprise controls and licensing pack (enterprise tier)

Ship after P0-P3 to align with enterprise procurement requirements.

#### P4.1 Identity and access

SSO/SAML, SCIM, RBAC, role separation (security admin vs platform operator vs developer).

#### P4.2 Compliance evidence pipeline

SIEM export, retention controls, policy attestation bundles, signed audit snapshots.

#### P4.3 Commercial packaging

Define Community / Team / Enterprise boundaries around fleet controls, org governance, and support/SLA.

---

## 90-Day Execution Plan

### Days 0-30

- ✅ Completed P0.2 cron correctness
- ✅ Completed P0.3 audit time windows
- ✅ Completed P1.1 vector parity over MCP

### Days 31-60

- ✅ Completed P1.2/P1.3/P1.4 search parity package
- 🚧 In progress: P2.1 fact scoping migration + read-path enforcement

### Days 61-90

- ✅ Completed P2.2/P2.3 sync completeness
- Ship first P3 slice: `fleet init`, `fleet push-policy`, `fleet audit` (MVP)

---

## Prioritization Rules (for future roadmap changes)

1. No new feature surface ahead of trust/reliability gaps in P0.
2. No fleet features without scope-safe sync behavior from P2.
3. Enterprise licensing features must map to explicit buyer pain (security, SRE, compliance), not generic "enterprise" labels.
4. Each roadmap item must include measurable success criteria (latency, accuracy, policy miss rate, drift rate, or operational MTTR).

---

## Not on this roadmap (acknowledged gaps, lower priority)

| Gap | Notes |
|-----|-------|
| Fact decay | Confidence only goes up. Needs a half-life or time-based decay. |
| Contradiction detection | `DeduplicationDecision::Delete` exists but is never triggered. |
| Trust-level policy integration | `trust_level` on agents isn't wired into policy evaluation. |
| Pipeline jobs | `JobType::Pipeline` is a stub with no real logic. |
