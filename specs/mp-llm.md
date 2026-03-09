# mp-llm — LLM Abstraction Layer Specification

> **Crate:** `crates/mp-llm/` | **Type:** Library | **Dependencies:** reqwest, tokio, async-trait, serde, rusqlite, mp-ext

`mp-llm` provides a provider-agnostic interface to LLM generation and embedding services. It defines shared types, two async traits (`LlmProvider` and `EmbeddingProvider`), and concrete implementations for Anthropic, OpenAI-compatible HTTP, and local GGUF models.

## Module Map

```
mp-llm/src/
├── lib.rs           # Provider factories + re-exports
├── types.rs         # Shared domain types (Message, ToolCall, etc.)
├── provider.rs      # LlmProvider + EmbeddingProvider traits
├── anthropic.rs     # Anthropic Messages API provider
├── http.rs          # OpenAI-compatible chat completions provider
├── local_embed.rs   # Local GGUF + HTTP embedding providers
└── sqlite_ai.rs     # Local generation provider (stub)
```

## Key Design Decisions

- **Generation and embedding are decoupled.** You can use Anthropic for generation and a local GGUF model for embeddings. This is the default configuration.
- **Provider selection is string-based** (`"anthropic"`, `"http"`, `"local"`) matching config file values.
- **Both traits are object-safe** (`Box<dyn LlmProvider>`, `Box<dyn EmbeddingProvider>`) for runtime polymorphism.
- **Streaming is provider-dependent.** Anthropic and HTTP providers support SSE streaming; local does not.

---

## types.rs — Shared Domain Types

**File:** `crates/mp-llm/src/types.rs`

All provider-agnostic data types used across LLM backends.

### Types

| Type | Fields | Purpose |
|---|---|---|
| `Role` | Enum: `System`, `User`, `Assistant`, `Tool` | Message role, serialized to lowercase |
| `Message` | `role`, `content`, `tool_call_id?`, `tool_calls?` | Single message in a conversation |
| `ToolDef` | `name`, `description`, `parameters` (JSON Schema Value) | Tool definition for LLM tool-use |
| `GenerateConfig` | `max_tokens?`, `temperature` (0.7), `stop?` | Generation parameters |
| `ToolCall` | `id`, `name`, `arguments` (JSON string) | LLM-requested tool invocation |
| `GenerateResponse` | `content?`, `tool_calls`, `usage?` | Non-streaming response |
| `Usage` | `prompt_tokens`, `completion_tokens`, `total_tokens` | Token accounting |
| `StreamEvent` | Enum: `Delta(String)`, `ToolCall(ToolCall)`, `Done(Usage)` | Streaming event |

### Convenience Constructors on `Message`

- `Message::system(content)`, `Message::user(content)`, `Message::assistant(content)`
- `Message::assistant_with_tool_calls(content, tool_calls)`
- `Message::tool(tool_call_id, content)`

---

## provider.rs — Provider Traits

**File:** `crates/mp-llm/src/provider.rs`

### `LlmProvider` (async trait, Send + Sync)

| Method | Signature | Purpose |
|---|---|---|
| `generate` | `(&self, messages, tools?, config?) -> Result<GenerateResponse>` | Non-streaming generation |
| `generate_stream` | `(&self, messages, tools?, config?) -> Result<StreamResult>` | Streaming generation |
| `embed` | `(&self, text) -> Result<Vec<f32>>` | Legacy single-text embedding |
| `supports_streaming` | `(&self) -> bool` | Capability query |
| `name` | `(&self) -> &str` | Human-readable identifier |

`StreamResult = Pin<Box<dyn Stream<Item = Result<StreamEvent>> + Send>>`

### `EmbeddingProvider` (async trait, Send + Sync)

| Method | Signature | Purpose |
|---|---|---|
| `embed` | `(&self, text) -> Result<Vec<f32>>` | Single text embedding |
| `embed_batch` | `(&self, texts) -> Result<Vec<Vec<f32>>>` | Batch embedding (default: sequential) |
| `dimensions` | `(&self) -> usize` | Vector dimensionality |
| `name` | `(&self) -> &str` | Identifier |

---

## lib.rs — Factory Functions

