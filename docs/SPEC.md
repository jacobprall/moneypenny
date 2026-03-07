# Moneypenny — Technical Specification

**Version:** 0.1 (draft)
**Status:** Design phase

---

## 1. Thesis

Moneypenny is a database-native autonomous AI agent platform. The database is the runtime — not just the storage. Inference, memory, search, sync, policy, and tool execution all happen inside the same transactional boundary.

The architectural bet: every other agent framework is orchestration-first with persistence bolted on. Moneypenny inverts this. The data layer is the product. The orchestrator is a thin loop on top.

---

## 2. Decisions

### 2.1 Language and runtime

**Decision:** Rust.

**Reasoning:** Single binary distribution. Native SQLite extension loading without FFI wrappers. Memory safety without GC pauses. Aligns with the "rock-solid" design goal. The trade-off is slower iteration speed vs TypeScript/Python, but the core loop is small — most complexity lives in the SQLite extensions (C) which already exist.

### 2.2 Agent loop model

**Decision:** Dual mode — `sqlite-agent` for local/offline, HTTP orchestrator for cloud LLMs.

**Reasoning:** Option B (sqlite-agent runs the loop inside SQLite) is the radical, differentiating choice — it makes "everything is a transaction" literally true. Option A (orchestrator calls external LLMs) is necessary for cloud model support (Claude, GPT-4, Gemini). Both modes share the same state layer, policy engine, and memory stores. The agent doesn't know or care which mode it's in.

- **Local mode:** `sqlite-ai` loads a GGUF model. `sqlite-agent` runs the full agent loop (goal → tool selection → MCP execution → result → iterate) inside SQLite. The entire operation is a single transaction.
- **Cloud mode:** The Rust orchestrator calls an external LLM (Anthropic by default) via HTTP, parses tool calls, executes them against the SQLite extensions, feeds results back. State management is still transactional — the orchestrator wraps each turn in a SQLite transaction.

### 2.3 LLM provider interface

**Decision:** Pluggable traits. Generation and embedding are separate concerns with independent providers.

**Generation** (cloud by default):

```
trait LlmProvider {
    fn generate(&self, messages, tools, config) -> Result<Response>;
    fn supports_streaming(&self) -> bool;
}
```

Implementations:
- `AnthropicProvider` — native Anthropic Messages API. Default cloud provider.
- `HttpProvider` — OpenAI-compatible API. Works with OpenAI, Ollama, vLLM, any OpenAI-compatible endpoint.
- `SqliteAiProvider` — on-device GGUF via `sqlite-ai`. No network.

**Embedding** (local by default):

```
trait EmbeddingProvider {
    fn embed(&self, text) -> Result<Vec<f32>>;
    fn embed_batch(&self, texts) -> Result<Vec<Vec<f32>>>;
    fn dimensions(&self) -> usize;
}
```

Implementations:
- `LocalEmbeddingProvider` — on-device GGUF via `sqlite-ai`. Ships with `nomic-embed-text-v1.5` (768-dim, ~274MB). Default. No network, no API keys. Embeddings never leave the machine.
- `HttpEmbeddingProvider` — OpenAI-compatible `/embeddings` API. Opt-in for users who want cloud embeddings.

Generation and embedding providers are configured independently per-agent. The default setup uses Anthropic for generation and local GGUF for embeddings — cloud intelligence with local privacy for vector search.

### 2.4 Process model

**Decision:** Gateway + workers. One SQLite database per agent.

**Reasoning:** SQLite in WAL mode supports concurrent readers but serializes writers. Multiple agents writing to one database creates contention. One database per agent eliminates this — each worker has exclusive write access to its own file. The gateway routes messages to the correct worker.

```
┌──────────────────────────────────────────────────────┐
│  Gateway                                              │
│  Message routing · Channel management · Lifecycle     │
│  Heartbeat scheduler · Agent registry                 │
│  metadata.db (low-write, agent configs + routing)     │
├──────────┬──────────┬──────────┬─────────────────────┤
│ Worker A │ Worker B │ Worker C │ ...                  │
│ agent_a  │ agent_b  │ agent_c  │                      │
│  .db     │  .db     │  .db     │                      │
└────┬─────┴────┬─────┴────┬─────┘                      │
     │          │          │                             │
     └──────────┼──────────┘                             │
                │  sqlite-sync (CRDTs)                   │
                │  shared knowledge propagation           │
                ▼                                        │
         SQLite Cloud (optional)                         │
```

- Gateway owns a small `metadata.db` for agent registry, channel configs, routing rules. Low-write.
- Each agent worker owns its `.db` file exclusively. Full ACID.
- `sqlite-sync` propagates shared knowledge between agent databases asynchronously via CRDTs.
- Each agent `.db` file is portable — back it up, move it, inspect it independently.

### 2.5 Multi-agent

**Decision:** First-class. Each agent is an independent identity with its own database, policies, and persona. Multiple agents run within one gateway.

**Reasoning:** Necessary differentiator. Use cases: a "code review" agent and a "data analysis" agent on the same machine. A team where each member's agent has different permissions. A fleet of agents across an org, sharing knowledge via sync.

Agents are defined in the gateway's `metadata.db`:

```sql
agents (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    persona TEXT,               -- system prompt / identity
    trust_level TEXT DEFAULT 'standard',
    llm_provider TEXT DEFAULT 'local',
    llm_model TEXT,
    db_path TEXT NOT NULL,      -- path to this agent's .db file
    sync_enabled INTEGER DEFAULT 1,
    created_at INTEGER
)
```

### 2.6 Channel abstraction

**Decision:** Minimal interface. Channels are thin bidirectional message adapters.

```
trait Channel {
    fn receive(&self) -> Stream<IncomingMessage>;
    fn send(&self, response: OutgoingMessage) -> Result<()>;
    fn capabilities(&self) -> ChannelCapabilities;
}

struct ChannelCapabilities {
    supports_threads: bool,
    supports_reactions: bool,
    supports_files: bool,
    supports_streaming: bool,
    max_message_length: Option<usize>,
}
```

Channel-specific features (threads, reactions, file uploads) are progressive enhancements discoverable via `capabilities()`. The agent loop doesn't depend on any channel-specific behavior.

Initial channels: CLI (v0.1), HTTP API (v0.1), Slack (v0.2), Discord (v0.2).

### 2.7 Configuration model

**Decision:** File bootstrap + SQL runtime.

- A small TOML file (`moneypenny.toml`) bootstraps the system: database paths, which channels to enable, initial agent definitions.
- Everything else lives in SQLite tables: policies, schedules, memory settings, channel configs. Queryable, auditable, synced via CRDTs.
- Runtime config changes don't require restarts — the gateway watches the config tables.

### 2.8 Distribution

**Decision:** Single binary.

`mp` is one executable. No runtime dependencies. GGUF models are downloaded on first use (or pre-bundled). SQLite extensions are statically linked.

```bash
curl -sSL https://moneypenny.dev/install.sh | sh
mp init
mp start
```

### 2.9 Developer experience

**Decision:** Vercel-level onboarding. Fastest path to wow.

The "hello world" flow:

```bash
mp init                          # creates moneypenny.toml + data dir
mp start                         # starts gateway + default agent + CLI channel
> hello                          # first message — agent responds
> remember that I prefer Rust    # fact extraction happens automatically
> what language do I prefer?     # agent recalls from Facts
```

Three commands to a working agent with persistent memory. Embeddings run locally out of the box (nomic-embed-text-v1.5, no API key needed). Cloud generation requires an Anthropic API key — or switch to local GGUF for fully offline operation. No config files to edit. No Docker. No database setup.

---

## 3. Memory Architecture

### 3.1 Design principles

- No anthropomorphic labels. Memory types are named for what the data *is*, not what cognitive function it maps to.
- Automatic capture over explicit saves. The system remembers so the agent doesn't have to.
- Progressive compression. Broad coverage at low token cost, depth on demand.
- Everything is governed. The policy engine controls who can read, write, and modify memory.
- Everything is synced. Knowledge propagates across agents via CRDTs.

### 3.2 Four stores, one database

