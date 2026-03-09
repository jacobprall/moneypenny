# mp-core — Core Library Specification

> **Crate:** `crates/mp-core/` | **Type:** Library | **Dependencies:** rusqlite, serde, anyhow, regex, reqwest, chrono, uuid, tracing

`mp-core` contains all business logic: the operation dispatcher, policy engine, search pipeline, context assembly, data stores, tool system, scheduler, sync, and ingestion. It has **no** dependency on `mp-ext` or `mp-llm` — those are injected by the binary crate.

## Module Map

```
mp-core/src/
├── lib.rs             # Module declarations (19 public modules)
├── operations.rs      # Canonical operation dispatcher
├── policy.rs          # ABAC policy engine
├── search.rs          # Hybrid retrieval (FTS5 + vector + RRF + MMR)
├── context.rs         # LLM context assembly with token budgeting
├── extraction.rs      # Fact extraction pipeline
├── agent.rs           # Sync agent turn loop (primarily for testing)
├── config.rs          # TOML configuration types
├── schema.rs          # DB schema definitions + migrations
├── db.rs              # SQLite connection helpers
├── gateway.rs         # Multi-agent routing + delegation
├── sync.rs            # CRDT sync wrapper
├── mcp.rs             # MCP stdio client
├── scheduler.rs       # Cron job engine
├── ingest.rs          # External event ingestion
├── channel.rs         # Channel trait + adapters
├── observability.rs   # Health + Prometheus metrics
├── encryption.rs      # DB-at-rest encryption
├── store/
│   ├── mod.rs         # Re-exports submodules
│   ├── facts.rs       # Fact CRUD, linking, audit, compaction
│   ├── log.rs         # Sessions, messages, tool calls
│   ├── knowledge.rs   # Documents, chunks, skills, edges
│   ├── scratch.rs     # Session-scoped ephemeral KV
│   └── redact.rs      # 18-pattern secret scanner
└── tools/
    ├── mod.rs         # Re-exports submodules
    ├── registry.rs    # Tool registration, discovery, execution pipeline
    ├── runtime.rs     # 19 self-awareness tools + JS bridge
    ├── builtins.rs    # OS tools (file, shell, http)
    └── hooks.rs       # Pre/post tool execution hooks
```

---

## operations.rs — Canonical Operation Dispatcher

**File:** `crates/mp-core/src/operations.rs`

The single entry point for all mutations and queries. Every user- or agent-initiated action flows through `execute()`.

### Public Types

| Type | Fields | Purpose |
|---|---|---|
| `ActorContext` | `agent_id`, `tenant_id?`, `user_id?`, `channel?` | Identifies the caller |
| `OperationContext` | `session_id?`, `trace_id?`, `timestamp?` | Ambient request metadata |
| `OperationRequest` | `op`, `op_version?`, `request_id?`, `idempotency_key?`, `actor`, `context`, `args` (JSON) | Full request envelope |
| `PolicyMeta` | `effect`, `policy_id?`, `reason?` | Policy decision summary |
| `AuditMeta` | `logged` (bool) | Whether audit was recorded |
| `OperationResponse` | `ok`, `code`, `message?`, `data` (JSON), `policy?`, `audit?` | Uniform response |

### Entry Point

```
pub fn execute(conn: &Connection, req: OperationRequest) -> OperationResponse
```

Pipeline:
1. **Idempotency check** — looks up `operation_idempotency` by `(actor_id, op, idempotency_key)`. Replays stored response or rejects conflicting fingerprints.
2. **Pre-hooks** — loads `operation_hooks` table (`phase="pre"`), runs pattern-matched checks (e.g., `deny_if_args_contains`, `max_args_bytes`, hard 2MB limit).
3. **Policy evaluation** — every operation, even reads.
4. **Dispatch** — routes `req.op` to the correct handler.
5. **Post-hooks** — redaction (`store::redact`), configurable transforms (`append_message_suffix`, `truncate_message`).
6. **Idempotency store** — persists response for future replay.
7. **Metadata stamp** — adds `_meta` (correlation_id, idempotency state) to response data.

### Supported Operations (~35)

