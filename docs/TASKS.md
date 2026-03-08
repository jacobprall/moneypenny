# Moneypenny — Task List

> Living task tracker. Updated as work progresses.

---

## M1 — Project Scaffolding ✅

- [x] Workspace & Cargo.toml
- [x] clap CLI with all command stubs wired
- [x] TOML config loading

## M2 — Database Layer & Schema ✅

- [x] All tables created (facts, fact_links, fact_audit, sessions, messages, tool_calls, documents, chunks, scratch, policies, policy_audit, jobs, job_runs, skills, edges)
- [x] `mp init` creates valid agent `.db` with all schemas and `metadata.db`
- [x] Schema migration system (V1 → V2 with `rule_type`/`rule_config` columns)

## M3 — LLM Provider Trait ✅ (with caveat)

**Generation providers:**
- [x] `LlmProvider` async trait design
- [x] `AnthropicProvider` (native Anthropic Messages API): request building, response parsing, streaming, tool call parsing. Default cloud provider.
- [x] `HttpProvider` (OpenAI-compatible): request building, response parsing, streaming, tool call parsing. Generation-only.
- [x] Multi-turn function calling: `Message.tool_calls` field for assistant messages
- [x] Default generation provider: `anthropic` (claude-sonnet-4-20250514)
- [x] `SqliteAiProvider` stub remains for generation; `LocalEmbeddingProvider` now uses sqlite-ai for embeddings

**Embedding providers (local-first):**
- [x] `EmbeddingProvider` trait: `embed()`, `embed_batch()`, `dimensions()`
- [x] `LocalEmbeddingProvider`: GGUF-based local embeddings via sqlite-ai. Default. Ships with `nomic-embed-text-v1.5` (768D, ~274MB).
- [x] `HttpEmbeddingProvider`: OpenAI-compatible `/embeddings` API for opt-in cloud embeddings.
- [x] `EmbeddingConfig` separated from `LlmConfig` in agent config
- [x] `build_embedding_provider()` factory in mp-llm
- [x] `LocalEmbeddingProvider` wired to sqlite-ai `llm_embed_generate()` — lazy GGUF model load, persistent per-provider connection, Arc<Mutex<>> thread safety

## M4 — Memory Stores ✅

- [x] Full CRUD for all four stores (Facts, Log, Knowledge, Scratch)
- [x] Progressive compression (Level 0/1/2)
- [x] Fact audit trail, fact linking, confidence scoring
- [x] Document chunking and skills

## M5 — Search ✅ (mostly)

- [x] RRF fusion (k=60), MMR re-ranking (λ=0.7)
- [x] Cross-store search, Jaccard similarity
- [x] Intent detection with store weighting
- [x] Vector similarity wired via `sqlite-vector` `vector_quantize_scan()` — fused into RRF pipeline alongside FTS5 results when query embedding is present

## M6 — Context Assembly & Token Budget ✅

- [x] Token budget allocator with reserved/flexible split
- [x] Full assembly pipeline (system prompt → policies → session summary → fact pointers → expanded facts → scratch → log → knowledge → current message)
- [x] Dynamic rebalancing for empty scratch, new session, deep session
- [x] Rolling conversation summaries — `maybe_summarize_session()` called after every turn; triggers every 20 messages (10 exchange pairs), summarizes all but the last 10 messages, stores in `sessions.summary`; included in context assembly as "session_summary" segment

## M7 — Policy Engine ✅

**Layer 1 — SQL row-based rules: DONE**
- [x] Configurable default: allow-by-default (dev) or deny-by-default (production) via `policy_mode` in agent config
- [x] Glob pattern matching on actor/action/resource
- [x] Regex matching on SQL content
- [x] Priority ordering, audit trail, SQL filter generation
- [x] Disabled policy skipping

**Layer 2 — Behavioral rules: DONE**
- [x] Schema V2: `rule_type` + `rule_config` columns on policies table
- [x] `rate_limit` rule type: deny when tool call count in a sliding window exceeds max
- [x] `retry_loop` rule type: deny when same tool+args called repeatedly in a window
- [x] `token_budget` rule type: deny when estimated session token usage exceeds limit
- [x] `time_window` rule type: restrict actions to specific hours/days
- [x] Behavioral rules integrate with static rules — if behavioral condition is not triggered, rule is skipped and evaluation continues
- [x] 9 tests covering all behavioral rule types and edge cases

**Future — Layer 3: Rule DSL**
- [ ] Lightweight Polar-inspired DSL (parser + evaluator) — deferred

