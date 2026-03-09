# MPQ — Moneypenny Query Language

## Design Spec (Locked)

MPQ is a SQL-subset query language that serves as the single interface to Moneypenny. Every platform operation — memory, search, policy, jobs, audit, ingestion, agent admin — is expressible as an MPQ expression through one MCP tool.

The 12 domain tools (`moneypenny.memory`, `moneypenny.policy`, etc.) have been removed from the MCP surface. The agent interacts through a single `moneypenny.query` tool. The tool description teaches the agent the syntax at runtime via patterns and examples. Legacy domain tool calls are still routed internally for graceful degradation.


## Core Principles

1. **One tool, one language.** Every Moneypenny operation is an MPQ string.
2. **The description is the syntax.** The MCP tool description defines the grammar. No external docs needed. The agent learns from examples in the description.
3. **Verb-first dispatch.** The leading keyword determines the operation type. The parser is a hand-rolled recursive descent dispatcher, not a grammar engine.
4. **One call, one transaction.** Every `moneypenny.query` invocation runs inside a single SQLite transaction. Multi-statement (`;`-separated) expressions are atomic — all succeed or all roll back.
5. **Policy evaluates on the expression.** The raw MPQ string is the `sql_content` field on `PolicyRequest`. Existing pattern-matching policies work unchanged.
6. **The expression is the audit trail.** Every MPQ expression is logged as a single, human-readable audit record.


## Verb Reference

Every MPQ expression starts with a verb. The verb determines the operation and the expected syntax.

### Reads

```
SEARCH <store> [WHERE <filters>] [SINCE <duration>] [BEFORE <duration>] [MODE fts|vector|hybrid]
    [| SORT <field> ASC|DESC]
    [| TAKE <n>]
    [| OFFSET <n>]
    [| COUNT]
```

Default search mode is `hybrid`. Default TAKE is 50. Maximum TAKE is 500.

Stores: `facts`, `knowledge`, `log`, `audit`

Examples:
```
SEARCH facts WHERE topic = "auth" SINCE 7d
SEARCH facts WHERE topic = "auth" AND confidence > 0.7 | SORT confidence DESC | TAKE 10
SEARCH knowledge WHERE "deployment guide" MODE fts | TAKE 5
SEARCH log WHERE action = "delete" SINCE 24h | TAKE 20
SEARCH audit WHERE actor = "junior-bot" AND action = "delete" | TAKE 50
SEARCH facts | COUNT
```

### Writes (Memory)

```
INSERT INTO facts (<content> [, key=value ...])
UPDATE facts SET key=value [, key=value ...] WHERE id = <id>
DELETE FROM facts WHERE <filters>
```

Examples:
```
INSERT INTO facts ("Redis is the preferred caching layer", topic="infrastructure", confidence=0.9)
UPDATE facts SET confidence = 0.5 WHERE id = "fact-abc-123"
DELETE FROM facts WHERE confidence < 0.3 AND BEFORE 30d
```

### Knowledge

```
INGEST <url> [AS <name>]
```

Examples:
```
INGEST "https://docs.example.com/api-guide"
INGEST "https://wiki.internal/runbook" AS "incident-runbook"
```

### Policy

```
CREATE POLICY <name> <effect> <action> ON <resource> [FOR AGENT <id>] [MESSAGE <reason>]
EVALUATE POLICY ON (<actor>, <action>, <resource>)
EXPLAIN POLICY FOR (<actor>, <action>, <resource>)
```

Effect: `allow` | `deny` | `audit`

Examples:
```
CREATE POLICY "no-junior-deletes" deny DELETE ON facts FOR AGENT "junior-bot" MESSAGE "junior agents cannot delete facts"
CREATE POLICY "audit-inserts" audit INSERT ON facts MESSAGE "log all fact creation"
EVALUATE POLICY ON ("junior-bot", "delete", "facts")
EXPLAIN POLICY FOR ("junior-bot", "delete", "facts")
```

### Jobs

```
CREATE JOB <name> SCHEDULE <cron> [TYPE <type>] [PAYLOAD <json>]
RUN JOB <name>
PAUSE JOB <name>
RESUME JOB <name>
LIST JOBS
HISTORY JOB <name> [| TAKE <n>]
```

Examples:
```
CREATE JOB "daily-digest" SCHEDULE "0 9 * * *" TYPE prompt PAYLOAD {"prompt": "Summarize today"}
RUN JOB "daily-digest"
PAUSE JOB "daily-digest"
HISTORY JOB "daily-digest" | TAKE 5
```

### Agents

```
CREATE AGENT <name> [CONFIG key=value ...]
DELETE AGENT <name>
CONFIG AGENT <name> SET key=value [, key=value ...]
```