```
┌───────────────────────────────────────────────────────────┐
│  Agent SQLite Database                                     │
│                                                            │
│  ┌────────────┐ ┌──────────┐ ┌────────────┐ ┌──────────┐  │
│  │  Facts     │ │  Log     │ │ Knowledge  │ │ Scratch  │  │
│  │            │ │          │ │            │ │          │  │
│  │ What is    │ │ What     │ │ What was   │ │ What's   │  │
│  │ true.      │ │ happened.│ │ provided.  │ │ in       │  │
│  │ Curated.   │ │ Immutable│ │ Ingested.  │ │ progress.│  │
│  │ Compressed.│ │ Complete.│ │ Chunked.   │ │ Session- │  │
│  │ Linked.    │ │ Append-  │ │ Embedded.  │ │ scoped.  │  │
│  │            │ │ only.    │ │ Graph-     │ │ Promoted │  │
│  │            │ │          │ │ linked.    │ │ or       │  │
│  │            │ │          │ │            │ │ discarded│  │
│  └────────────┘ └──────────┘ └────────────┘ └──────────┘  │
│                                                            │
│  ┌────────────────────────────────────────────────────────┐│
│  │ Extraction Pipeline (async, after each turn)           ││
│  │ LLM: ADD / UPDATE / DELETE / NOOP                      ││
│  └────────────────────────────────────────────────────────┘│
│                                                            │
│  ┌────────────────────────────────────────────────────────┐│
│  │ Hybrid Search (FTS5 + vector + RRF)                    ││
│  │ Queries all stores · Policy-filtered at the SQL level  ││
│  └────────────────────────────────────────────────────────┘│
│                                                            │
│  ┌────────────────────────────────────────────────────────┐│
│  │ Policy Engine (configurable default, behavioral rules)  ││
│  │ Governs: tools, memory access, fact writes, channels   ││
│  └────────────────────────────────────────────────────────┘│
└───────────────────────────────────────────────────────────┘
```

### 3.3 Facts

**What it holds:** Distilled, curated knowledge — user preferences, learned patterns, project conventions, entity relationships. Small, high-value entries. Auto-managed.

**How it grows:** After each conversation turn, the extraction pipeline runs asynchronously:

1. Assemble extraction context: new messages + rolling conversation summary + top-K existing facts (by relevance to new messages).
2. LLM extraction call produces candidate facts as structured JSON.
3. Each candidate is compared against existing facts via vector similarity.
4. LLM decides: **ADD** (new fact), **UPDATE** (refine existing), **DELETE** (contradiction), **NOOP** (already known).
5. All changes applied in a single transaction with full audit trail.

Inspired by Mem0's two-phase extraction pipeline: proven to achieve 26% higher accuracy than OpenAI's memory with 90% token savings.

**Progressive compression:**

Every fact is stored at three resolution levels, generated at extraction time:

```
Level 0 (full):     "The ORDERS table in production uses soft deletes via a
                     deleted_at timestamp column. Any query must include
                     WHERE deleted_at IS NULL for accurate results."
                     [~40 tokens]

Level 1 (summary):  "ORDERS uses soft deletes; filter WHERE deleted_at IS NULL"
                     [~12 tokens]

Level 2 (pointer):  "ORDERS: soft-delete filter"
                     [~4 tokens]
```

**Context loading strategy:**

1. ALL Level 2 pointers are loaded into every prompt. At ~4 tokens each, 500 facts cost ~2K tokens. The agent sees everything it knows at a glance.
2. The system auto-expands relevant pointers to Level 1 via vector similarity against the current message (precomputed embeddings on all levels).
3. If the agent needs full detail, it requests Level 0 expansion via tool call.

**Capacity:** Thousands of facts fit as pointers. No ceiling on storage — only the pointer list needs to fit in context. When the agent "revisits" a compressed fact, it pulls the full Level 0 content, and the fact re-enters the extraction pipeline as if it were new input. This means revisiting old facts can trigger UPDATE operations — the fact evolves on each interaction.

**Temporal decay + confidence:**

- Every fact carries `created_at`, `updated_at`, `confidence`, and optionally `superseded_at`.
- Confidence increases when the same fact is re-extracted from independent conversations (validation signal).
- Retrieval scoring factors in recency and confidence, configurable per-agent.
- Half-life is configurable: 30 days for a personal assistant, infinite for architectural decisions.

**Fact linking (A-Mem inspired):**

Facts link to related facts via a lightweight edge table. When a new fact is added, the system finds semantically similar existing facts and establishes links. Linked facts can be traversed: "show me everything related to ORDERS" follows the graph.

**Schema:**

```sql
facts (
    id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL,
    content TEXT NOT NULL,              -- Level 0: full
    summary TEXT NOT NULL,              -- Level 1: compressed
    pointer TEXT NOT NULL,              -- Level 2: 2-5 word label
    content_embedding BLOB,
    summary_embedding BLOB,
    pointer_embedding BLOB,
    keywords TEXT,                      -- for FTS5
    source_message_id TEXT,             -- provenance
    confidence REAL DEFAULT 1.0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    superseded_at INTEGER,              -- null if current
    version INTEGER DEFAULT 1
)

fact_links (
    source_id TEXT NOT NULL,
    target_id TEXT NOT NULL,
    relation TEXT,                      -- 'relates_to', 'supersedes', 'contradicts'
    strength REAL DEFAULT 1.0,
    PRIMARY KEY (source_id, target_id)
)

fact_audit (
    id TEXT PRIMARY KEY,
    fact_id TEXT NOT NULL,
    operation TEXT NOT NULL,            -- 'add', 'update', 'delete'
    old_content TEXT,
    new_content TEXT,
    reason TEXT,                        -- LLM's explanation
    source_message_id TEXT,
    created_at INTEGER NOT NULL
)
```

### 3.4 Log

**What it holds:** Everything that happened. Every message, tool call, tool result, policy decision. Append-only, immutable, complete.

**How it grows:** Automatically on every interaction. No extraction, no filtering — raw capture with secret redaction applied before write.

**How it's used:**
- Searched on demand via hybrid search when the agent needs context from past conversations.
- Source material for the extraction pipeline — facts are *derived from* the log.
- Audit trail for governance and debugging.

**Schema:**

```sql
sessions (
    id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL,
    channel TEXT,
    started_at INTEGER NOT NULL,
    ended_at INTEGER,
    summary TEXT,                       -- rolling summary, updated periodically
    summary_embedding BLOB
)

messages (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    role TEXT NOT NULL,                 -- 'user', 'assistant', 'system', 'tool'
    content TEXT NOT NULL,
    content_embedding BLOB,
    created_at INTEGER NOT NULL
)

tool_calls (
    id TEXT PRIMARY KEY,
    message_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    tool_name TEXT NOT NULL,
    arguments TEXT,                     -- JSON
    result TEXT,
    status TEXT,                        -- 'success', 'error', 'denied'
    policy_decision TEXT,               -- 'allowed', 'denied', 'audited'
    duration_ms INTEGER,
    created_at INTEGER NOT NULL
)
```

### 3.5 Knowledge

**What it holds:** Explicitly provided reference material — documents, code, runbooks, skills. Chunked, embedded, graph-linked. Not derived from conversations.

**How it grows:** Via explicit ingestion — CLI command, API call, or agent tool. Content is parsed, chunked (markdown-aware, ~2000 char max), embedded, and indexed.

**How it's used:** RAG retrieval at query time. Progressive disclosure: discovery-tier summaries (~100 tokens) first for broad scanning, activation-tier full chunks on demand.

**Skills:**

Skills are a specialized form of Knowledge — a document describing a capability, optionally linked to a sqlite-js tool function.

```sql
skills (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT NOT NULL,
    content TEXT NOT NULL,              -- full skill document
    tool_id TEXT,                       -- optional: linked sqlite-js function
    content_embedding BLOB,
    usage_count INTEGER DEFAULT 0,
    success_rate REAL,
    promoted INTEGER DEFAULT 0,         -- proven patterns get promoted
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
)
```

- Skills are discovered at query time via RAG — the agent doesn't need a static tool list.
- Usage metrics track how often a skill is invoked and whether it succeeds.
- Skills with high usage and success rates are automatically promoted (surfaced more readily).
- Skills sync across agents via `sqlite-sync` — Agent A learns a skill, it propagates to Agent B.

**Schema for documents/chunks:**

