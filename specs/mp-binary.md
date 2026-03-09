# mp (Binary Crate) — Specification

> **Crate:** `crates/mp/` | **Type:** Binary | **Dependencies:** mp-core, mp-llm, mp-ext, clap, axum, tokio, reqwest, and more

The `mp` crate is the top-level binary that wires everything together: CLI parsing, command handlers, the async agent turn loop, worker process management, channel adapters, and the sidecar protocol.

## Module Map

```
mp/src/
├── main.rs       # Entry point + all command handlers + agent turn loop + workers (~3700 lines)
├── cli.rs        # clap CLI definition
└── adapters.rs   # HTTP/Slack/Discord/Telegram transport adapters
```

---

## cli.rs — CLI Definition

**File:** `crates/mp/src/cli.rs`

All commands defined via clap derive macros.

### Top-Level Commands

| Command | Purpose |
|---|---|
| `init` | Create `moneypenny.toml` + data directory + download embedding model |
| `start` | Start gateway (spawns worker per agent) |
| `stop` | Stop gateway |
| `chat [agent] [--session-id]` | Interactive REPL with session support |
| `send <agent> <message>` | One-shot message |
| `sidecar` | Canonical operations over stdio JSONL |
| `health` | System health check |
| `worker` (hidden) | Internal worker subprocess mode |

### Subcommand Groups

| Group | Subcommands |
|---|---|
| `agent` | `list`, `create`, `delete`, `status`, `config` |
| `session` | `list` |
| `facts` | `list`, `search`, `inspect`, `expand`, `reset-compaction`, `promote`, `delete` |
| `ingest` | (flags for document/URL/OpenClaw, `--replay`, `--dry-run`, `--status`) |
| `knowledge` | `search`, `list` |
| `skill` | `add`, `list`, `promote` |
| `policy` | `list`, `add`, `test`, `violations`, `load` |
| `job` | `create`, `list`, `trigger`, `pause`, `history` |
| `audit` | `search`, `export` (json/csv/sql) |
| `sync` | `status`, `now`, `push`, `pull`, `connect` |
| `db` | `query`, `schema` |

---

## main.rs — Application Core

**File:** `crates/mp/src/main.rs`

### Entry Point

`main()` parses CLI args, loads `moneypenny.toml`, initializes logging, and dispatches to the appropriate `cmd_*` handler.

### Database Initialization: `open_agent_db()`

Called for every command that needs an agent database. Sequence:

1. `mp_core::db::open(path)` — opens SQLite with WAL + pragmas
2. `mp_ext::init_all_extensions(&conn)` — loads all 7 extensions
3. `mp_core::schema::init_agent_db(&conn)` — runs schema migrations
4. `mp_core::schema::init_vector_indexes(&conn, dims)` — registers vector indexes
5. `mp_core::schema::init_sync_tables(&conn)` — enables CRDT tracking
6. `mp_core::tools::registry::register_builtins(&conn)` — seeds built-in tools
7. `mp_core::tools::registry::register_runtime_skills(&conn)` — seeds runtime tools
8. `mp_core::mcp::discover_and_register(&conn, servers)` — discovers MCP tools

### Agent Turn Loop: `agent_turn()`

The async production turn loop (distinct from `mp_core::agent::turn` which is sync/test-only).

#### Flow

1. Assemble context via `mp_core::context::assemble` (128K token budget)
2. Policy-check the incoming message
3. **Intent classification:**
   - `is_text_first_intent()` — detects queries that should skip tools entirely
   - `has_write_confirmation()` — detects user confirming a write action
   - `allow_multi_tool_calls()` — detects explicit multi-tool requests
4. **Tool filtering:**
   - Text-first intent → clear all tools, force direct response
   - Unconfirmed writes → retain only read-only tools
5. **LLM generation loop** (up to 10 rounds):
   - Generate with tools
   - Parse tool calls from response
   - For each tool call: policy check → execute → collect result
   - Limits: 2 tool calls normally, 8 if multi-tool opted in
   - Enrich `memory_search` args with embedding vector for hybrid search
6. **Loop-break guards:**
   - 3 consecutive tool failures
   - 4 same-tool streak
   - Total call budget exceeded
   - On break: final tools-disabled LLM call for natural language answer
7. **Post-response:**
   - Redact secrets via `mp_core::store::redact`
   - Store assistant message
8. **Post-turn async tasks:**
   - `extract_facts()` — LLM-driven fact extraction
   - `embed_pending()` — embed unembedded facts + chunks
   - `maybe_summarize_session()` — rolling session summarization

### Worker Process Model

Each agent runs in its own child process for isolation.

#### Key Types

