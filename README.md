# swe

The local-first coding agent platform where every agent is a SQLite database. Conversations, code understanding, accumulated knowledge, and governance all in one portable file. Interactive CLI, MCP sidecar for Cursor and Claude Code, or scheduled background agents. Same memory, same policies, different transports.

## Why swe

**A full-stack coding agent from a single SQLite file.** Persistent sessions, hybrid search, local embedding and text generation, policy enforcement, cost tracking, audit logs and more — all backed by one `.swe/` directory with no external services. Just bring your Anthropic, OpenAI, or Google key.

**Your codebase, indexed and searchable in milliseconds.** On first run, swe walks your repo, chunks every source file, builds a full-text BM25 index and generates vector embeddings locally via sqlite-ai — no API calls, no external services. Subsequent runs are incremental: only changed files are re-indexed. `swe search "where do we handle rate limiting"` returns ranked results from both keyword and semantic matching, fused with Reciprocal Rank Fusion, in under a millisecond.

**The more you build, the more it learns.** Every session persists — pick up next week where you left off. At session end, swe distills the conversation into durable "learned" skills: architecture decisions, code conventions, debugging insights, user preferences. Existing skills on the same topic are merged, not duplicated. The agent accumulates project-specific knowledge that compounds across every session.

**Governance as version-controlled YAML files.** Drop policy files into `.swe/policies/` and they're loaded on every session. Block destructive commands, protect sensitive paths, cap spend per session — policies match on tool, path, cost, args, and actor with `allow`, `deny`, `audit`, or `confirm` effects. Denied actions don't crash; the agent adapts. Credentials are scrubbed from tool output before reaching the LLM. Full audit trail on every operation. Commit your policies alongside your code.

```yaml
# .swe/policies/security.yaml
- name: protect-env-files
  effect: deny
  path: "*.env*"
  message: Prevent agent from reading or writing environment files

- name: no-force-push
  effect: deny
  tool: bash
  args: "push.*--force|push.*-f"
  message: Block force-push to any remote
```

**Agents defined as markdown blueprints.** Drop an `agent.md` with YAML frontmatter and a prompt into `.swe/blueprints/<name>/`. The frontmatter declares the schedule, model, tool whitelist, permission deny-list, cost limits, and chaining rules. The body is the agent's system prompt. When the agent runs, its state lives in `.swe/agents/<name>.db` — the blueprint is the template, the DB is the running agent. `swe serve` starts a daemon that runs a cron scheduler — with wake-from-sleep catch-up, per-agent concurrency locks, and `on_complete`/`on_failure` chaining.

```markdown
---
name: PR Reviewer
schedule: "0 9 * * 1-5"
tools: [code_search, file_read, "mcp__github__*"]
permissions:
  deny: [file_write, bash]
max_cost_per_session: 2.00
---
Review open PRs for code quality and security.
```

**Plug into any MCP client — or consume any MCP server.** `swe setup` gives Cursor, Claude Code, or any MCP client access to swe's hybrid code search, codebase index, and policy engine. Going the other direction, point swe at external MCP servers (GitHub, Linear, Slack) via `mcp-servers.json` — their tools become available to your agents as `mcp__github__create_pull_request`, governed by the same policy pipeline.

**Delegate to specialized subagents.** The `delegate` tool spawns a child loop with its own tool restrictions, iteration budget, and cost ceiling. A code review subagent gets `file_read` and `code_search` but not `bash`. It runs, produces a result, and the parent agent continues. No shared memory, no leaking permissions.

**Cloud sync for teams.** `swe cloud init` enables CRDT-based replication via sqlite-sync. Policies, learned skills, agent definitions, and config sync across databases and machines automatically. Session summaries sync up; team governance syncs down. The agent stays local-first. The cloud acts as a coordination layer, not a dependency.

### Also includes

- **50+ bundled skills** — refactoring techniques, design patterns (GoF), code smell identification. Available out of the box via the `read_skill` and `delegate` tools.
- **Per-model cost tracking** — rate tables for 25+ models across Anthropic, OpenAI, and Google. Per-turn and per-session spend reported and enforced.
- **Local SLM for zero-cost secondary tasks** — session naming, conversation compaction, and knowledge extraction run on a bundled Qwen 0.5B model. No API calls for housekeeping.
- **Parallel tool execution** — multiple tool calls in a single turn run concurrently, each independently policy-gated.
- **Cache-optimized prompts** — static context (system prompt, project overview) is separated from dynamic context (search results, conversation) with an explicit cache breakpoint for LLM prefix caching.

## Quickstart

### Prerequisites

