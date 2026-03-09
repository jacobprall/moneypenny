# mp-core

`mp-core` contains the core domain model and runtime logic for Moneypenny.

## What this crate does

- Defines schema and store APIs for facts, messages, knowledge, scratch, and audit logs.
- Implements policy enforcement, search, extraction, scheduling, sync, and operations.
- Hosts agent lifecycle primitives and channel/gateway-facing orchestration.
- Provides MCP-facing core functionality consumed by the `mp` binary.

## Key modules

- `src/lib.rs`: module exports for the core runtime surface.
- `src/schema.rs`: schema management and migration-related logic.
- `src/store/`: persistence layer for facts, logs, knowledge, embeddings, and redaction.
- `src/policy.rs`: policy evaluation and enforcement flow.
- `src/operations.rs`: canonical runtime operations and command handlers.
- `src/search.rs`: retrieval and search logic across memory stores.
- `src/scheduler.rs`: scheduled job execution primitives.
- `src/mcp.rs`: MCP integration hooks.

## Test and build

From the workspace root:

```bash
cargo check -p mp-core
cargo test -p mp-core
```

## Relationship to other crates

- Consumed directly by `mp` for runtime execution.
- Uses `mp-ext` during tests/runtime paths that require bundled SQLite extension capabilities.
