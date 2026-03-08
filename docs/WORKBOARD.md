# Moneypenny — Central Workboard

Status: active single source of truth for outstanding work.

---

## How To Use

- Track only **outstanding** items here.
- Keep `docs/PLAN.md` strategic and milestone-oriented.
- When work ships, remove or re-scope the item here first.

---

## M1/M3 Tail — Canonical + Governance

- [ ] Enforce idempotency keys for mutating operations with durable key store and deterministic replay responses.
- [ ] Add explicit idempotency enforcement state in audit records (not only response `_meta`).
- [ ] Add configurable operation-level hook registry (pre/post) beyond current baseline guardrails.
- [ ] Expand canonical operation coverage for remaining control-plane families:
  - [ ] `memory.fact.add`, `memory.fact.update`, `memory.search`
  - [ ] `policy.evaluate`, `policy.explain`
  - [ ] `audit.query`, `audit.append`
  - [ ] `session.resolve`, `session.list`
  - [ ] `js.tool.add`, `js.tool.list`, `js.tool.delete`

## M2 — Ingest + Projection

- [ ] Expand projection depth with normalized structured fields (token/cost/provider/model/correlation semantics).
- [ ] Add extraction pass over imported conversations to promote durable facts from external logs.
- [ ] Add replay safety command ergonomics (clear run selection/filtering, operator-safe preview defaults).

## Surface Parity (Adapters)

- [ ] Add stdio sidecar operation endpoint for CLI runtime integration.
- [ ] Map MCP adapter to canonical operations (translation only, no business logic).
- [ ] Expose canonical operations via HTTP/gRPC parity layer.
- [ ] Add parity contract tests across CLI/MCP/API/ingest for same-op same-outcome guarantees.

## M4 — Agent-First JS Jobs

- [ ] Define schema for agent-generated job specs.
- [ ] Implement "plan -> confirm -> apply" flow for agent job creation.
- [ ] Ensure JS job creation uses same canonical handlers as CLI/API/MCP.
- [ ] Add validation tests for natural-language-to-job workflows.

## Explicit Exceptions (Intentional For Now)

- [ ] `mp init` owns filesystem/config bootstrapping (while delegating agent registry mutation through canonical ops).
- [ ] Chat/send/tool runtime log-plane writes remain direct store writes (not operator control-plane ops).
- [ ] Sync replication commands remain data-plane operations, not canonical capability ops.
- [ ] Placeholder commands still unimplemented: `ingest --url`, `policy load`, `audit export`.

## Deferred

- [ ] Additional channel adapters beyond current set.
- [ ] Expanded web UI admin surfaces (memory/policy/audit browsers).
- [ ] Marketplace/distribution packaging work.
