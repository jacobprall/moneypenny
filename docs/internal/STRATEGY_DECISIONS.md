# Strategy Decisions

Durable strategic decisions only. Historical implementation detail is intentionally excluded.

---

## Positioning

- Primary: enterprise-grade dynamic intelligence layer for agents.
- Secondary: first-party runtime remains a reference experience.
- Narrative: make every runtime smarter, safer, and stateful by default.

---

## Non-Negotiable Decisions

1. **Capability-first architecture**
   - One canonical operation contract across CLI, MCP, API, ingest.
   - Adapters translate protocol; they do not own business logic.

2. **Policy-authoritative execution**
   - All mutating/policy-relevant operations run through one governed path.
   - Hooks are middleware; policy is the allow/deny authority.

3. **Event-sourced integration**
   - Preserve all external events raw for replay and forensics.
   - Project recognized events into native query tables.

4. **Runtime-agnostic integration**
   - Design for OpenClaw and any CLI/runtime via stdio, MCP, or HTTP.
   - No runtime-specific branching in core logic.

5. **Rust kernel + governed JS extension surface**
   - Rust owns memory/policy/audit/projection.
   - JS provides programmable jobs/hooks under policy and audit.

6. **Agent-first operation model**
   - Natural language compiles to same canonical operations as operator flows.
   - No hidden "agent-only" mutating pathways.

---

## Deprioritized

- Competing as a broad orchestration/channel framework first.
- Expanding non-core surfaces before integration parity is solved.
- Duplicated policy engines or duplicated execution paths.

---

## Current Strategic Priorities

1. Canonical operation handlers and envelopes.
2. Cross-surface parity (CLI/MCP/API/ingest).
3. OpenClaw pre/post integration with replay-safe ingestion.
4. Agent-created JS jobs with policy-governed scheduling.

---

## Source Of Truth

- Architecture: `docs/SPEC_CURRENT.md`
- OpenClaw contract: `docs/OPENCLAW_INTEGRATION.md`
- Central backlog: `docs/WORKBOARD.md`