Examples:
```
CREATE AGENT "reviewer" CONFIG model="claude-3", temperature=0.2
CONFIG AGENT "reviewer" SET temperature=0.5
DELETE AGENT "reviewer"
```

### Sessions

```
RESOLVE SESSION [<id>]
LIST SESSIONS [| TAKE <n>]
```

### Skills & Tools

```
CREATE SKILL <content>
PROMOTE SKILL <id>
CREATE TOOL <name> LANGUAGE js BODY <code>
LIST TOOLS
DELETE TOOL <name>
```

### Embedding Admin

```
EMBEDDING STATUS
EMBEDDING RETRY DEAD
EMBEDDING BACKFILL [| PROCESS]
```

### Pipelines

Pipeline stages are separated by `|`. Stages flow left to right. Each stage operates on the output of the previous.

Valid pipeline stages after a `SEARCH`:
- `SORT <field> ASC|DESC`
- `TAKE <n>`
- `OFFSET <n>`
- `COUNT`
- `DELETE` (v2)
- `SUMMARIZE INTO <name>` (v2)
- `TAG key=value` (v2)

Valid pipeline stage after other reads:
- `TAKE <n>`

### Multi-Statement

Multiple independent operations are separated by `;`. All operations within a single `moneypenny.query` call execute inside one transaction.

```
DELETE FROM facts WHERE confidence < 0.2 AND BEFORE 90d; SEARCH facts WHERE topic = "auth" | TAKE 10
```


## Filter Syntax

Filters appear after `WHERE` in SEARCH, UPDATE, and DELETE expressions.

### Operators
- `=`, `!=`, `>`, `<`, `>=`, `<=`
- `LIKE` (SQL LIKE with `%` wildcards)

### Logical
- `AND` — all conditions joined by AND (flat filter chains)
- No `OR`, `NOT`, or parenthesized grouping in v1 — these come back as a package deal in v2

### Temporal (SEARCH only)
- `SINCE <duration>` — results newer than duration
- `BEFORE <duration>` — results older than duration
- Durations: `<n>d`, `<n>h`, `<n>m`, `<n>s`

### Scoping
- `SCOPE private|shared|protected`
- `AGENT <id>` or `AGENT *`

### Special
- `id = <value>` — exact ID match
- Bare string in SEARCH (no `WHERE`): treated as search query text
  - `SEARCH facts "authentication patterns"` → searches for "authentication patterns"


## Literals

- Strings: `"double-quoted"` only
- Numbers: integers and floats
- Durations: `7d`, `24h`, `30m`, `90s`
- Booleans: `true`, `false`
- No null. No arrays in v1.


## MCP Tool

### Name
`moneypenny.query`

### Input Schema
```json
{
  "type": "object",
  "properties": {
    "expression": {
      "type": "string",
      "description": "MPQ expression"
    },
    "dry_run": {
      "type": "boolean",
      "default": false,
      "description": "Parse and policy-check without executing. Returns the execution plan."
    }
  },
  "required": ["expression"]
}
```

### Tool Description (for LLM consumption, ~300 tokens)

```
Moneypenny Query (MPQ). One tool for all Moneypenny operations.

SEARCH <store> [WHERE <filters>] [SINCE <duration>] [| SORT field ASC|DESC] [| TAKE n]
INSERT INTO facts ("content", key=value ...)
UPDATE facts SET key=value WHERE id = "id"
DELETE FROM facts WHERE <filters>
INGEST "url"
CREATE POLICY "name" allow|deny|audit <action> ON <resource> [FOR AGENT "id"] [MESSAGE "reason"]
EVALUATE POLICY ON ("actor", "action", "resource")
CREATE JOB "name" SCHEDULE "cron" [TYPE type]
RUN|PAUSE|RESUME JOB "name"
CREATE AGENT "name" [CONFIG key=value]
SEARCH audit WHERE <filters> [| TAKE n]

Stores: facts, knowledge, log, audit
Filters: field = value, field > value, field LIKE "%pattern%", AND
Durations: 7d, 24h, 30m
Pipeline: chain stages with |
Multi-statement: separate with ;

Examples:
  SEARCH facts WHERE topic = "auth" AND confidence > 0.7 SINCE 7d | SORT confidence DESC | TAKE 10
  INSERT INTO facts ("Redis is preferred for caching", topic="infrastructure", confidence=0.9)
  DELETE FROM facts WHERE confidence < 0.3 AND BEFORE 30d
  CREATE POLICY "no-junior-deletes" deny DELETE ON facts FOR AGENT "junior-bot"
  SEARCH knowledge WHERE "deployment" | TAKE 5
  SEARCH facts | COUNT
  CREATE JOB "digest" SCHEDULE "0 9 * * *" TYPE prompt
  SEARCH audit WHERE action = "delete" SINCE 24h | TAKE 20
```