## M8 — Extraction Pipeline ✅

- [x] ADD/UPDATE/DELETE/NOOP pipeline mechanics
- [x] Context assembly, candidate parsing (JSON), deduplication by Jaccard similarity
- [x] Policy checks per candidate, fact linking, transactional commit with audit trail
- [x] Extraction system prompt (instructs LLM to identify durable facts from conversation)
- [x] `extract_facts()` async function: assemble context → call LLM → parse JSON candidates → run pipeline
- [x] Wired into `cmd_chat` post-response (runs during user think time) and `cmd_send`
- [x] User-visible feedback: "N fact(s) learned" printed after extraction

## M9 — Tool System ✅ (mostly)

- [x] Built-in tools (file I/O, shell exec, HTTP, SQL)
- [x] 14 runtime tools (memory_search, fact_add/update/list, scratch_set/get, knowledge_ingest/list, job_create/list/pause/resume, policy_list, audit_query)
- [x] Tool registry in SQLite, tool lifecycle with policy checks and audit
- [x] Tool execution wired into agent loop with proper JSON Schema tool defs for LLM function calling
- [x] Pre- and post-execution hooks (`ToolHooks`) — pre-hooks can abort or override arguments, post-hooks can transform output; hooks chain in registration order with glob-based tool pattern matching
- [x] MCP tool discovery — pure Rust stdio client (`mp-core/src/mcp.rs`); configure servers in `[[agents.mcp_servers]]`; discovered tools registered in `skills` table and exposed to LLM; dispatch spawns server per call; unreachable servers skipped with warning
- [x] JS tool persistence — `js_tool_add`/`js_tool_list`/`js_tool_delete` runtime tools store scripts in `skills` table; execution via `node -e` or `deno eval`; user-defined tools appear in LLM tool list automatically

## M10 — Agent Loop ✅

- [x] `agent::turn()` implements full loop: message → context assembly → policy → LLM → tool calls → secret redaction → store
- [x] Tested with mock LLM (sync) — 292 tests passing
- [x] `HttpProvider` wired into async `agent_turn()` in the `mp` binary
- [x] Multi-turn tool calling with OpenAI function calling format
- [x] `mp chat` produces real LLM responses with interactive REPL
- [x] `mp send` calls the LLM end-to-end and returns real responses
- [x] Post-response fact extraction on every turn

## M11 — Job Scheduler ✅

- [x] `jobs` table CRUD, all four job types (prompt, tool, js, pipeline)
- [x] `dispatch_job`, retry logic, overlap policy, `pause_job`, history
- [x] `poll_due_jobs` query
- [x] Scheduler polling loop as async task in gateway process (1-second poll interval)
- [x] Per-agent job dispatch with logging

## M12 — CLI Channel & DX ✅

- [x] All CLI commands parsed and routed
- [x] Working commands: `mp agent status`, `mp facts list/search/inspect/delete`, `mp knowledge list/search`, `mp skill add/list/promote`, `mp policy list/add/test/violations`, `mp job list/create/run/pause/history`, `mp audit`, `mp db query/schema`, `mp health`, `mp ingest`
- [x] `mp chat` — full interactive REPL with /help, /facts, /scratch, /session, /quit
- [x] `mp send` — single-message LLM call with response and extraction
- [x] `mp start` — full gateway runtime with scheduler, workers, CLI channel, graceful shutdown

## M13 — Gateway & Multi-Agent ✅ (complete)

**Data layer: DONE**
- [x] Agent registry, message routing, delegation with depth limits
- [x] Fact scope model (private/shared/protected)

**Runtime: DONE**
- [x] `mp start` as long-running gateway process
- [x] Worker isolation — one child process per agent via `mp worker --agent <name>` (hidden subcommand)
- [x] Gateway-to-worker communication protocol (JSON lines over stdin/stdout)
- [x] Scheduler loop running in gateway, dispatching to per-agent DBs
- [x] CLI channel on gateway, forwarding to default agent
- [x] Graceful shutdown via Ctrl-C (kills workers, stops scheduler)
- [x] Inter-worker message routing via `WorkerBus` — workers converted to `tokio::process`, gateway holds `Arc<WorkerBus>` with per-worker async stdin/stdout channels; `delegate_to_agent` tool routes through bus in gateway mode, returns a stub in standalone mode
- [x] Sync integration via `sqlite-sync` CRDTs — tables initialized on every DB open (`facts`, `fact_links`, `skills`, `policies`); `mp sync status/now/push/pull/connect`; gateway auto-sync loop; local P2P via payload files; optional cloud sync via `cloudsync_network_*`