```sql
documents (
    id TEXT PRIMARY KEY,
    path TEXT,
    title TEXT,
    content_hash TEXT NOT NULL,        -- for change detection
    metadata TEXT,                     -- JSON
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
)

chunks (
    id TEXT PRIMARY KEY,
    document_id TEXT NOT NULL,
    content TEXT NOT NULL,
    summary TEXT,                      -- discovery-tier summary
    content_embedding BLOB,
    summary_embedding BLOB,
    position INTEGER,                  -- order within document
    created_at INTEGER NOT NULL
)

edges (
    source_id TEXT NOT NULL,
    target_id TEXT NOT NULL,
    relation TEXT NOT NULL,            -- 'routes_to', 'depends_on', 'references'
    PRIMARY KEY (source_id, target_id, relation)
)
```

### 3.6 Scratch

**What it holds:** Active task state within a session — intermediate findings, plans, accumulated decisions. Working memory.

**How it grows:** Written by the agent during multi-step tasks. The agent can store intermediate results, partial plans, and accumulated context.

**Lifecycle:** Session-scoped. At session end (or periodically during long sessions), the extraction pipeline runs over Scratch contents and promotes durable findings to Facts. Everything else is discarded.

**Why it's separate:** Scratch is ephemeral and high-churn. It shouldn't pollute Facts with intermediate noise. It also doesn't need to sync — it's local to one session on one agent.

```sql
scratch (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    key TEXT NOT NULL,                 -- agent-defined label
    content TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
)
```

### 3.7 Context assembly

When a message arrives, the system assembles the LLM prompt:

```
Token budget: N (model-dependent)

1. System prompt + agent persona                    [fixed]
2. Active policies (relevant deny rules)            [small, fixed]
3. Fact pointers (ALL Level 2)                      [~2K tokens]
4. Auto-expanded facts (Level 1, by relevance)      [~500 tokens]
5. Scratch (current session's working state)         [variable]
6. Retrieved from Log                               [budget-allocated]
   - Recent messages from current session
   - Semantically relevant past messages (hybrid search)
7. Retrieved from Knowledge                         [budget-allocated]
   - Hybrid search results for current query
   - Skill descriptions if task-relevant
8. Current message                                  [fixed]

Steps 6 and 7 share the remaining budget. The split is tunable:
more Log for conversational tasks, more Knowledge for reference tasks.
```

### 3.8 Rolling conversation summary

Conversations grow beyond any context window. The summary strategy:

**Incremental summarization:** Every N turns (default: 5), the system generates a summary of the recent turns and appends it to the session's rolling summary. The rolling summary is what gets fed to the extraction pipeline alongside new messages.

**Progressive compression on summaries:** As the session grows, older summary segments are compressed to lower resolution. Recent segments stay at full resolution. Same principle as fact compression — broad coverage of history at low token cost, detail on recent turns.

**Summary persistence:** The rolling summary is stored on the `sessions` table and updated in-place. It's available for cross-session search via hybrid search.

### 3.9 Extraction pipeline

Runs asynchronously after each conversation turn. Does not block the agent's response.

```
Message arrives
    │
    ├──► Agent responds immediately (fast path)
    │
    └──► Extraction pipeline (background, async)
              │
              ├─ 1. Assemble extraction context:
              │     - New messages from this turn
              │     - Rolling conversation summary
              │     - Top-K existing facts by relevance
              │
              ├─ 2. LLM extraction call:
              │     Input: extraction context
              │     Output: [{content, summary, pointer, keywords, confidence}]
              │
              ├─ 3. For each candidate fact:
              │     - Vector search against existing facts
              │     - If similar found (cosine > threshold):
              │         LLM decides: UPDATE, DELETE, or NOOP
              │     - If no similar found: ADD
              │
              ├─ 4. Link new/updated facts:
              │     - Find related facts via embedding similarity
              │     - Establish edges in fact_links
              │
              ├─ 5. Policy check:
              │     - Each candidate passes through policy engine
              │     - PII check, scope check, content policy
              │     - Denied candidates are logged but not stored
              │
              └─ 6. Commit transactionally:
                    BEGIN;
                      INSERT/UPDATE/DELETE facts
                      INSERT fact_audit entries
                      UPDATE embeddings
                    COMMIT;
```

**Extraction model:** The extraction LLM can be a different (smaller, cheaper) model than the conversational LLM. A 3B parameter local model is sufficient for fact extraction, even if the conversational model is Claude Sonnet. This keeps extraction fast and cheap.

**Revisiting compressed facts:** When the agent expands a Level 2 pointer to Level 0, the full fact content re-enters the extraction pipeline context. This means revisiting old facts can trigger UPDATEs — the fact evolves, its confidence changes, its links update. Facts are living knowledge, not static records.

---

## 4. Policy Engine

### 4.1 Design principles

- Configurable default. The fallthrough when no rule matches is configurable per-agent: `allow` for development, `deny` for production governance.
- Policies are data. Stored as SQL rows, synced via CRDTs, queryable.
- One model for everything. The same `allow(actor, action, resource)` predicate governs tools, memory, facts, channels, SQL execution.
- Behavioral awareness. Rules can evaluate patterns over time — rate limits, retry loops, token budgets, time windows — not just static properties.

### 4.2 Architecture

**Evaluation engine:** Custom Rust implementation. In-process, no external dependencies. Two-layer design: static pattern rules (Layer 1) and behavioral rules (Layer 2) that evaluate patterns over time.

**Two-layer interface:**

```
┌─────────────────────────────────────────────────┐
│  Layer 1: SQL rows (static rules)                │
│  INSERT INTO policies VALUES (...)               │
│  Glob patterns, regex, priority ordering.        │
│  Synced via CRDTs. Edited via CLI or API.        │
├─────────────────────────────────────────────────┤
│  Layer 2: Behavioral rules (dynamic state)       │
│  rule_type: rate_limit, retry_loop,              │
│             token_budget, time_window            │
│  rule_config: JSON parameters per type           │
│  Evaluates by querying tool_calls + audit tables │
└──────────────────────┬──────────────────────────┘
                       │
                       ▼
              ┌────────────────┐
              │  Rule Evaluator │  Pattern match + behavioral check
              │  (in-process)   │  Configurable default (allow/deny)
              │                │  Returns: allow/deny/audit + reason
              └───────┬────────┘
                      │
                      ▼
              ┌────────────────┐
              │  SQL Filter     │  For data queries:
              │  Generator      │  policy → WHERE clause
              └────────────────┘
```

**Future — Layer 3: Rule DSL.** A lightweight Polar-inspired domain-specific language for expressing complex conditional rules. Deferred until the SQL-based layers prove insufficient for real-world use cases.

### 4.3 The universal predicate

Everything reduces to: `allow(actor, action, resource)`

| Actor | Action | Resource | Example |
|---|---|---|---|
| Agent | call | Tool | Can agent-alpha call shell_exec? |
| Agent | execute | SQL query | Can this agent run DDL? |
| Agent | read | Fact scope | Can agent-beta see agent-alpha's facts? |
| Agent | write | Fact | Can this agent store this fact? (PII check) |
| Channel | trigger | Tool | Can Slack messages trigger file writes? |
| Extraction | add | Fact | Can this extracted fact be stored? |
| User | configure | Agent | Can this user modify agent-alpha's policies? |

### 4.4 Policy rules as SQL rows

```sql
policies (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    priority INTEGER NOT NULL DEFAULT 0,   -- higher = evaluated first
    phase TEXT NOT NULL DEFAULT 'pre',     -- 'pre', 'post', 'both'
    effect TEXT NOT NULL,                  -- 'deny', 'allow', 'audit'

    -- Matchers (all optional; all present matchers must match)
    actor_pattern TEXT,                    -- glob: 'agent:*', 'agent:alpha'
    action_pattern TEXT,                   -- glob: 'call', 'execute', '*'
    resource_pattern TEXT,                 -- glob: 'tool:shell_*', 'fact:*'

    -- Content matchers (for SQL and tool arguments)
    sql_pattern TEXT,                      -- regex applied to SQL content
    argument_pattern TEXT,                 -- JSON path expression on tool args

    -- Scope
    agent_id TEXT,                         -- null = applies to all agents
    channel_pattern TEXT,                  -- null = applies to all channels
    schedule TEXT,                         -- cron: only active during certain hours

    -- Response
    message TEXT,                          -- shown to agent on deny
    enabled INTEGER DEFAULT 1,
    created_at INTEGER NOT NULL
)
```

**Example rules:**