| Category | Operations |
|---|---|
| Memory | `memory.search`, `memory.fact.add`, `memory.fact.update`, `memory.fact.get`, `memory.fact.compaction.reset` |
| Facts | `fact.delete` |
| Knowledge | `knowledge.ingest` |
| Skills | `skill.add`, `skill.promote` |
| Policies | `policy.add`, `policy.evaluate`, `policy.explain` |
| Policy Specs | `policy.spec.plan`, `policy.spec.confirm`, `policy.spec.apply` |
| Jobs | `job.create`, `job.list`, `job.run`, `job.pause`, `job.history` |
| Job Specs | `job.spec.plan`, `job.spec.confirm`, `job.spec.apply` |
| Audit | `audit.query`, `audit.append` |
| Sessions | `session.resolve`, `session.list` |
| JS Tools | `js.tool.add`, `js.tool.list`, `js.tool.delete` |
| Agents | `agent.create`, `agent.delete`, `agent.config` |
| Ingest | `ingest.events`, `ingest.status`, `ingest.replay` |

### Design Patterns

- **Plan/Confirm/Apply:** Both jobs and policies use a 3-phase state machine (`planned → confirmed → applied`) stored in `job_specs`/`policy_specs` tables, enabling human-in-the-loop approval.
- **Idempotency:** Mutations keyed by `(actor_id, op, idempotency_key)`. Fingerprint is `(op, agent_id, args)`. Replay returns stored response; conflict is rejected.
- **Policy-required guard:** Operations that need policy metadata verify it's present.

---

## policy.rs — Policy Engine

**File:** `crates/mp-core/src/policy.rs`

ABAC policy engine backed by SQLite. Evaluates actor/action/resource triples with support for behavioral rules.

### Public Types

| Type | Purpose |
|---|---|
| `Effect` | Enum: `Allow`, `Deny`, `Audit` |
| `PolicyMode` | Enum: `AllowByDefault`, `DenyByDefault` |
| `PolicyDecision` | Result: `effect`, `policy_id?`, `reason?` |
| `PolicyRequest<'a>` | Input: `actor`, `action`, `resource`, `sql_content?`, `channel?`, `arguments?` |
| `PolicyAuditContext<'a>` | Extra metadata: `session_id`, `correlation_id`, `idempotency_key/state` |

### Public Functions

| Function | Purpose |
|---|---|
| `evaluate(conn, req)` | Evaluate with `AllowByDefault` mode |
| `evaluate_with_mode(conn, req, mode)` | Evaluate with explicit mode |
| `evaluate_with_audit(conn, req, audit)` | Evaluate with audit context metadata |
| `generate_sql_filter(conn, agent_id)` | Generate WHERE clause for data-level fact scoping |

### Evaluation Flow

1. Load all enabled policies ordered by `priority DESC`.
2. For each policy: match `actor_pattern`, `action_pattern`, `resource_pattern`, `sql_pattern`, `argument_pattern`, `channel_pattern` using glob matching (`*` wildcards).
3. If the policy has a `rule_type`, check the behavioral condition:
   - **`rate_limit`** — count `tool_calls` in time window; trigger if >= max
   - **`retry_loop`** — group `tool_calls` by (tool, args); trigger if any group >= threshold
   - **`token_budget`** — estimate session tokens (chars/4); trigger if >= budget
   - **`time_window`** — check UTC hour and weekday
   - If condition NOT triggered, rule is skipped (falls through).
4. First matching rule wins. No match → PolicyMode fallback.
5. Every decision logged to `policy_audit`.

---

## search.rs — Hybrid Retrieval Engine

**File:** `crates/mp-core/src/search.rs`

Unified search across all data stores using hybrid FTS5 + vector KNN, fused via RRF.

### Public Types

| Type | Purpose |
|---|---|
| `Store` | Enum: `Facts`, `Log`, `Knowledge` — source tag |
| `SearchResult` | `id`, `store`, `content`, `score`, `sources` |
| `StoreWeights` | `facts` (0.4), `log` (0.2), `knowledge` (0.4) |

### Public Functions