| Type | Purpose |
|---|---|
| `WorkerHandle` | Manages child process lifecycle (pid, handle, shutdown) |
| `WorkerChannel` | Piped stdin/stdout to a worker process |
| `WorkerBus` | `Arc` router: `Mutex<HashMap<agent_name, WorkerChannel>>`. Routes JSONL messages to workers. |

#### `spawn_worker(agent_name)`
Launches `mp worker --agent <name>` as a child process with piped stdio.

#### `cmd_worker()`
Worker main loop:
1. Read JSONL request from stdin
2. Call `agent_turn()`
3. Write JSONL response to stdout
4. Run background tasks (fact extraction, embedding, summarization)

#### `WorkerBus::route()` / `route_full()`
Serialize request → write to worker's stdin → read response from stdout. Sequential per-worker (mutex-protected).

### Gateway: `cmd_start()`

1. Spawn one worker per configured agent
2. Create `DispatchFn` and `OpDispatchFn` closures routing through `WorkerBus`
3. Start `adapters::run_http_server` (if HTTP channel configured)
4. Start `adapters::run_telegram_polling` (if Telegram configured)
5. Start `run_scheduler` (cron job polling loop)
6. Handle SIGINT/SIGTERM for graceful shutdown

### Sidecar: `cmd_sidecar()`

Reads canonical operation JSON from stdin, executes via `mp_core::operations::execute()`, writes response JSON to stdout. Also handles MCP JSON-RPC translation for `tools/list` and `tools/call`.

### Interactive Chat: `cmd_chat()`

Interactive REPL with:
- Readline input
- Session commands: `/session`, `/new`, `/exit`
- Session continuity via `--session-id` flag
- Fact extraction + embedding after each turn

### Other Command Handlers

| Handler | What it does |
|---|---|
| `cmd_init` | Creates config template + data directory |
| `cmd_agent` | Agent CRUD against metadata DB |
| `cmd_facts` | Fact listing, search, inspection, expansion, compaction reset, promotion, deletion |
| `cmd_ingest` | Document/URL/OpenClaw ingestion with chunking, embedding, replay, dry-run |
| `cmd_knowledge` | Knowledge search + listing |
| `cmd_skill` | Skill add + list + promote |
| `cmd_policy` | Policy CRUD + testing + violation viewing + file loading |
| `cmd_job` | Job CRUD + trigger + history |
| `cmd_audit` | Audit search + export (json/csv/sql) |
| `cmd_sync` | CRDT sync operations (status, now, push, pull, connect) |
| `cmd_db` | Read-only SQL queries + schema inspection |
| `cmd_health` | System health report |

### Helper Functions

| Function | Purpose |
|---|---|
| `resolve_agent(config, name)` | Find agent in config by name (or first) |
| `build_provider(agent)` | Delegates to `mp_llm::build_provider` |
| `build_embedding_provider(config, agent)` | Delegates to `mp_llm::build_embedding_provider` |
| `build_llm_tools()` | Returns static catalog of ~15 tools for LLM tool-use |
| `enrich_memory_search_args_with_embedding()` | Adds embedding vector to search args |
| `extract_facts()` | LLM-driven fact extraction from conversation |
| `embed_pending()` | Embed unembedded facts + chunks |
| `maybe_summarize_session()` | Rolling session summarization |

---

## adapters.rs — Channel Adapters

**File:** `crates/mp/src/adapters.rs`

### Dispatch Types

```rust
type DispatchFn = Arc<dyn Fn(agent, message, session_id?) -> Future<(response, session_id)>>;
type OpDispatchFn = Arc<dyn Fn(agent, op_json) -> Future<String>>;
```

### `run_http_server()`

Starts a combined Axum server with:

| Endpoint | Purpose |
|---|---|
| `POST /v1/chat` | JSON request/response chat |
| `POST /v1/ops` | Canonical operation endpoint |
| `GET /v1/chat/stream` | SSE streaming chat |
| `GET /v1/ws` | WebSocket chat |
| `GET /health` | Health check |
| `POST /slack/events` | Slack Events API |
| `POST /discord/interactions` | Discord Interactions API |

### Auth

- **HTTP API:** Bearer token (from config)
- **Slack:** HMAC-SHA256 signature verification via `X-Slack-Signature`
- **Discord:** Ed25519 signature verification

All use constant-time comparison.

### Session Tracking

Per-user sessions via `Arc<RwLock<HashMap<user_id, session_id>>>` for Slack, Discord, and Telegram.

### `run_telegram_polling()`

Long-polling loop against the Telegram Bot API. Per-chat session tracking.

### Design Notes

- All adapters share the same `DispatchFn` — the `WorkerBus` handles agent routing
- Slack and Discord dispatch asynchronously (return 200 OK immediately, send response later)
- CORS is permissive on the HTTP router
- Graceful shutdown via `broadcast::Receiver<()>`