```sql
-- Block destructive SQL
INSERT INTO policies (id, name, priority, effect, action_pattern, resource_pattern, sql_pattern, message)
VALUES ('block-drop', 'Block DROP statements', 100, 'deny', 'execute', 'sql:*',
        'DROP|TRUNCATE', 'Destructive SQL operations are blocked by policy.');

-- Require WHERE on DELETE
INSERT INTO policies (id, name, priority, effect, action_pattern, sql_pattern, message)
VALUES ('require-where', 'Require WHERE on DELETE/UPDATE', 90, 'deny', 'execute',
        '(DELETE|UPDATE)\s+(?!.*WHERE)', 'DELETE and UPDATE require a WHERE clause.');

-- Audit all tool calls from public channels
INSERT INTO policies (id, name, priority, effect, action_pattern, channel_pattern)
VALUES ('audit-public', 'Audit public channel tools', 50, 'audit', 'call', 'slack:*-general');

-- No shell access for untrusted agents
INSERT INTO policies (id, name, priority, effect, actor_pattern, resource_pattern, message)
VALUES ('no-shell', 'No shell for untrusted', 100, 'deny', 'agent:untrusted-*',
        'tool:shell_*', 'Untrusted agents cannot execute shell commands.');
```

### 4.5 Behavioral rules

Behavioral rules extend static pattern matching with dynamic state evaluation. They use `rule_type` and `rule_config` columns on the policies table.

When a policy row matches on its static patterns (actor, action, resource) AND has a `rule_type`, the engine evaluates the behavioral condition by querying recent history from the `tool_calls` and `policy_audit` tables. If the behavioral condition is not triggered, the rule is skipped and evaluation continues to lower-priority rules.

**Rule types:**

**`rate_limit`** — Deny if too many matching actions in a time window.

```sql
INSERT INTO policies (id, name, priority, effect, action_pattern, resource_pattern,
                      rule_type, rule_config, message)
VALUES ('rate-limit-shell', 'Shell rate limit', 90, 'deny',
        'call', 'tool:shell_*',
        'rate_limit', '{"max": 10, "window_seconds": 300}',
        'Rate limit exceeded: max 10 shell commands per 5 minutes.');
```

**`retry_loop`** — Deny if the same tool is called with the same arguments repeatedly.

```sql
INSERT INTO policies (id, name, priority, effect,
                      rule_type, rule_config, message)
VALUES ('no-retry-loops', 'Detect retry loops', 85, 'deny',
        'retry_loop', '{"same_tool_same_args": 3, "window_seconds": 60}',
        'Retry loop detected. Try a different approach or ask the user.');
```

**`token_budget`** — Deny if session token usage exceeds a limit.

```sql
INSERT INTO policies (id, name, priority, effect,
                      rule_type, rule_config, message)
VALUES ('token-budget', 'Session token budget', 70, 'deny',
        'token_budget', '{"max_tokens_per_session": 500000}',
        'Session token budget exceeded.');
```

**`time_window`** — Only active during specific hours (uses the `schedule` column as a cron expression for when the rule is in effect).

```sql
INSERT INTO policies (id, name, priority, effect, actor_pattern, resource_pattern,
                      rule_type, rule_config, schedule, message)
VALUES ('no-prod-jobs-daytime', 'No prod jobs 9-5', 80, 'deny',
        'agent:prod-*', 'job:*',
        'time_window', '{}', '0 9-17 * * 1-5',
        'Production agents cannot run jobs during business hours.');
```

### 4.6 SQL filter generation

For data access queries, the policy engine generates WHERE clauses instead of evaluating row-by-row:

```
Policy: "Agent B can only see shared facts or its own facts"
Generated: WHERE (scope = 'shared' OR agent_id = 'agent-b')

Policy: "Facts with confidence < 0.3 are hidden"
Generated: AND confidence >= 0.3
```

This is applied at the SQL level — the query never returns rows the agent shouldn't see.

### 4.7 Secret redaction

Eighteen regex patterns are applied before any data is written to SQLite:

- API keys (OpenAI, AWS, GCP, Azure, Stripe, etc.)
- Snowflake tokens and connection strings
- JWTs and bearer tokens
- PEM private keys
- Password/secret/token assignments in code
- Database connection URIs

Redaction is non-configurable and always-on. It runs before policy evaluation, before storage, before logging. Secrets never touch disk.

### 4.8 Audit trail

Every policy decision is logged:

```sql
policy_audit (
    id TEXT PRIMARY KEY,
    policy_id TEXT,                     -- which rule matched (null if default-deny)
    actor TEXT NOT NULL,
    action TEXT NOT NULL,
    resource TEXT NOT NULL,
    effect TEXT NOT NULL,               -- 'allowed', 'denied', 'audited'
    reason TEXT,
    session_id TEXT,
    created_at INTEGER NOT NULL
)
```

The audit trail is queryable. "Why was this tool call denied?" "How many policy violations this week?" "Which agent triggers the most audits?"

---

## 5. Tool System

### 5.1 Tool sources

Tools come from three places:

1. **Built-in tools** — shipped with Moneypenny. File I/O, shell exec, HTTP requests, SQL queries. Always available, always governed by policy.
2. **MCP tools** — discovered dynamically via Model Context Protocol. `sqlite-agent` connects to MCP servers and discovers available tools at runtime via `tools/list`.
3. **sqlite-js tools** — user-defined JavaScript functions stored in the SQLite database. Created via `js_create_scalar`, persisted via `js_init_table`, synced across agents via `sqlite-sync`.

### 5.2 Tool lifecycle

```
Tool defined (MCP, sqlite-js, or built-in)
    │
    ▼
Tool registered in SQLite
    │
    ▼
Tool discoverable via RAG
    │       (agent searches for capabilities by intent)
    │
    ▼
Agent requests tool call
    │
    ▼
Policy engine evaluates: allow(agent, "call", tool)
    │
    ├── DENY  → block, log, inform agent
    │
    ├── AUDIT → log, continue
    │
    └── ALLOW → execute
              │
              ▼
        Tool executes
              │
              ▼
        Post-execution audit
              │
              ▼
        Result returned to agent
        Secret redaction applied
```

### 5.3 Tool discovery via RAG

The agent doesn't need a static tool list. When planning a task, the agent describes what it needs, and the system searches the tools/skills tables via hybrid search. Relevant tools surface with descriptions. The agent selects which to use.

This means the agent can work with an unbounded number of tools — as long as they're indexed and searchable, they're discoverable.

### 5.4 sqlite-js tool persistence

Tools created via sqlite-js are persisted in the database:

```sql
SELECT js_create_scalar('fetch_weather', '(function(args) {
    // tool implementation
})');

SELECT js_init_table();  -- persists to SQLite, enables sync
```

These tools:
- Survive restarts (persisted in SQLite)
- Sync across agents (via `sqlite-sync`)
- Are governed by the policy engine (same as any tool)
- Are discoverable via RAG (the tool name and description are indexed)

---

## 6. Search

### 6.1 Hybrid search with RRF

All memory stores are searchable through one interface combining:

- **Vector similarity** — cosine distance on embeddings via `sqlite-vector`. Handles semantic matches ("deployment issues" finds "CI/CD pipeline failures").
- **Full-text search** — FTS5 with BM25 ranking. Handles exact tokens (error codes, variable names, UUIDs, function names).
- **Reciprocal Rank Fusion** — merges both ranking signals without score calibration. `score = Σ 1/(k + rank_i)` with k=60.

### 6.2 Search across stores

A single search query can span Facts, Log, and Knowledge. Results are tagged with their source store. The system can weight stores differently depending on context — more Facts for "what do I know about X", more Log for "when did we discuss X", more Knowledge for "how do I do X".

### 6.3 Policy-filtered search

Search results are filtered at the SQL level via policy-generated WHERE clauses. An agent never sees results it's not authorized to access. This is enforced in the query, not in post-processing.

---

## 7. Sync

### 7.1 What syncs

| Store | Syncs? | Why |
|---|---|---|
| Facts | Yes | Shared knowledge across agents |
| Log | No | Per-agent, per-session history |
| Knowledge | Yes | Shared reference material |
| Skills | Yes | Learned capabilities propagate |
| Policies | Yes | Governance rules apply fleet-wide |
| Jobs | Yes | Fleet-wide schedules propagate to all agents |
| Job runs | Yes | Visibility into execution across the fleet |
| Scratch | No | Session-scoped, ephemeral |