| Function | Purpose |
|---|---|
| `search(conn, query, agent_id, limit, weights, query_embedding?)` | Main entry point — full hybrid pipeline |
| `detect_intent(query)` | Keyword-based intent → StoreWeights |
| `fts5_search_facts(conn, query, agent_id, limit)` | FTS5 on facts (LIKE fallback) |
| `fts5_search_messages(conn, query, agent_id, limit)` | LIKE on messages |
| `fts5_search_tool_calls(conn, query, agent_id, limit)` | LIKE on tool call logs |
| `fts5_search_policy_audit(conn, query, agent_id, limit)` | LIKE on policy audit |
| `fts5_search_knowledge(conn, query, limit)` | LIKE on chunks |
| `vector_search_facts(conn, blob, agent_id, limit)` | KNN on `facts.content_embedding` |
| `vector_search_messages(conn, blob, agent_id, limit)` | KNN on `messages.content_embedding` |
| `vector_search_tool_calls(conn, blob, agent_id, limit)` | KNN on `tool_calls.content_embedding` |
| `vector_search_policy_audit(conn, blob, agent_id, limit)` | KNN on `policy_audit.content_embedding` |
| `vector_search_knowledge(conn, blob, limit)` | KNN on `chunks.content_embedding` |
| `rrf_fuse(ranked_lists)` | Reciprocal Rank Fusion (K=60) |
| `mmr_rerank(results, k, lambda)` | MMR diversity re-ranking (lambda=0.7) |
| `jaccard_similarity(a, b)` | Token-level Jaccard (used by extraction too) |

### Pipeline

1. FTS5/LIKE text search across 5 stores
2. Vector KNN across 5 stores (if embedding provided)
3. RRF fusion across up to 10 ranked lists
4. Content resolution for vector-only hits
5. Store weight application
6. MMR re-ranking for diversity
7. Return top-K with source attribution

Graceful degradation: FTS5 falls back to LIKE; vector search silently returns empty if indexes absent.

---

## context.rs — LLM Context Assembly

**File:** `crates/mp-core/src/context.rs`

Builds the full context window for an LLM call with token budgeting across 9 segments.

### Public Types

| Type | Purpose |
|---|---|
| `ContextSegment` | `label`, `content`, `token_estimate` |
| `BudgetSplit` | Percentage allocation: `facts_expanded_pct` (20%), `scratch_pct` (10%), `log_pct` (30%), `knowledge_pct` (40%) |
| `TokenBudget` | Total budget with reserved slots (system=500, policies=200, pointers=2000, current_msg=500, headroom=2000) |
| `RebalanceContext` | `scratch_is_empty`, `session_is_new`, `session_message_count` |
| `FlexibleAllocation` | Computed token counts: `facts_expanded`, `scratch`, `log`, `knowledge` |

### Segment Assembly Order

1. **system_prompt** — persona or default
2. **policies** — active deny rules (top 10) injected so LLM knows constraints
3. **session_summary** — rolling summary bridging prior turns
4. **fact_pointers** — ALL pointers for the agent (compacted, ~2K tokens for 500 facts)
5. **facts_expanded** — relevance-matched full fact content
6. **scratch** — session working memory entries
7. **log** — last 20 messages
8. **knowledge** — relevance-matched document chunks
9. **current_message** — user input (always last)

### Rebalancing

- Empty scratch: budget → log + knowledge
- New session (0 messages): log budget → knowledge
- Deep session (>100 messages): 10% of knowledge → log

Token estimation: `ceil(chars / 4)`.

---

## extraction.rs — Fact Extraction Pipeline

**File:** `crates/mp-core/src/extraction.rs`

Extracts structured facts from conversation turns.

### Public Types

| Type | Purpose |
|---|---|
| `CandidateFact` | `content`, `summary`, `pointer`, `keywords?`, `confidence` |
| `DeduplicationDecision` | Enum: `ADD`, `UPDATE`, `DELETE`, `NOOP` |
| `ExtractionOutcome` | Candidate + decision + fact_id + policy_allowed + denial reason |

### Public Functions

| Function | Purpose |
|---|---|
| `assemble_extraction_context(conn, agent_id, session_id, new_messages, top_k)` | Build prompt context from session summary + messages + top-K existing facts |
| `parse_candidates(json_text)` | Deserialize JSON array of CandidateFacts from LLM output |
| `find_similar_fact(conn, agent_id, candidate)` | Jaccard word-overlap similarity (threshold > 0.5). Placeholder for vector search. |
| `process_candidate(conn, agent_id, candidate, decision, existing_fact_id, source_message_id)` | Apply one candidate: policy check → fact add/update/delete/bump |
| `run_pipeline(conn, agent_id, session_id, candidates, source_message_id)` | Full pipeline: atomic transaction over all candidates |