**File:** `crates/mp-llm/src/lib.rs`

### `build_provider(provider_type, api_base?, api_key?, model?) -> Box<dyn LlmProvider>`

| `provider_type` | Creates |
|---|---|
| `"anthropic"` | `AnthropicProvider` (default model: `claude-sonnet-4-20250514`) |
| `"http"` | `HttpProvider` (default: `gpt-4o-mini` at `api.openai.com/v1`) |
| `"local"` | `SqliteAiProvider` (stub — not yet implemented) |

### `build_embedding_provider(provider_type, model, path?, dims, api_base?, api_key?) -> Box<dyn EmbeddingProvider>`

| `provider_type` | Creates |
|---|---|
| `"local"` | `LocalEmbeddingProvider` (GGUF via sqlite-ai, default: nomic-embed-text-v1.5, 768 dims) |
| `"http"` | `HttpEmbeddingProvider` (OpenAI-compatible `/embeddings` endpoint) |

### Re-exports

- `f32_slice_to_blob(v: &[f32]) -> Vec<u8>` — Encode f32 slice to little-endian blob for SQLite storage
- `parse_f32_blob(blob: &[u8]) -> Vec<f32>` — Decode little-endian f32 blob

---

## anthropic.rs — Anthropic Messages API

**File:** `crates/mp-llm/src/anthropic.rs`

### `AnthropicProvider`

- **API version:** `2023-06-01`
- **Default model:** `claude-sonnet-4-20250514`
- **Default max_tokens:** 8192
- **Auth:** `x-api-key` header

### Wire Format Translation

- System messages → top-level `"system"` field (concatenated if multiple)
- Tool results → `"user"` messages with `"tool_result"` content blocks
- Consecutive tool results merged into single user message (Anthropic requirement)
- Assistant + tool calls → content blocks with `"tool_use"` type
- Tool call `arguments` parsed from JSON string to `Value` for `"input"` field

### Streaming

Full SSE parser via `async_stream`. Handles: `message_start`, `content_block_start`, `content_block_delta`, `content_block_stop`, `message_delta`, `message_stop`. Tool call JSON accumulated incrementally via `ToolAccumulator`.

`embed()` returns error — Anthropic has no embeddings API.

---

## http.rs — OpenAI-Compatible HTTP Provider

**File:** `crates/mp-llm/src/http.rs`

### `HttpProvider`

- **Default base:** `https://api.openai.com/v1`
- **Default model:** `gpt-4o-mini`
- **Auth:** Bearer token
- **Endpoint:** `POST /chat/completions`

Compatible with: OpenAI, Ollama, vLLM, LiteLLM, and any OpenAI-compatible API.

### Streaming

SSE parser handles `data: [DONE]` sentinel. Tool calls accumulated by index across delta chunks.

`embed()` returns error directing to `EmbeddingProvider`.

---

## local_embed.rs — Embedding Providers

**File:** `crates/mp-llm/src/local_embed.rs`

### `LocalEmbeddingProvider`

On-device GGUF embeddings via the sqlite-ai extension.

- Opens an in-memory SQLite connection with `mp-ext` extensions loaded
- Lazy model load on first `embed()` call via `llm_model_load` + `llm_context_create_embedding`
- Subsequent calls use warm model via `llm_embed_generate`
- Thread-safe via `Arc<Mutex<EmbedState>>`
- Runs in `spawn_blocking` to avoid blocking the async runtime

### `HttpEmbeddingProvider`

Remote embeddings via OpenAI-compatible `/embeddings` endpoint.

- `embed()` — single text
- `embed_batch()` — optimized: sends all texts in one HTTP request

### Blob Utilities

- `parse_f32_blob(blob) -> Vec<f32>` — Decode LE f32 blob from SQLite
- `f32_slice_to_blob(v) -> Vec<u8>` — Encode f32 slice to LE blob

---

## sqlite_ai.rs — Local Generation Provider (Stub)

**File:** `crates/mp-llm/src/sqlite_ai.rs`

### `SqliteAiProvider`

Placeholder for fully local inference via sqlite-ai extension. All methods bail with "not yet implemented". Exists so config files can reference `provider = "local"` today.