### 7.2 How it works

`sqlite-sync` uses CRDTs (Conflict-free Replicated Data Types) for automatic, conflict-free merging. When two agents independently learn conflicting facts, the CRDT layer resolves deterministically — no manual conflict resolution, no data loss.

Sync is optional and configurable per-agent. An agent can operate fully isolated (no sync), sync with specific peers, or sync with all agents via SQLite Cloud.

### 7.3 Shared knowledge propagation

Agent A discovers a fact → extraction pipeline stores it → `sqlite-sync` propagates to Agent B's database → Agent B's fact pointers update → Agent B now knows what Agent A learned.

This is the "multi-agent memory mesh" — each agent gets smarter as the mesh grows.

---

## 8. Job Scheduler

### 8.1 Design

The job scheduler is SQLite-native. Jobs are rows in a table. Job logic lives in sqlite-js functions. The orchestration layer polls the schedule table and dispatches due jobs to agent workers.

No external cron. No separate scheduler process. No Redis queues. The schedule is data in the same database that syncs across agents.

### 8.2 Schema

```sql
jobs (
    id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL,             -- which agent executes this
    name TEXT NOT NULL,
    description TEXT,

    -- Schedule
    schedule TEXT NOT NULL,             -- cron expression: '*/30 * * * *'
    next_run_at INTEGER NOT NULL,       -- precomputed next execution time
    last_run_at INTEGER,
    timezone TEXT DEFAULT 'UTC',

    -- What to execute
    job_type TEXT NOT NULL,             -- 'prompt', 'tool', 'js', 'pipeline'
    payload TEXT NOT NULL,              -- JSON: depends on job_type

    -- Behavior
    max_retries INTEGER DEFAULT 0,
    retry_delay_ms INTEGER DEFAULT 5000,
    timeout_ms INTEGER DEFAULT 30000,
    overlap_policy TEXT DEFAULT 'skip', -- 'skip' (if still running), 'queue', 'allow'

    -- State
    status TEXT DEFAULT 'active',       -- 'active', 'paused', 'disabled'
    enabled INTEGER DEFAULT 1,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
)

job_runs (
    id TEXT PRIMARY KEY,
    job_id TEXT NOT NULL,
    agent_id TEXT NOT NULL,
    started_at INTEGER NOT NULL,
    ended_at INTEGER,
    status TEXT NOT NULL,               -- 'running', 'success', 'error', 'timeout', 'denied'
    result TEXT,                         -- output or error message
    policy_decision TEXT,               -- 'allowed', 'denied'
    retry_count INTEGER DEFAULT 0,
    created_at INTEGER NOT NULL
)
```

### 8.3 Job types

**`prompt`** — Send a message to the agent as if a user sent it. The agent processes it through the full loop (context assembly, policy, LLM, tools, memory). This is the "heartbeat" — the agent can run autonomous tasks on a schedule.

```json
{
    "job_type": "prompt",
    "payload": {
        "message": "Check for new issues in the GitHub repo and summarize any critical bugs.",
        "channel": "internal"
    }
}
```

**`tool`** — Call a specific tool directly, bypassing the LLM. Still governed by the policy engine. Useful for maintenance tasks.

```json
{
    "job_type": "tool",
    "payload": {
        "tool": "memory_add_directory",
        "args": { "path": "/docs", "context": "project-docs" }
    }
}
```

**`js`** — Execute a sqlite-js function. The function has access to the full SQLite database. Useful for custom data processing, cleanup, aggregation.

```json
{
    "job_type": "js",
    "payload": {
        "function": "daily_digest",
        "args": []
    }
}
```

**`pipeline`** — Run a sequence of steps: tool calls, prompts, and JS functions chained together. Each step's output feeds the next. The pipeline is atomic — if any step fails, the whole job fails and rolls back.

```json
{
    "job_type": "pipeline",
    "payload": {
        "steps": [
            { "type": "tool", "tool": "fetch_metrics", "args": {} },
            { "type": "js", "function": "format_report", "args": ["$prev"] },
            { "type": "prompt", "message": "Analyze this report and flag anomalies: $prev" }
        ]
    }
}
```

### 8.4 Orchestration flow

```
Gateway (scheduler loop)
    │
    │  Every second: SELECT * FROM jobs
    │  WHERE enabled = 1
    │    AND status = 'active'
    │    AND next_run_at <= now()
    │
    ├── For each due job:
    │     │
    │     ├── Check overlap_policy
    │     │   (skip if already running and policy = 'skip')
    │     │
    │     ├── Policy engine: allow(agent, "execute", job)
    │     │   (jobs are governed like any other action)
    │     │
    │     ├── Dispatch to agent worker
    │     │   (the worker that owns this agent's .db)
    │     │
    │     ├── Worker executes job within a transaction
    │     │   BEGIN;
    │     │     INSERT INTO job_runs (status='running')
    │     │     Execute job payload
    │     │     UPDATE job_runs (status='success'|'error')
    │     │     UPDATE jobs SET last_run_at, next_run_at
    │     │   COMMIT;
    │     │
    │     └── On failure: retry per max_retries, then mark failed
    │
    └── Sleep until next check
```

### 8.5 sqlite-js as the job logic layer

sqlite-js functions are the natural fit for job logic because:

- They're stored in SQLite — same database, same transaction, same sync.
- They have full SQL access — a JS function can query facts, update knowledge, call other functions.
- They sync across agents — a job defined on Agent A with a sqlite-js function propagates to Agent B.
- They're governed by policy — the policy engine checks `allow(agent, "execute", js_function)` before running.

A user creates a custom daily digest:

```sql
-- Define the logic
SELECT js_create_scalar('daily_digest', '(function(args) {
    var rows = SQL("SELECT content FROM facts WHERE updated_at > ? ORDER BY confidence DESC LIMIT 20",
                   [Date.now()/1000 - 86400]);
    var digest = rows.map(r => "- " + r.content).join("\n");
    SQL("INSERT INTO scratch (id, session_id, key, content, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
        [UUID(), CURRENT_SESSION, "daily_digest", digest, Date.now()/1000, Date.now()/1000]);
    return digest;
})');

-- Schedule it
INSERT INTO jobs (id, agent_id, name, schedule, job_type, payload, next_run_at, created_at, updated_at)
VALUES ('daily-digest', 'agent-alpha', 'Daily fact digest', '0 9 * * *',
        'js', '{"function": "daily_digest", "args": []}',
        strftime('%s','now'), strftime('%s','now'), strftime('%s','now'));
```

The function runs every morning at 9am, summarizes yesterday's new facts, and stores the result in Scratch for the agent's next session.

### 8.6 Jobs sync

Jobs sync across agents via `sqlite-sync`. A fleet-wide job (e.g., "re-index knowledge base daily") can be defined once and propagate to all agents. Each agent's scheduler picks it up and runs it locally against its own database.

The `job_runs` table also syncs — giving visibility into job execution across the fleet.

### 8.7 Jobs and the policy engine

Jobs are governed like everything else:

```sql
-- Only trusted agents can run pipeline jobs
INSERT INTO policies (id, name, priority, effect, actor_pattern, resource_pattern, message)
VALUES ('pipeline-trusted', 'Pipelines require trust', 100, 'deny',
        'agent:untrusted-*', 'job:pipeline:*', 'Untrusted agents cannot run pipeline jobs.');

-- No jobs during business hours on production agents
INSERT INTO policies (id, name, priority, effect, actor_pattern, schedule, message)
VALUES ('no-prod-jobs-daytime', 'No prod jobs 9-5', 80, 'deny',
        'agent:prod-*', '0 9-17 * * 1-5', 'Production agents cannot run scheduled jobs during business hours.');
```

---

## 9. Existing Components

These ship today as cross-platform binaries with package manager distribution:

| Component | Role in Moneypenny | Status |
|---|---|---|
| `sqlite-ai` | LLM inference, embeddings, chat, audio, vision (GGUF) | Shipping |
| `sqlite-vector` | Vector search, SIMD-optimized, 6 distance metrics | Shipping |
| `sqlite-memory` | Persistent memory, hybrid search, markdown chunking | Shipping |
| `sqlite-rag` | Hybrid search engine with RRF, multi-format docs | Shipping |
| `sqlite-sync` | CRDT sync, offline-first, row-level security | Shipping |
| `sqlite-agent` | Autonomous agent loop inside SQLite, MCP tool use | Shipping |
| `sqlite-js` | User-defined JS functions, aggregates, windows | Shipping |
| `sqlite-wasm` | Browser runtime with OPFS, bundles sync+vector+memory | Shipping |
| `local-first-coco` | Policy engine, audit, secret redaction, session memory | Shipping (as coco-db) |