Pipeline per candidate: find similar → pick decision (>0.8 → Update, >0.5 → Noop, else → Add) → process → link related facts (Jaccard > 0.3).

---

## store/facts.rs — Fact Store

**File:** `crates/mp-core/src/store/facts.rs`

### Public Types

| Type | Key Fields |
|---|---|
| `Fact` | `id`, `agent_id`, `content`, `summary`, `pointer`, 3 embedding blobs, `keywords`, `confidence`, `version`, `superseded_at`, `context_compact`, `compaction_level` |
| `FactLink` | `source_id`, `target_id`, `relation`, `strength` |
| `FactAuditEntry` | `fact_id`, `operation`, `old_content`, `new_content`, `reason` |
| `NewFact` | Input DTO for `add()` |

### Public Functions

| Function | Purpose |
|---|---|
| `add(conn, fact, reason)` | Insert fact + audit entry. Returns UUID. |
| `update(conn, fact_id, content, summary, pointer, reason, source_msg_id)` | Update content, bump version, log audit. |
| `delete(conn, fact_id, reason)` | Soft-delete (set `superseded_at`). |
| `get(conn, fact_id)` | Fetch by ID. |
| `list_active(conn, agent_id)` | All non-superseded facts, newest first. |
| `all_pointers(conn, agent_id)` | (id, pointer, context_compact, compaction_level) tuples. |
| `compact_for_context(conn, agent_id)` | Progressive compaction: halve word count each call, 5-word floor. |
| `reset_compaction(conn, fact_id)` | Clear compaction metadata. |
| `link(conn, src, tgt, relation, strength)` | INSERT OR REPLACE fact link. |
| `get_links(conn, fact_id)` | All links where fact is source or target. |
| `get_audit(conn, fact_id)` | Chronological audit trail. |
| `bump_confidence(conn, fact_id, amount)` | Increment confidence, cap at 10.0. |
| `set_content_embedding(conn, fact_id, blob)` | Store FLOAT32 embedding. |
| `ids_without_embedding(conn, agent_id)` | IDs of active facts missing embeddings. |

---

## store/log.rs — Session & Message Log

**File:** `crates/mp-core/src/store/log.rs`

### Public Types

| Type | Key Fields |
|---|---|
| `Session` | `id`, `agent_id`, `channel`, `started_at`, `ended_at`, `summary` |
| `Message` | `id`, `session_id`, `role`, `content`, `created_at` |
| `ToolCallRecord` | `id`, `message_id`, `session_id`, `tool_name`, `arguments`, `result`, `status`, `policy_decision`, `duration_ms` |

### Public Functions

| Function | Purpose |
|---|---|
| `create_session` | New session row |
| `end_session` | Set `ended_at` |
| `update_summary` | Update rolling session summary |
| `append_message` | Append message, return ID |
| `get_messages` / `get_recent_messages` | Fetch chronologically / last N |
| `record_tool_call` | Full tool call record |
| `get_tool_calls` | All calls for a session |
| `messages_without_embedding` / `set_message_embedding` | Embedding lifecycle |
| `tool_calls_without_embedding` / `set_tool_call_embedding` | Embedding lifecycle |
| `policy_audit_without_embedding` / `set_policy_audit_embedding` | Embedding lifecycle |

Embedding discovery functions compose structured text for non-message types (`[tool_call] tool=... args=... result=...`).

---

## store/knowledge.rs — Knowledge Base

**File:** `crates/mp-core/src/store/knowledge.rs`

### Public Types

| Type | Key Fields |
|---|---|
| `Document` | `id`, `path`, `title`, `content_hash`, `metadata` |
| `Chunk` | `id`, `document_id`, `content`, `summary`, `position` |
| `Skill` | `id`, `name`, `description`, `content`, `tool_id`, `usage_count`, `success_rate`, `promoted` |

### Public Functions

| Function | Purpose |
|---|---|
| `ingest(conn, path, title, content, metadata)` | Store doc + chunk it. Returns `(doc_id, chunk_count)`. |
| `get_document` / `list_documents` | Document retrieval |
| `get_chunks` | Chunks for a document |
| `chunk_markdown(content)` | Split markdown on headings, max 2000 chars |
| `add_edge` | INSERT OR IGNORE knowledge graph edge |
| `add_skill` / `get_skill` | Skill CRUD |
| `record_skill_usage` | Increment count, update running success rate |
| `promote_skill` | Set `promoted = 1` |
| `set_chunk_embedding` / `chunks_without_embedding` | Embedding lifecycle |

