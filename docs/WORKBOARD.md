# Moneypenny — Central Workboard

Status: active single source of truth for outstanding work.

---

## How To Use

- Track only **outstanding** items here.
- When work ships, remove or re-scope the item here first.

---

## Implementation Queue (Ordered, No Timeline)

1. **Ingest depth (M2)**
   - [x] Expand projection depth with normalized structured fields (token/cost/provider/model/correlation semantics).
   - [x] Add extraction pass over imported conversations to promote durable facts from external logs.
   - [x] Add replay safety command ergonomics (clear run selection/filtering, operator-safe preview defaults).

2. **Runtime integration parity (adapters + contracts)**
   - [x] Add stdio sidecar operation endpoint for CLI runtime integration.
   - [x] Map MCP adapter to canonical operations (translation only, no business logic).
   - [ ] Expose canonical operations via HTTP/gRPC parity layer (HTTP shipped; gRPC pending).
   - [ ] Add parity contract tests across CLI/MCP/API/ingest for same-op same-outcome guarantees (HTTP <-> sidecar parity test shipped; MCP/ingest parity tests pending).

3. **Agent-first JS jobs (M4)**
   - [ ] Define schema for agent-generated job specs.
   - [ ] Implement "plan -> confirm -> apply" flow for agent job creation.
   - [ ] Ensure JS job creation uses same canonical handlers as CLI/API/MCP.
   - [ ] Add validation tests for natural-language-to-job workflows.

4. **Ship placeholder commands**
   - [ ] Implement `ingest --url`.
   - [ ] Implement `policy load`.
   - [ ] Implement `audit export`.

5. **Deferred product surface expansion**
   - [ ] Additional channel adapters beyond current set.
   - [ ] Expanded web UI admin surfaces (memory/policy/audit browsers).
   - [ ] Marketplace/distribution packaging work.

---

## M2 — Ingest + Projection

- [x] Expand projection depth with normalized structured fields (token/cost/provider/model/correlation semantics).
- [x] Add extraction pass over imported conversations to promote durable facts from external logs.
- [x] Add replay safety command ergonomics (clear run selection/filtering, operator-safe preview defaults).

## Surface Parity (Adapters)

- [x] Add stdio sidecar operation endpoint for CLI runtime integration.
- [x] Map MCP adapter to canonical operations (translation only, no business logic).
- [ ] Expose canonical operations via HTTP/gRPC parity layer (HTTP shipped; gRPC pending).
- [ ] Add parity contract tests across CLI/MCP/API/ingest for same-op same-outcome guarantees (HTTP <-> sidecar parity test shipped; MCP/ingest parity tests pending).

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