---

## 10. What Needs to Be Built

### Phase 1: Core (MVP)

| Component | Description | Depends on |
|---|---|---|
| `mp` binary | Rust binary, statically links SQLite + extensions | — |
| Agent loop | Message → context assembly → policy → LLM → tools → store → respond | All extensions |
| LLM trait | Pluggable provider interface, SqliteAi + Http implementations | sqlite-ai |
| Memory stores | Facts, Log, Knowledge, Scratch — schema + CRUD + search | sqlite-vector, sqlite-memory |
| Extraction pipeline | Async fact extraction with ADD/UPDATE/DELETE/NOOP | LLM trait |
| Progressive compression | Three-level fact compression + expansion | Extraction pipeline |
| Policy engine | Custom rule engine, behavioral rules, SQL filter gen | — |
| Job scheduler | SQLite-native cron. Jobs table, job types, dispatch loop | Agent loop, policy engine |
| sqlite-js job logic | User-defined job functions, persisted + syncable | sqlite-js |
| CLI channel | Interactive terminal, the first channel | Agent loop |
| `mp init/start` | Bootstrap + run commands | Everything above |

### Phase 2: Multi-agent + Channels

| Component | Description |
|---|---|
| Gateway | Message routing, agent registry, lifecycle management |
| Worker isolation | One process per agent, owned database |
| Sync integration | Facts, Knowledge, Skills, Policies, Jobs across agents |
| Pipeline jobs | Multi-step job chains with atomic rollback |
| Slack adapter | Channel implementation |
| Discord adapter | Channel implementation |
| HTTP API adapter | REST/WebSocket interface for custom integrations |

### Phase 3: Ecosystem

| Component | Description |
|---|---|
| Skill marketplace | Shareable, versionable skill packs |
| Web UI | Conversation, memory browser, audit viewer, policy editor |
| WASM runtime | Browser-native agent via sqlite-wasm |
| Telegram adapter | Channel implementation |
| Additional channels | WhatsApp, iMessage, email, etc. |

---

## 11. Influences and Prior Art

| System | What we learned | What we do differently |
|---|---|---|
| **Mem0** | LLM-as-memory-manager. ADD/UPDATE/DELETE/NOOP extraction pipeline. Custom extraction prompts. 26% accuracy gain over OpenAI memory. | No external vector DB. No Neo4j. Everything in SQLite. Transactional. Governed. |
| **Letta (MemGPT)** | Tiered memory (core/recall/archival). OS-inspired hierarchy. | Automatic capture, not agent-driven. Progressive compression instead of fixed tiers. |
| **A-Mem** | Zettelkasten-style atomic notes. Dynamic linking. Memory evolution. 85-93% token reduction. | Fact linking via SQL edges. Revisiting facts triggers re-extraction and evolution. |
| **OpenClaw** | Markdown-as-source-of-truth. Hybrid search. Temporal decay. 272K GitHub stars prove demand. | Database-native, not file-based. Transactional. Multi-agent sync. Governed. |
| **ctx (ctxpipe.ai)** | Four memory types: working/episodic/semantic/procedural + temporal dimension. "Context window is limited; what gets retrieved is the whole game." Fleet-scale governance. | Local-first, not centralized graph. Same insights, SQLite-native implementation. Scratch = working memory. Skills = procedural memory. |
| **Oso (design influence)** | `allow(actor, action, resource)` as universal predicate. SQL constraint generation. Embeddable evaluation. | Custom Rust engine — no external dependency. SQL rows for static rules, behavioral rules for dynamic state. Policies sync via CRDTs. Applied to agent governance, not just data access. |
| **coco-db** | Policy engine, secret redaction, session memory, hybrid search with RRF, progressive disclosure in RAG, auto-capture via hooks. | Generalized from Snowflake-specific to universal agent platform. Same patterns, broader scope. |

---

## 12. Streaming

### 12.1 End-to-end flow

Streaming is the default response mode. Tokens flow from the LLM through the orchestrator to the channel with minimal buffering.

```
LLM (token stream)
    │
    ▼
Orchestrator
    ├─ Accumulates tokens into a buffer for:
    │   - Tool call detection (structured JSON in token stream)
    │   - Secret redaction (pattern matching on accumulated text)
    │   - Audit logging (complete message stored after stream ends)
    │
    ├─ Forwards displayable tokens to channel immediately
    │
    └─ On stream end:
        - Store complete message in Log
        - Trigger extraction pipeline (async)
        - If tool calls detected: execute, then resume streaming with results
```

### 12.2 Channel adaptation

Channels that support streaming (`capabilities().supports_streaming`) receive tokens as they arrive. Channels that don't (e.g., email, some webhook APIs) receive the complete response after the stream ends. The orchestrator handles this transparently — the agent loop doesn't change.

### 12.3 Tool call interruption

When the LLM emits a tool call mid-stream, the orchestrator:

1. Pauses the token stream to the channel
2. Evaluates the tool call against the policy engine
3. Executes (or denies) the tool
4. Injects the result back into the LLM context
5. Resumes streaming

The user sees a natural pause while the tool executes, then the response continues. No separate "thinking" or "tool use" UI required at the protocol level — channels can optionally render tool calls if they support it (via `capabilities()`).

---

## 13. Error Handling and Agent Recovery

### 13.1 Policy denial flow

When the policy engine denies a tool call, the denial is not a fatal error — it's information the agent can act on.

```
Agent requests: shell_exec("rm -rf /data")
    │
    ▼
Policy engine: DENY — "Destructive shell operations are blocked."
    │
    ▼
Orchestrator injects denial as a tool result:
    {
        "tool": "shell_exec",
        "status": "denied",
        "message": "Destructive shell operations are blocked by policy.",
        "policy_id": "no-destructive-shell"
    }
    │
    ▼
Agent receives denial as context, adjusts approach:
    "I can't delete that directory directly. Let me use
     a safer approach — I'll list the contents first and
     ask for confirmation."
```

The agent stays in its loop. No crash, no retry — just a course correction. The denial is logged in `policy_audit` and the adjusted approach is captured in Log.

### 13.2 Tool execution errors

Tool errors (network failures, invalid arguments, timeouts) follow the same pattern: the error is returned as a tool result, the agent sees it and adapts. The orchestrator enforces a configurable retry limit per tool call per turn to prevent infinite loops.

```
Default: max 3 retries per tool per turn.
After 3 failures: the orchestrator injects a system message:
    "Tool {name} has failed 3 times. Consider an alternative approach."
```

### 13.3 Agent stuck detection

Ported from coco-db's `SessionAnalyze`. The orchestrator monitors for:

- **Retry loops** — same tool called with same arguments repeatedly
- **Error clusters** — multiple tool failures in rapid succession
- **Context thrashing** — agent alternating between contradictory approaches

When detected, the orchestrator injects a diagnostic prompt:

```
"You appear to be in a retry loop calling {tool} with the same arguments.
The last 3 attempts all returned: {error}. Consider a different approach
or ask the user for guidance."
```

This is logged as a behavioral event in `policy_audit` with effect `'intervention'`.

### 13.4 Crash recovery

If the agent worker crashes mid-transaction, SQLite's WAL journal guarantees the database rolls back to the last consistent state. No orphaned writes, no partial facts, no half-executed tool chains.

On restart, the gateway detects the failed worker, restarts it, and the agent resumes from its last committed state. The session's rolling summary and fact pointers are intact because they were committed in prior transactions.

---

## 14. Behavioral Policies

Beyond static tool/content rules, the policy engine supports pattern-based policies that detect and respond to agent behavior over time.

### 14.1 Three policy categories

| Category | Evaluates | Examples |
|---|---|---|
| **Tool-level** | Can this agent call this tool? | Block shell_exec for untrusted agents |
| **Content-level** | What are the arguments/content? | Block DROP SQL, redact PII |
| **Behavioral** | What pattern is the agent exhibiting? | Rate limit, retry loop detection, escalation |

### 14.2 Behavioral rule types

**Rate limiting:**