---

## store/scratch.rs — Session Scratch Pad

**File:** `crates/mp-core/src/store/scratch.rs`

Session-scoped ephemeral key-value store.

| Function | Purpose |
|---|---|
| `set(conn, session_id, key, content)` | Upsert by (session_id, key) |
| `get(conn, session_id, key)` | Fetch by session + key |
| `list(conn, session_id)` | All entries for session |
| `remove(conn, entry_id)` | Hard delete |
| `clear_session(conn, session_id)` | Delete all session entries |

---

## store/redact.rs — Secret Redaction

**File:** `crates/mp-core/src/store/redact.rs`

Always-on, non-configurable secret scanner. 18 compiled regex patterns.

| Function | Purpose |
|---|---|
| `redact(text)` | Replace all matched secrets with `[REDACTED]` |
| `contains_secrets(text)` | Boolean check |

Patterns: OpenAI keys, AWS keys, GCP keys, PEM blocks, Azure connection strings, Stripe keys, JWTs, Bearer tokens, DB URIs, GitHub PATs, Anthropic keys, generic password/secret/token assignments.

---

## tools/registry.rs — Tool Registry & Execution

**File:** `crates/mp-core/src/tools/registry.rs`

Central tool management. Persists tool definitions to the `skills` table.

### Public Types

| Type | Purpose |
|---|---|
| `ToolSource` | Enum: `Builtin`, `Mcp`, `SqliteJs`, `Runtime` |
| `ToolDef` | `name`, `description`, `source`, `parameters_schema`, `enabled` |
| `ToolResult` | `output`, `success`, `duration_ms` |

### Key Functions

| Function | Purpose |
|---|---|
| `register(conn, tool)` | INSERT OR REPLACE into `skills` |
| `lookup(conn, name)` | Find by name |
| `list_tools(conn)` | All registered tools |
| `discover(conn, intent, limit)` | Fuzzy search by name/description |
| `execute(conn, agent_id, session_id, msg_id, name, args, executor, hooks)` | Full pipeline: policy → pre-hooks → dispatch → post-hooks → redact → audit |
| `register_builtins(conn)` | Seed 5 built-in tools |
| `register_runtime_skills(conn)` | Seed 19 runtime tools |

Execution pipeline: policy check → pre-hooks (can abort/override args) → dispatch (runtime → MCP → JS → builtin fallback) → post-hooks (can transform output) → redact → record_tool_call.

---

## tools/runtime.rs — Runtime Tools & JS Bridge

**File:** `crates/mp-core/src/tools/runtime.rs`

19 self-awareness tools that give the agent access to its own memory, knowledge, scheduling, governance, and custom JS.

| Tool | Category | What it does |
|---|---|---|
| `web_search` | Web | DuckDuckGo instant answer API |
| `memory_search` | Memory | Hybrid search with optional vector embedding |
| `fact_add` / `fact_update` / `fact_list` | Memory | Fact CRUD |
| `scratch_set` / `scratch_get` | Scratch | Session working memory |
| `knowledge_ingest` / `knowledge_list` | Knowledge | Doc ingestion + listing |
| `job_create` / `job_list` / `job_pause` / `job_resume` | Scheduler | Job lifecycle |
| `policy_list` / `policy_add` | Governance | Policy CRUD (add creates draft spec) |
| `audit_query` | Governance | Query audit trail |
| `js_tool_add` / `js_tool_list` / `js_tool_delete` | JS Tools | Custom tool management |

Key helper: `eval_js(conn, script)` — evaluates JavaScript via in-process QuickJS (`js_eval` SQL function).

---

## tools/builtins.rs — OS-Level Tools

**File:** `crates/mp-core/src/tools/builtins.rs`

5 primitive tools: `file_read`, `file_write`, `shell_exec`, `http_request` (stub), `sql_query` (stub).

`dispatch(tool_name, arguments) -> ToolResult` routes to the correct handler. Errors captured as `success: false` results.

---

## tools/hooks.rs — Pre/Post Hooks

**File:** `crates/mp-core/src/tools/hooks.rs`