### Response Envelope

Same `OperationResponse` used by existing operations:

```json
{
  "ok": true,
  "code": "success",
  "message": "2 statements executed",
  "data": {
    "results": [ ... ],
    "meta": {
      "statements": 1,
      "stages": 3,
      "total_rows": 10,
      "execution_ms": 12
    }
  },
  "policy": { "effect": "allow", "policy_id": "...", "reason": "..." },
  "audit": { "recorded": true }
}
```

### Error Responses

**Parse error:**
```json
{
  "ok": false,
  "code": "parse_error",
  "message": "unexpected token at position 34",
  "data": {
    "position": 34,
    "expected": ["SINCE", "AND", "|"],
    "got": "DURING",
    "hint": "use SINCE <duration> for relative time filters (e.g. SINCE 7d)"
  }
}
```

**Policy denial:**
```json
{
  "ok": false,
  "code": "policy_denied",
  "message": "policy denied: junior agents cannot delete facts",
  "data": {
    "statement_index": 0,
    "policy_id": "policy-abc",
    "reason": "junior agents cannot delete facts"
  }
}
```

**Execution error:**
```json
{
  "ok": false,
  "code": "execution_error",
  "message": "store 'factss' does not exist",
  "data": {
    "statement_index": 0,
    "stage_index": 0,
    "hint": "valid stores: facts, knowledge, log, audit"
  }
}
```


## Parser Architecture

### Approach
Hand-rolled recursive descent in Rust. No grammar engine, no build dependencies.

### Structure
Module `mp-core/src/dsl/` with:
- `mod.rs` — public API: `parse(input: &str) -> Result<Program, ParseError>`
- `lexer.rs` — tokenizer: whitespace-split + string literal handling + operator recognition
- `ast.rs` — AST types: `Program`, `Statement`, `Stage`, `Filter`, `Literal`, etc.
- `parser.rs` — recursive descent: verb dispatch → per-verb parsers
- `validate.rs` — semantic checks: store exists, fields valid, TAKE ceiling, mutation ordering
- `execute.rs` — pipeline runner: AST → SQL compilation for reads, dispatch_operation for mutations

### Parse Flow
```
input: &str
  → lexer → Vec<Token>
  → split on Semicolon → Vec<Vec<Token>>  (statements)
  → per-statement:
      → match leading verb
      → verb-specific parser → Statement AST
      → split on Pipe → Vec<Stage>
      → semantic validation
  → Program { statements: Vec<Statement> }
```

### AST Types (implemented)

```rust
pub struct Program {
    pub statements: Vec<Statement>,
    pub raw: String,
}

pub struct Statement {
    pub head: Head,                // verb-specific payload
    pub pipeline: Vec<PipeStage>,  // post-query stages
    pub raw: String,               // original expression for audit + policy sql_content
}

pub enum Head {
    Search(SearchHead), Insert(InsertHead), Update(UpdateHead), Delete(DeleteHead),
    Ingest(IngestHead),
    CreatePolicy(CreatePolicyHead), EvaluatePolicy(EvalPolicyHead), ExplainPolicy(EvalPolicyHead),
    CreateJob(CreateJobHead), RunJob(StringArg), PauseJob(StringArg), ResumeJob(StringArg),
    ListJobs, HistoryJob(StringArg),
    CreateAgent(CreateAgentHead), DeleteAgent(StringArg), ConfigAgent(ConfigAgentHead),
    ResolveSession(OptionalStringArg), ListSessions,
    CreateSkill(StringArg), PromoteSkill(StringArg),
    CreateTool(CreateToolHead), ListTools, DeleteTool(StringArg),
    EmbeddingStatus, EmbeddingRetryDead, EmbeddingBackfill,
}

pub enum PipeStage {
    Sort { field: String, order: SortOrder },
    Take(usize),
    Offset(usize),
    Count,
    Process,
}

// Flat AND-joined conditions — no recursion, no precedence
pub enum Condition {
    Cmp { field: String, op: CmpOp, value: Literal },
    Like { field: String, pattern: String },
    Scope(String),
    Agent(String),
    Since(DurationLit),
    Before(DurationLit),
}

pub enum Literal { Str(String), Int(i64), Float(f64), Bool(bool) }
pub enum SearchMode { Fts, Vector, Hybrid }
pub enum SortOrder { Asc, Desc }
pub enum CmpOp { Eq, Ne, Gt, Lt, Ge, Le }
pub enum Store { Facts, Knowledge, Log, Audit }
pub struct DurationLit { pub amount: u64, pub unit: DurationUnit }
```