```sql
INSERT INTO policies (id, name, priority, effect, action_pattern, resource_pattern,
                      rule_type, rule_config, message)
VALUES ('rate-limit-shell', 'Shell rate limit', 90, 'deny',
        'call', 'tool:shell_*',
        'rate_limit', '{"max": 10, "window_seconds": 300}',
        'Rate limit exceeded: max 10 shell commands per 5 minutes.');
```

**Retry loop detection:**

```sql
INSERT INTO policies (id, name, priority, effect,
                      rule_type, rule_config, message)
VALUES ('no-retry-loops', 'Detect retry loops', 85, 'intervene',
        'retry_loop', '{"same_tool_same_args": 3, "window_seconds": 60}',
        'Retry loop detected. Try a different approach or ask the user.');
```

**Cost/token budget:**

```sql
INSERT INTO policies (id, name, priority, effect,
                      rule_type, rule_config, message)
VALUES ('token-budget', 'Session token budget', 70, 'deny',
        'token_budget', '{"max_tokens_per_session": 500000}',
        'Session token budget exceeded. Wrap up or start a new session.');
```

### 14.3 Implementation

Behavioral policies require state — they evaluate patterns *over time*, not single events. The orchestrator maintains a lightweight in-memory window of recent actions per agent (tool calls, token counts, error counts). This window is not persisted — it resets on worker restart. The window feeds into the policy evaluation:

```
Policy engine receives:
    actor, action, resource      (current event)
    + agent_state {              (behavioral context)
        tool_call_counts_5m,
        error_count_5m,
        total_tokens_session,
        last_n_tool_calls,
        repeated_call_count
    }
```

Static rules (tool-level, content-level) ignore `agent_state`. Behavioral rules use it.

---

## 15. Encryption at Rest

### 15.1 Database encryption

Every agent `.db` file is encrypted at rest using SQLite Encryption Extension (SEE) or SQLCipher. The encryption key is:

- On macOS: stored in the system Keychain
- On Linux: stored in the kernel keyring or a user-provided key file
- On Windows: stored in the Windows Credential Manager
- In WASM: derived from a user-provided passphrase

The key is never written to disk in plaintext. The database is decrypted in-memory at open time and re-encrypted on every write. If the key is lost, the database is unrecoverable — this is by design.

### 15.2 What's encrypted

Everything. Facts, Log, Knowledge, Scratch, policies, audit trail, embeddings, job definitions, sync metadata. The `.db` file is opaque without the key. This means a stolen laptop or a lost USB drive doesn't leak agent memory.

### 15.3 Sync and encryption

Each agent's database is encrypted with its own key. `sqlite-sync` handles encryption transparently — data is decrypted for sync operations and re-encrypted at rest on each device. The sync transport (to SQLite Cloud) uses TLS. Data at rest on each endpoint is encrypted independently.

---

## 16. Fact Scope Model

### 16.1 Scopes

Every fact has a scope that controls visibility and sync behavior:

| Scope | Visible to | Syncs? | Use case |
|---|---|---|---|
| `private` | Owning agent only | No | Personal preferences, user-specific knowledge |
| `shared` | All agents in the mesh | Yes | Team knowledge, project conventions, learned patterns |
| `protected` | Agents with matching role | Yes (filtered) | Sensitive knowledge visible only to trusted agents |

### 16.2 How scope is assigned

**Default: `private`.** Facts extracted from conversations are private by default. They belong to the agent that extracted them.

**Promotion to `shared`:** Facts become shared when:

- The user explicitly says "remember this for all agents" or similar
- The extraction pipeline detects a fact that's project-level (not user-specific) — e.g., "the ORDERS table uses soft deletes" is project knowledge, not personal preference
- A fact is independently extracted by multiple agents (convergence signal) — if 3 agents all learn the same thing, it's likely shared knowledge
- An admin promotes a fact via CLI or API

**Scope in the extraction prompt:** The extraction LLM is prompted to classify each candidate fact:

```
For each fact, determine:
- content, summary, pointer, keywords, confidence
- scope: "private" if this is a personal preference or user-specific,
         "shared" if this is project/team knowledge that all agents should know
```

### 16.3 Policy enforcement on scope

The policy engine enforces scope at the SQL level:

```
Default policy for all agents:
    allow(agent, "read", fact) if fact.scope = "private" and fact.agent_id = agent.id;
    allow(agent, "read", fact) if fact.scope = "shared";
    allow(agent, "read", fact) if fact.scope = "protected" and agent.trust_level in fact.required_roles;
```

This compiles to a WHERE clause on every fact query. An agent physically cannot retrieve facts outside its scope.

---

## 17. Search Quality

### 17.1 MMR re-ranking (diversity)

After hybrid search (vector + FTS5 + RRF) returns candidates, MMR (Maximal Marginal Relevance) re-ranks to balance relevance with diversity. This prevents near-duplicate results from consuming the context budget.

**Algorithm:**

1. Start with the highest-scoring result.
2. For each remaining candidate, compute: `score = λ × relevance - (1-λ) × max_similarity_to_already_selected`
3. Select the candidate with the highest MMR score. Repeat until K results.

Similarity between results uses Jaccard similarity on tokenized content (fast, no embedding computation needed).

**Default λ = 0.7** — biased toward relevance, with enough diversity penalty to remove near-duplicates.

### 17.2 Cross-store deduplication

When a search spans Facts, Log, and Knowledge, the same information might appear in multiple stores (a fact extracted from a conversation, the original conversation in Log, and a document in Knowledge that says the same thing). The search layer deduplicates by:

1. Grouping results by content similarity (embedding cosine > 0.92)
2. Keeping the highest-scoring result from the highest-priority store (Facts > Knowledge > Log)
3. Annotating the result with its source stores so the agent knows the provenance

### 17.3 Store weighting

The system adjusts store weights based on query intent:

| Query signal | Facts weight | Log weight | Knowledge weight |
|---|---|---|---|
| "What do I/we know about X" | High | Low | Medium |
| "When did we discuss X" | Low | High | Low |
| "How do I do X" | Medium | Low | High |
| Default (no signal) | 0.4 | 0.2 | 0.4 |

Intent detection is lightweight — keyword matching on the query, not an LLM call. Weights are tunable per-agent.

---

## 18. Token Budget Allocation

### 18.1 Budget model

The context window is a fixed resource. The allocator divides it into reserved and flexible segments:

```
Total budget: N tokens (model-dependent)

Reserved (non-negotiable):
    System prompt + persona          ~500 tokens
    Active policies                  ~200 tokens
    Fact pointers (all Level 2)      ~2,000 tokens (scales with fact count)
    Current message                  ~500 tokens
    Response headroom                ~2,000 tokens (space for the LLM to respond)
                                     ─────────────
    Reserved total                   ~5,200 tokens

Flexible (allocated dynamically):
    Remaining = N - reserved

    Split into:
        Auto-expanded facts (Level 1)    20% of remaining
        Scratch                          10% of remaining
        Log retrieval                    30% of remaining
        Knowledge retrieval              40% of remaining
```

### 18.2 Dynamic rebalancing

The default 20/10/30/40 split adjusts based on context:

- **If Scratch is empty** (no active multi-step task): its 10% reallocates to Log and Knowledge equally.
- **If the query matches many facts** at Level 1: the fact expansion budget can borrow from Log/Knowledge (up to 40% of remaining).
- **If the session is new** (no conversation history): Log budget reallocates to Knowledge.
- **If the session is deep** (100+ messages): Log gets a larger share for continuity.

### 18.3 Per-agent override

Agents can override the default split in their configuration:

```sql
UPDATE agents SET config = json_set(config,
    '$.budget.facts_pct', 25,
    '$.budget.scratch_pct', 5,
    '$.budget.log_pct', 20,
    '$.budget.knowledge_pct', 50
) WHERE id = 'agent-researcher';
```

A "researcher" agent that primarily queries documents gets 50% Knowledge. A "support" agent that relies on conversation history gets more Log.

---

## 19. CLI Surface

### 19.1 Design principles

- Every command does one thing.
- Output is human-readable by default, `--json` for machine consumption.
- Destructive operations require `--confirm` or interactive confirmation.
- All state-changing commands are policy-governed (the CLI authenticates as an actor).

### 19.2 Commands