| Type | Purpose |
|---|---|
| `HookContext` | `tool_name`, `agent_id`, `session_id` |
| `PreOutcome` | `Continue { args? }` or `Abort(reason)` |
| `PostOutcome` | `Keep` or `OverrideOutput(new_output)` |
| `ToolHooks` | Container with `add_pre()`, `add_post()` methods |

Pre-hooks chain (each sees modified args), first Abort wins. Post-hooks chain (each sees modified output). Glob pattern matching.

---

## gateway.rs — Multi-Agent Routing

**File:** `crates/mp-core/src/gateway.rs`

### Public Types

| Type | Purpose |
|---|---|
| `AgentEntry` | Agent record: `id`, `name`, `persona`, `trust_level`, `db_path`, `status` |
| `AgentStatus` | Enum: `Running`, `Stopped`, `Error` |
| `RoutedMessage` | `source_agent`, `target_agent`, `channel`, `content`, `delegation_depth` |
| `FactScope` | Enum: `Private`, `Shared`, `Protected` |

### Key Functions

| Function | Purpose |
|---|---|
| `list_agents(meta_conn)` | All agents from metadata DB |
| `get_agent(meta_conn, name)` | Lookup by name |
| `route_message(meta_conn, msg, handler)` | Resolve target → policy check → invoke handler |
| `delegate(meta_conn, source, target, msg, depth, handler)` | Delegation with max depth 3 |
| `can_access_fact(trust, scope, fact_agent, requester)` | Access control: Private=owner, Shared=all, Protected=owner+elevated |

---

## sync.rs — CRDT Sync

**File:** `crates/mp-core/src/sync.rs`

Wraps `sqlite-sync` extension for CRDT-based replication.

**Default sync tables:** `facts`, `fact_links`, `skills`, `policies`.

| Function | Purpose |
|---|---|
| `init_sync_tables(conn, tables)` | Idempotent CRDT tracking enable |
| `status(conn, tables)` | Current sync state (site_id, db_version) |
| `local_sync_bidirectional(a, b, tables)` | Two-way payload exchange |
| `local_sync_push` / `local_sync_pull` | One-way sync |
| `cloud_sync(conn, url)` | Bidirectional cloud sync |

---

## Other Modules (Brief)

### config.rs
TOML configuration types: `Config`, `AgentConfig`, `LlmConfig`, `EmbeddingConfig`, `ChannelsConfig`, `SyncConfig`, `McpServerConfig`. Load from file, serialize to TOML.

### schema.rs
Agent DB schema (v1–v11) with incremental migrations. Metadata DB schema (v1). Helper: `init_agent_db()`, `init_metadata_db()`, `init_vector_indexes()`, `init_sync_tables()`.

### db.rs
`open(path)` and `open_memory()` — configures WAL mode, NORMAL sync, foreign keys, 5s busy timeout.

### agent.rs
Sync agent turn loop for testing: message → context → policy → LLM → tool parse → execute → redact → store. Uses `[TOOL:name](args)` inline format. `AgentLlm` trait for mock injection.

### mcp.rs
MCP stdio client. `McpClient::connect()` spawns server, `list_tools()` discovers tools, `call_tool()` invokes them. `discover_and_register()` connects to all configured servers and seeds `skills` table. Each dispatch spawns a fresh server process.

### scheduler.rs
Cron job engine. Types: `Job`, `JobRun`, `NewJob`, `JobType` (Prompt/Tool/Js/Pipeline), `OverlapPolicy` (Skip/Queue/Allow). Functions: CRUD, `poll_due_jobs()`, `dispatch_job()` (overlap check → policy → run → retry).

### ingest.rs
JSONL event ingestion from external sources. Dedup by content hash. Projects events into native tables (messages, tool_calls, policy_audit). Incremental (tracks line offsets). Supports replay and dry-run.

### channel.rs
`Channel` trait with `name()`, `capabilities()`, `send()`. Capability model: threads, reactions, files, streaming, max_message_length. Concrete: `CliChannel`, `HttpApiChannel`, `SlackChannel`, `DiscordChannel`.

### observability.rs
`agent_health()` — fact/session counts. `jobs_health()` — active/failed counts. `Metrics` — atomic counters with `render_prometheus()`.

### encryption.rs
`KeySource` enum (Keychain/KeyFile/CredentialManager/Passphrase/None). `get_key()`, `store_key()`, `generate_key()`, `apply_encryption()`. Platform stubs.
