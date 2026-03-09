# mp-llm

`mp-llm` provides model-provider abstractions for text generation and embeddings.

## What this crate does

- Defines provider traits and shared request/response types.
- Implements provider backends for Anthropic and OpenAI-compatible HTTP APIs.
- Provides local embedding support and SQLite AI-backed local inference hooks.
- Exposes provider factory functions used by runtime config.

## Key modules

- `src/provider.rs`: core traits (`LlmProvider`, `EmbeddingProvider`) and interfaces.
- `src/types.rs`: common message, tool call, usage, and generation structures.
- `src/anthropic.rs`: Anthropic API adapter.
- `src/http.rs`: OpenAI-compatible HTTP adapter.
- `src/local_embed.rs`: local + HTTP embedding provider implementations.
- `src/sqlite_ai.rs`: SQLite AI local provider integration.
- `src/lib.rs`: provider factory and crate exports.

## Test and build

From the workspace root:

```bash
cargo check -p mp-llm
cargo test -p mp-llm
```

## Relationship to other crates

- Used by `mp` to construct runtime generation and embedding providers from config.
- May use `mp-ext` capabilities when local SQLite-backed model paths are selected.
