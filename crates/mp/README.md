# mp

`mp` is the workspace binary crate that exposes the Moneypenny CLI and runtime entrypoint.

## What this crate does

- Parses CLI arguments and routes subcommands.
- Loads `moneypenny.toml` configuration and initializes logging.
- Boots runtime services (gateway, channels, workers, sidecar/MCP mode).
- Bridges the high-level command surface to `mp-core` and `mp-llm`.

## Key modules

- `src/main.rs`: top-level command dispatch and runtime orchestration.
- `src/cli.rs`: command and argument definitions.
- `src/adapters.rs`: channel/runtime adapters.
- `src/domain_tools.rs`: domain-specific tool wiring.

## Common commands

From the workspace root:

```bash
cargo run -p mp -- init
cargo run -p mp -- start
cargo run -p mp -- chat
cargo run -p mp -- sidecar
```

## Relationship to other crates

- Depends on `mp-core` for core domain logic, data access, policy, and orchestration.
- Depends on `mp-llm` for LLM and embedding provider abstractions.
- Depends on `mp-ext` for SQLite extension initialization used by runtime database connections.
