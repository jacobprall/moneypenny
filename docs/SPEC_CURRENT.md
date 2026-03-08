# Moneypenny — Current Architecture Spec

Status: active

---

## System Role

Moneypenny is the enterprise-grade dynamic intelligence layer for agents:
- durable memory,
- policy-governed execution,
- explainable audit,
- replay-safe event ingestion,
- runtime-agnostic integration surfaces.

---

## Core Architecture

### 1) Canonical Operation Layer

All key behavior is expressed as canonical operations (capabilities), not transport-specific handlers.

Implemented now (v1):
- `job.create`, `job.list`, `job.run`, `job.pause`
- `policy.add`
- `knowledge.ingest`
- `skill.add`, `skill.promote`
- `fact.delete`
- `agent.create`, `agent.config`, `agent.delete`
- `ingest.events`, `ingest.status`, `ingest.replay`

Planned expansion:
- `memory.search`, `memory.fact.add`, `memory.fact.update`
- `policy.evaluate`, `policy.explain`
- `audit.query`, `audit.append`
- `js.tool.add`, `js.tool.list`
- `session.resolve`, `session.list`

### 2) Unified Execution Pipeline

Every mutating or policy-relevant operation follows:
1. Parse operation envelope
2. Resolve actor/session/tenant context
3. Pre-policy evaluation
4. Optional pre-hooks
5. Handler execution
6. Optional post-hooks
7. Redaction + audit write
8. Standard result envelope

### 3) Data Plane

- Raw external event retention for replay and forensics.
- Deterministic projection into native tables (`sessions`, `messages`, `tool_calls`, `policy_audit`).
- Idempotent ingest with event ID/hash dedupe.

### 4) Storage and Runtime

- SQLite-backed state with local-first operation.
- Rust kernel for execution, policy, audit, projection, and core memory logic.
- Governed JS extension surface (jobs/hooks/tool scripts) under policy and audit constraints.

---

## Integration Surfaces

Current state:

- CLI maps mutating control-plane behavior through canonical operations.
- JSONL/event ingest maps to canonical ingest operations (`ingest.events`, `ingest.status`, `ingest.replay`).

In progress / planned:

- Stdio sidecar operation endpoint
- MCP translation adapter to canonical operations
- HTTP/gRPC parity layer

Target principle remains: no adapter-specific business logic.

## Policy Model

- Policy is authoritative for allow/deny/audit decisions.
- Hooks are programmable middleware and cannot bypass policy.
- Denials and decisions must be explainable and queryable.

---

## Agent-First Requirements

- Natural-language requests compile to canonical operations.
- Agent-created JS jobs use the same operation path as CLI/API/MCP.
- No hidden "agent-only" mutating paths.

---

## OpenClaw Integration (Current Scope)

- Pre-execution query bridge for memory/policy calls.
- Post-execution event append + deterministic projection.
- Replay-safe ingestion from OpenClaw JSONL logs, including run replay + dry-run preflight.

---

## Related Docs

- Interface contract: `docs/INTERFACE_RFC.md`
- OpenClaw contract: `docs/OPENCLAW_INTEGRATION.md`
- Strategy decisions: `docs/STRATEGY_DECISIONS.md`
- Central backlog: `docs/WORKBOARD.md`
- Plan: `docs/PLAN.md`