## M14–M17 — Adapters & UI ⚠️ (in progress)

- [x] HTTP API adapter — REST (`POST /v1/chat`), SSE (`GET /v1/chat/stream`), WebSocket (`/v1/ws`), health (`GET /health`), optional Bearer auth
- [x] Slack adapter — Events API webhook, HMAC-SHA256 signature verification, `chat.postMessage` replies, per-user session continuity
- [x] Discord adapter — Interactions endpoint, Ed25519 signature verification, deferred response + follow-up webhook, per-user session continuity
- [x] Telegram adapter — Long-polling (`getUpdates`), `sendMessage` replies, per-chat session continuity
- [x] Web UI — conversation chat (React + shadcn, served at `/` when `web-ui/dist` or `web_ui_dir` set)
- [ ] Web UI — memory browser, audit viewer, policy editor
- [ ] WASM runtime via sqlite-wasm

---

## Cross-Cutting: SQLite Extension Static Linking

> Ref: `plans/wire_sqlite_extensions.plan.md`

**Phase 1 — Vendor sources: DONE**
- [x] All 7 extensions added as git submodules under `vendor/`
- [x] `build.rs` updated to use `vendor/` base path
- [x] `mcp-ffi` Cargo dependency repointed to `vendor/sqlite-mcp`
- [x] Workspace `Cargo.toml` excludes `vendor/`
- [x] README updated with `--recurse-submodules` clone instructions

**Phase 2–4 — Compilation & linking: DONE ✅**
- [x] Fix sqlite-sync SQLite header version mismatch — fixed by injecting bundled SQLite 3.49.1 headers via `DEP_SQLITE3_INCLUDE`
- [x] `libsqlite3-sys` added as explicit direct dependency in `mp-ext/Cargo.toml` to propagate `DEP_SQLITE3_INCLUDE`
- [x] `mcp-ffi` `crate-type` extended to `["staticlib", "rlib"]` so it can be used as a Rust dependency
- [x] All 7 C/C++ extensions compile cleanly
- [x] CMake builds for llama.cpp + whisper.cpp + miniaudio (sqlite-ai) — all bundled
- [x] Runtime initialization verified: all 7 extensions log `initialized` on first DB open
- [x] Wire `sqlite-vector` embeddings into search — `init_vector_indexes` on every DB open, `embed_pending` rebuilds quantized index after extraction/ingest
- [x] Wire `sqlite-ai` into `LocalEmbeddingProvider` — `llm_embed_generate()` called via `spawn_blocking`, model loaded lazily

---

## Priority Order

**Completed — all hard items done:**
1. ~~Wire `HttpProvider` into agent loop~~ ✅
2. ~~`mp chat` / `mp send` end-to-end~~ ✅
3. ~~Behavioral policy rules~~ ✅
4. ~~Post-response extraction with real LLM~~ ✅
5. ~~Full gateway runtime with worker isolation~~ ✅
6. ~~Scheduler polling loop~~ ✅

**Next:**
1. ~~Fix extension compilation~~ ✅
2. ~~Wire `sqlite-vector` into search~~ ✅
3. ~~Wire `sqlite-ai` into `LocalEmbeddingProvider`~~ ✅
4. ~~Rolling conversation summaries (M6)~~ ✅
5. ~~Inter-worker message routing in gateway~~ ✅

**Later:**
1. Channel adapters (M14–M17)
2. Policy DSL (M7 Layer 3)
3. Web UI, WASM, skill marketplace
4. MCP tool discovery (blocked on sqlite-mcp extension)
5. ~~Sync via sqlite-sync~~ ✅

---

## Testing

- [x] **Integration tests** (`crates/mp/tests/`) — run `mp` binary in temp dirs; `common` helpers for init and `run_mp_with_config`
- [x] **Init** — `integration_init.rs`: config/data dir/agent DB/metadata/models created; init refuses to overwrite
- [x] **Commands** — `integration_commands.rs`: health, facts list, sync status, db schema, agent status, policy list, job list after init
- [x] **Sync** — `integration_sync.rs`: sync status, sync now (no peers), sync push/pull between two DBs (graceful skip if payload_save fails in env)
- [x] **E2E** — `e2e_send.rs`: `mp send` runs without panic (no API key required; may fail at provider)
- [x] **HTTP API e2e** — `e2e_http.rs`: patch config to enable HTTP channel and disable CLI, spawn `mp start`, wait for server, GET /health (assert 200 + body), POST /v1/chat (assert 200 or skip), then kill gateway