### Execution

- **Reads**: compile SEARCH + WHERE + SORT + TAKE + OFFSET into SQL, call existing `fts5_search_*`, `vector_search_*`, or `hybrid_search` functions in `search.rs`. COUNT runs in Rust over the result set.
- **Mutations**: construct an `OperationRequest` from the AST, call `dispatch_operation`. Gets policy + audit for free.
- **Admin ops**: construct an `OperationRequest`, call `dispatch_operation`.
- **Multi-statement**: iterate statements, execute each, collect results. Whole call is one transaction.

### Policy Integration

Two-layer enforcement, no new policy infrastructure:

| Layer | action | resource | sql_content | When |
|-------|--------|----------|-------------|------|
| **Top-level** | `"query"` | `"mpq"` | full expression | Before any execution. `sql_pattern` regex matches the whole expression (e.g. deny expressions containing `DELETE`). |
| **Per-statement** | verb-specific | store-specific | individual statement | Before each statement executes. `head_to_policy_tuple()` maps verb → `(action, resource)` so existing policies fire granularly. |

Verb → policy mapping examples:
- `SEARCH facts` → `("search", "memory")`
- `DELETE FROM facts` → `("delete", "fact")`
- `CREATE POLICY` → `("create", "policy")`
- `INGEST` → `("ingest", "knowledge")`

Both layers pass `session_id` and `trace_id` via `PolicyAuditContext` for audit trail linkage.

Behavioral rules (rate_limit, retry_loop, token_budget, time_window) apply at both layers.

### Audit

Every `moneypenny.query` call generates:
1. **Top-level audit record** — `policy_audit` row with `action=query, resource=mpq`
2. **Per-statement audit records** — one `policy_audit` row per statement with verb-specific `action`/`resource`
3. **Per-operation audit records** — mutations dispatched through `operations::execute` generate their own audit via existing paths

All audit records carry `session_id` and `correlation_id` (trace_id) for cross-referencing.


## Phasing

### v1 (MVP) ✅ Implemented
- Full verb set (all current operations expressible)
- SEARCH with FTS5, vector, and hybrid modes
- Flat filters (AND-only, no OR/NOT/parens)
- Pipeline stages: SORT, TAKE, OFFSET, COUNT, PROCESS
- Multi-statement with `;`
- Single transaction per call
- `dry_run` mode
- Structured error responses with hints
- Two-layer policy: top-level `sql_content` + per-statement verb-specific checks
- Audit context with session/trace propagation
- Hand-rolled recursive descent parser in `mp-core/src/dsl/`
- Domain tools removed from MCP surface; `moneypenny.query` is the sole primary tool (+ `capabilities` + `execute` fallback)
- Legacy domain tool routing preserved internally for graceful degradation

### v2
- OR, NOT, and parenthesized filter grouping
- Pipeline mutation stages: DELETE, SUMMARIZE INTO, TAG
- `explain` mode (execute with per-stage diagnostics)
- Plan-level policy: new `pipeline` rule type for AST-level checks
- Grammar profiles per agent
- Cost estimation

### v3
- UPDATE SET as pipeline stage
- MERGE INTO deduplication
- Sub-pipelines / nested expressions
- Streaming results for large pipelines


## Reserved Words

```
SEARCH INSERT UPDATE DELETE FROM INTO SET WHERE AND OR NOT LIKE
SINCE BEFORE SCOPE AGENT MODE SORT ASC DESC TAKE OFFSET COUNT
INGEST AS CREATE EVALUATE EXPLAIN POLICY ALLOW DENY AUDIT ON FOR MESSAGE
JOB SCHEDULE TYPE PAYLOAD RUN PAUSE RESUME LIST HISTORY
CONFIG SKILL PROMOTE TOOL LANGUAGE BODY
EMBEDDING STATUS RETRY DEAD BACKFILL PROCESS
SESSION RESOLVE
true false
```


## Testing Strategy

### Unit Tests
- Lexer: token stream for each expression pattern
- Parser: AST output for every verb, every filter operator, every pipeline stage
- Validation: rejection of invalid stores, bad types, TAKE exceeding ceiling, mutations before reads in pipeline

### Integration Tests
- Full round-trip: MPQ expression → parse → execute against test SQLite → verify results
- Multi-statement atomicity: second statement fails → first statement rolled back
- Policy integration: MPQ expression matched by sql_pattern → denied

### Property Tests
- Any input string either parses successfully or returns a structured ParseError. Never panics.