```
mp init                                 Create moneypenny.toml + data directory
mp start                                Start gateway + all configured agents
mp stop                                 Graceful shutdown

Agent management:
mp agent list                           List all agents
mp agent create <name>                  Create a new agent
mp agent delete <name> --confirm        Delete an agent and its database
mp agent status [name]                  Show agent status, memory stats, sync state
mp agent config <name> <key> <val>      Set agent configuration

Conversations:
mp chat [agent]                         Interactive CLI chat (default channel)
mp send <agent> "message"               Send a one-off message, print response

Memory:
mp facts list [agent]                   List all facts (pointer + summary)
mp facts search "query" [agent]         Hybrid search across facts
mp facts inspect <id>                   Show full fact with audit history
mp facts promote <id> --shared          Promote a fact to shared scope
mp facts delete <id> --confirm          Delete a fact

Knowledge:
mp ingest <path> [agent]                Ingest documents (files, directories)
mp ingest --url <url> [agent]           Ingest from URL
mp knowledge search "query"             Search ingested knowledge
mp knowledge list                       List ingested documents

Skills:
mp skill add <path> [agent]             Add a skill from a markdown file
mp skill list [agent]                   List skills with usage stats
mp skill promote <id>                   Manually promote a skill

Policies:
mp policy list                          List all active policies
mp policy add --name "..." ...          Add a policy rule
mp policy test "DROP TABLE foo"         Dry-run: would this be allowed?
mp policy violations [--last 7d]        Show recent violations
mp policy load <file>                   Load policies from JSON file

Jobs:
mp job list [agent]                     List scheduled jobs
mp job create --name "..." ...          Create a job
mp job run <id>                         Trigger a job immediately
mp job pause <id>                       Pause a job
mp job history [id]                     Show job run history

Audit:
mp audit [agent]                        Recent audit trail
mp audit search "query"                 Search audit entries
mp audit export --format sql            Export audit as SQL/JSON/CSV

Sync:
mp sync status                          Show sync state per agent
mp sync now [agent]                     Trigger immediate sync
mp sync connect <url>                   Configure SQLite Cloud connection

Debug:
mp db query "SELECT ..." [agent]        Read-only SQL against an agent's database
mp db schema [agent]                    Show database schema
mp health                               Gateway + worker health check
```

### 19.3 The `init` experience

```bash
$ mp init

  Moneypenny v0.1.0

  Creating project in ./mp-data

  ✓ Created moneypenny.toml
  ✓ Created data directory
  ✓ Created models directory
  ✓ Initialized agent "main"
      LLM:       anthropic (claude-sonnet-4-20250514)
      Embedding: local (nomic-embed-text-v1.5, 768D)
  ✓ Downloaded embedding model (nomic-embed-text-v1.5, 274MB)

  Ready. Run `mp start` to begin.

$ mp start

  Moneypenny v0.1.0

  ✓ Gateway started on pid 4821
  ✓ Agent "main" loaded (0 facts, 0 sessions)
  ✓ CLI channel active

  Type a message to begin, or /help for commands.

  > hello
  Hello! I'm your Moneypenny agent. My memory and search run
  locally — embeddings never leave your machine. How can I help?

  > remember that our team standup is at 9:15am Pacific
  Got it — I'll remember that.

  > /facts
  1 fact stored:
    • "Team standup: 9:15am Pacific"  [confidence: 1.0, scope: private]
```

---

## 20. Observability

### 20.1 Health checks

```
GET /health (HTTP API channel, when enabled)

{
    "status": "healthy",
    "gateway": { "pid": 4821, "uptime_seconds": 3600 },
    "agents": [
        {
            "id": "main",
            "status": "running",
            "facts": 142,
            "sessions": 38,
            "db_size_bytes": 52428800,
            "last_sync": "2026-03-06T22:15:00Z",
            "llm_provider": "anthropic",
            "llm_model": "claude-sonnet-4-20250514"
        }
    ],
    "jobs": { "active": 3, "last_failure": null },
    "sync": { "status": "connected", "pending_changes": 0 }
}
```

### 20.2 Metrics

The gateway exposes Prometheus-compatible metrics (when the HTTP API channel is enabled):

```
mp_messages_total{agent, channel, role}                Counter
mp_tool_calls_total{agent, tool, status}               Counter
mp_policy_decisions_total{agent, effect}                Counter
mp_facts_total{agent, scope}                            Gauge
mp_extraction_duration_seconds{agent}                    Histogram
mp_llm_latency_seconds{agent, provider, model}          Histogram
mp_job_runs_total{agent, job, status}                   Counter
mp_sync_operations_total{agent, direction}              Counter
mp_db_size_bytes{agent}                                 Gauge
mp_token_usage_total{agent, provider, purpose}          Counter
```

### 20.3 Structured logging

All components emit structured JSON logs to stderr:

```json
{"ts":"2026-03-06T22:15:00Z","level":"info","component":"extraction","agent":"main","event":"fact_added","fact_id":"f_abc123","pointer":"Team standup: 9:15am","confidence":1.0}
{"ts":"2026-03-06T22:15:01Z","level":"warn","component":"policy","agent":"main","event":"denied","tool":"shell_exec","policy":"no-destructive-shell","reason":"Destructive shell operations are blocked."}
```

Log level is configurable per-component. Default: `info` for orchestrator, `warn` for extensions.

### 20.4 `mp health` CLI

```bash
$ mp health

  Gateway:  ✓ running (pid 4821, uptime 1h 12m)
  Agent "main":
    Status:    running
    Facts:     142 (138 private, 4 shared)
    Sessions:  38 (current: active, 12 messages)
    DB size:   50.0 MB
    LLM:       anthropic / claude-sonnet-4-20250514
    Sync:      connected, 0 pending
    Jobs:      3 active, 0 failed (last 24h)
    Policies:  12 rules, 3 violations (last 24h)
```

---

## 21. Agent-to-Agent Communication

### 21.1 Model

Agents can communicate directly via an internal message channel. This is not sync (passive data propagation) — it's active delegation.

```
Agent A → Gateway → Agent B
```

The gateway routes inter-agent messages like any other channel message. Agent B receives it as an incoming message, processes it through the full loop (context assembly, policy, LLM, tools), and returns a response to Agent A.

### 21.2 Delegation

An agent can delegate a task to another agent via a built-in tool:

```
delegate(agent_id: "agent-researcher", message: "Find the top 5 open issues in our GitHub repo and summarize them.")
```

The orchestrator:

1. Checks policy: `allow(agent-main, "delegate", agent-researcher)`
2. Routes the message to agent-researcher's worker
3. Agent-researcher processes the task (may use its own tools, memory, policies)
4. Returns the result to agent-main as a tool result
5. Agent-main incorporates the result into its response

### 21.3 Governance

Delegation is policy-governed. You can control:

- Which agents can delegate to which other agents
- Which channels can trigger delegation
- Maximum delegation depth (prevent infinite chains: A → B → C → A)
- Token/cost budgets for delegated tasks

```sql
-- Only trusted agents can delegate
INSERT INTO policies (id, name, priority, effect, action_pattern, actor_pattern, message)
VALUES ('delegate-trusted', 'Delegation requires trust', 100, 'deny',
        'delegate', 'agent:untrusted-*', 'Untrusted agents cannot delegate tasks.');

-- Max delegation depth of 2
INSERT INTO policies (id, name, priority, effect, action_pattern,
                      rule_type, rule_config, message)
VALUES ('delegate-depth', 'Max delegation depth', 90, 'deny', 'delegate',
        'delegation_depth', '{"max_depth": 2}',
        'Maximum delegation depth exceeded.');
```

---

## 22. Design Values

- **Simple.** Few moving parts. Convention over configuration. Sensible defaults.
- **Rock-solid.** ACID transactions. Deny-by-default policies. Secret redaction always on. Full audit trail. Crash recovery via WAL.
- **Intuitive.** Predictable behavior. Good mental model. Three commands to a working agent.
- **Seamless.** Everything works together naturally. No seams between memory, search, policy, and sync.
- **Elegant.** The design should feel inevitable, not forced. Database-as-runtime isn't a gimmick — it's the only architecture that delivers transactional guarantees on the full agent state.
- **Governed.** Every action is auditable. Every resource is policy-controlled. Every secret is redacted. Trust is earned through visibility, not assumed through obscurity.
- **Observable.** Health checks, metrics, structured logs, audit trail. You can always answer: what is the agent doing, why did it do that, and is it healthy.