- [Bun](https://bun.sh) >= 1.0
- An LLM API key (Anthropic, OpenAI, or Google)

### Install

```bash
git clone https://github.com/nicholasgasior/swe.git
cd swe
pnpm install
```

### Configure

Set your API key via environment variable:

```bash
export ANTHROPIC_API_KEY=sk-ant-...
```

Or persist it in the global config:

```bash
swe config set anthropic_api_key sk-ant-...
```

### Download models

Local embedding and text generation models for zero-cost indexing, session naming, and compaction:

```bash
swe setup models
```

### Verify setup

```bash
swe doctor
```

### Start coding

```bash
swe chat "refactor the auth module to use JWT"
```

### Set up as MCP sidecar for Cursor

```bash
swe setup cursor
# Restart Cursor — swe tools are now available via MCP
```

### Run background agents

Define a blueprint in `.swe/blueprints/pr-reviewer/agent.md`:

```markdown
---
name: PR Reviewer
schedule: "0 9 * * 1-5"
model: claude-sonnet-4-6
tools:
  - code_search
  - file_read
  - "mcp__github__*"
permissions:
  deny:
    - file_write
    - bash
---

Review open PRs. Assess code quality, potential bugs, and security concerns.
```

Start the daemon:

```bash
swe serve
```

## CLI Commands

| Command | Description |
|---|---|
| `swe chat [message]` | Interactive agent session |
| `swe search <query>` | Hybrid code search (BM25 + vector) |
| `swe index` | Build or refresh the codebase index |
| `swe inspect` | Query agent state (events, messages, metrics) |
| `swe mcp` | Start MCP server (stdio) |
| `swe setup <target>` | Configure integrations (`cursor`, `claude`, `models`) |
| `swe config <get\|set>` | Read/write global configuration |
| `swe doctor` | Validate environment and configuration |
| `swe policy <subcommand>` | Manage governance policies (`list`, `add`, `remove`, `sync`) |
| `swe events` | Query the event/audit log |
| `swe serve` | Start daemon (scheduler + HTTP + MCP) |
| `swe agents <subcommand>` | List, run, and manage background agents |
| `swe cloud <subcommand>` | Cloud sync and team management |

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│  Transports                                                 │
│    CLI · MCP Server · HTTP API · Daemon                     │
├─────────────────────────────────────────────────────────────┤
│  @swe/agents    Blueprints, scheduler, cron, agent chaining  │
│  @swe/http      Localhost API + SSE event streaming         │
│  @swe/mcp       MCP server + client + IDE sidecar           │
│  @swe/cloud     Optional sync (sqlite-sync, team.db)        │
├─────────────────────────────────────────────────────────────┤
│  @swe/loop      Agent turn loop, LLM providers, cost ctrl   │
│  @swe/tools     Built-in tools (file ops, bash, git, search)│
│  @swe/skills    SKILL.md discovery, subagent delegation      │
├─────────────────────────────────────────────────────────────┤
│  @swe/ctx       Prompt assembly + governance pipeline        │
│  @swe/search    Indexer, chunker, hybrid BM25+vector search  │
├─────────────────────────────────────────────────────────────┤
│  @swe/db        SQLite + extensions (vector, ai, sync, FTS5) │
└─────────────────────────────────────────────────────────────┘
```

### Repo layout

```
swe/
├── apps/
│   └── cli/                 # swe CLI (Bun binary)
├── packages/
│   ├── db/                  # Layer 1 — Storage (SQLite, schemas, migrations)
│   ├── search/              # Layer 2 — Code intelligence (indexing, hybrid search)
│   ├── ctx/                 # Layer 3 — Context assembly + governance pipeline
│   ├── loop/                # Layer 4 — Agent loop (LLM streaming, cost, tool exec)
│   ├── tools/               # Layer 5 — Tool registry + built-in tools
│   ├── skills/              # Layer 6 — Skills catalog + subagent definitions
│   ├── mcp/                 # Layer 7 — MCP server, client, sidecar, IDE setup
│   ├── http/                # Layer 8 — HTTP API + SSE routes
│   ├── cloud/               # Layer 9 — Cloud sync (sqlite-sync, team DB)
│   └── agents/              # Layer 10 — Blueprint loader, scheduler, runner
├── package.json             # pnpm workspace root
├── pnpm-workspace.yaml
└── tsconfig.base.json
```

### Layer dependencies

```
@swe/db            ← foundation (SQLite + extensions)
@swe/search        ← @swe/db
@swe/ctx           ← @swe/db
@swe/tools         ← @swe/db, @swe/search
@swe/loop          ← @swe/db, @swe/ctx, @swe/tools
@swe/skills        ← @swe/db
@swe/mcp           ← @swe/db, @swe/search, @swe/tools
@swe/http          ← @swe/db, @swe/agents
@swe/cloud         ← @swe/db
@swe/agents        ← @swe/db, @swe/loop
apps/cli           ← all packages
```

Each package can be used independently. `@swe/db` + `@swe/search` alone is a code search engine. `@swe/mcp` alone is an MCP server for code intelligence. `@swe/ctx` alone is a prompt formatter with governance.

### Storage model

Per-repository state lives in `.swe/`:

```
project/
└── .swe/
    ├── workspace.sqlite       # Shared code index (file tree, chunks, FTS, vectors)
    ├── agents/
    │   ├── default.db         # Default agent (conversations, metrics, config)
    │   └── pr-reviewer.db     # Named agent — runtime state for the blueprint
    ├── blueprints/
    │   ├── hello/agent.md     # Scaffolded starter blueprint
    │   └── pr-reviewer/agent.md
    ├── policies/
    │   └── defaults.yaml      # Governance rules (synced into DB on startup)
    └── skills/                # User-defined SKILL.md files
```

Global config and models live in `~/.swe/`:

```
~/.swe/
├── config.json                # API keys, default model, preferences
└── models/
    ├── nomic-embed-text-v1.5.Q8_0.gguf    # Local embeddings (768 dims)
    └── qwen2.5-0.5b-instruct-q4_k_m.gguf  # Local text gen (session naming, compaction)
```

### Governance pipeline

Every tool call flows through: **pre-hooks → policy evaluate → execute → post-hooks → event log**.

Policies support structured matching on tool name, file path, cost thresholds, argument patterns, and actor identity. Four effects: `allow`, `deny`, `audit`, `confirm`. Built-in guards for cost limits, credential redaction, and path boundaries are always active.

## License

Open source. MIT.
