# moneypenny

The portable intelligence layer for coding agents. Persistent context, governance, and cost controls from a single `.mp/` directory in your repo. Standalone CLI, MCP sidecar for Cursor or Claude Code, or background daemon with scheduled agents.

## Why moneypenny

**A full-stack coding agent from a single SQLite file.** Persistent sessions, hybrid search, local embeddings, policy enforcement, cost tracking, audit logs. One `.mp/mp.db` per repo, no external services. Bring your Anthropic, OpenAI, or Google key.

**Your codebase, indexed and searchable in milliseconds.** On first run, moneypenny walks your repo, chunks every source file, and builds a full-text BM25 index with vector embeddings generated locally. No API calls. Subsequent runs are incremental. `mp search "where do we handle rate limiting"` returns ranked results from keyword and semantic matching in under a millisecond.

**The more you build, the more it knows.** Every session persists. Pick up next week where you left off. At session end, moneypenny extracts architecture decisions, code conventions, debugging insights, and user preferences into learned skills. Existing skills on the same topic are merged, not duplicated. Project knowledge compounds across sessions.

**Version-controlled governance.** On first run, moneypenny scaffolds `.mp/agents/_global.yaml` with sensible defaults: deny `.git/` and `node_modules/`, set exclude patterns, configure turn limits. Agent definitions layer additional restrictions inline. Policies support cost caps, audit trails, confirmation prompts, and regex argument matching. Commit them alongside your code.

```yaml
# .mp/agents/_global.yaml
deny_paths:
  - "**/.git/**"
  - "**/node_modules/**"

exclude_patterns:
  - "**/node_modules/**"
  - "**/.git/**"
  - "**/dist/**"

max_turns: 64
```

**Agents defined as markdown files.** Drop a `.md` file with YAML frontmatter into `.mp/agents/`. Frontmatter declares the model, tools, permissions, and turn limits. The body is the system prompt. All agents share a single `mp.db` with sessions scoped by agent name.

```markdown
---
name: security-reviewer
description: Reviews code for security vulnerabilities
model: claude-sonnet-4-6
tools:
  - read_file
  - grep
  - code_search
deny_paths:
  - "**/.env*"
  - "**/secrets/**"
deny_tools:
  - "run_terminal_cmd"
max_turns: 32
---

You are a security-focused code reviewer. Focus on input validation,
auth flaws, injection vulnerabilities, and credential exposure.
```

**MCP in both directions.** `mp setup cursor` gives Cursor, Claude Code, or any MCP client access to moneypenny's search, index, and governance. Going the other direction, point moneypenny at external MCP servers (GitHub, Linear, Slack) via `mcp-servers.json`. Their tools appear as `mcp__github__create_pull_request`, governed by the same pipeline.

**Delegate to subagents.** The `delegate` tool spawns a child loop with its own tool restrictions, iteration budget, and cost ceiling. A code review subagent gets `read_file` and `code_search` but not `bash`. It runs, returns a result, and the parent continues. No shared memory, no leaking permissions.

**Cloud sync for teams.** `mp cloud init` enables CRDT-based replication via sqlite-sync. Policies, learned skills, agent definitions, and config sync across machines automatically. The agent stays local-first. The cloud is a coordination layer, not a dependency.

### Also includes

- **50+ bundled skills** for refactoring, design patterns, and code smell identification. Available via `read_skill` and `delegate`.
- **Per-model cost tracking** with rate tables for 25+ models. Per-turn and per-session spend reported and enforced.
- **Local SLM** for session naming, compaction, and knowledge extraction. Bundled Qwen 0.5B, no API calls for housekeeping.
- **Parallel tool execution.** Multiple tool calls in a single turn run concurrently, each policy-gated.
- **Cache-optimized prompts.** Static context separated from dynamic context with an explicit cache breakpoint for LLM prefix caching.

## Quickstart

### Prerequisites

- [Bun](https://bun.sh) >= 1.0
- An LLM API key (Anthropic, OpenAI, or Google)

### Install

```bash
git clone https://github.com/nicholasgasior/moneypenny.git
cd moneypenny
pnpm install
```

### Configure

Set your API key via environment variable:

```bash
export ANTHROPIC_API_KEY=sk-ant-...
```

Or persist it in the global config:

```bash
mp config set anthropic_api_key sk-ant-...
```

### Download models

Local embedding and text generation models for zero-cost indexing, session naming, and compaction:

```bash
mp setup models
```

### Verify setup

```bash
mp doctor
```

### Start coding

```bash
mp chat "refactor the auth module to use JWT"
```

### Set up as MCP sidecar for Cursor

```bash
mp setup cursor
# Restart Cursor — moneypenny tools are now available via MCP
```

### Run background agents

Define an agent in `.mp/agents/pr-reviewer.md`:

```markdown
---
name: PR Reviewer
schedule: "0 9 * * 1-5"
model: claude-sonnet-4-6
tools:
  - code_search
  - read_file
  - "mcp__github__*"
max_turns: 20
---

Review open PRs. Assess code quality, potential bugs, and security concerns.
```

Start the daemon:

```bash
mp serve
```

## CLI Commands

| Command | Description |
|---|---|
| `mp chat [message]` | Interactive agent session |
| `mp search <query>` | Hybrid code search (BM25 + vector) |
| `mp index` | Build or refresh the codebase index |
| `mp inspect` | Query agent state (events, messages, metrics) |
| `mp mcp` | Start MCP server (stdio) |
| `mp setup <target>` | Configure integrations (`cursor`, `claude`, `models`) |
| `mp config <get\|set>` | Read/write global configuration |
| `mp doctor` | Validate environment and configuration |
| `mp policy <subcommand>` | Manage governance policies (`list`, `add`, `remove`, `sync`) |
| `mp events` | Query the event/audit log |
| `mp serve` | Start daemon (scheduler + HTTP + MCP) |
| `mp agents <subcommand>` | List, run, and manage background agents |
| `mp cloud <subcommand>` | Cloud sync and team management |

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│  Transports                                                 │
│    CLI · MCP Server · HTTP API · Daemon                     │
├─────────────────────────────────────────────────────────────┤
│  @moneypenny/agents  Agent loader, scheduler, cron, chaining│
│  @moneypenny/http    Localhost API + SSE event streaming     │
│  @moneypenny/mcp     MCP server + client + IDE sidecar      │
│  @moneypenny/cloud   Optional sync (sqlite-sync, team.db)   │
├─────────────────────────────────────────────────────────────┤
│  @moneypenny/loop    Agent turn loop, LLM providers, cost   │
│  @moneypenny/tools   Built-in tools (file ops, bash, git)   │
│  @moneypenny/skills  SKILL.md discovery, subagent delegation│
├─────────────────────────────────────────────────────────────┤
│  @moneypenny/ctx     Prompt assembly + governance pipeline  │
│  @moneypenny/search  Indexer, chunker, hybrid BM25+vector   │
├─────────────────────────────────────────────────────────────┤
│  @moneypenny/db      SQLite + extensions (vector, ai, sync) │
└─────────────────────────────────────────────────────────────┘
```

### Repo layout

```
moneypenny/
├── apps/
│   └── cli/                 # mp CLI (Bun binary)
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
│   └── agents/              # Layer 10 — Agent loader, scheduler, runner
├── package.json             # pnpm workspace root
├── pnpm-workspace.yaml
└── tsconfig.base.json
```

### Storage model

Per-repository state lives in `.mp/`:

```
project/
└── .mp/
    ├── mp.db                    # Single DB: sessions, metrics, skills, config
    ├── workspace.sqlite         # Shared code index (file tree, chunks, FTS, vectors)
    ├── agents/
    │   ├── _global.yaml         # Repo-wide defaults (permissions, excludes, model)
    │   ├── default.md           # Default agent definition
    │   └── pr-reviewer.md       # Specialized agent
    └── skills/                  # User-defined SKILL.md files
```

Global config and models live in `~/.mp/`:

```
~/.mp/
├── config.json                # API keys, default model, preferences
└── models/
    ├── nomic-embed-text-v1.5.Q8_0.gguf    # Local embeddings (768 dims)
    └── qwen2.5-0.5b-instruct-q4_k_m.gguf  # Local text gen (session naming, compaction)
```

### Governance pipeline

Every tool call flows through: **pre-hooks → policy evaluate → execute → post-hooks → event log**.

Governance composes in two layers. **`_global.yaml`** defines repo-wide permissions and exclude patterns (`deny_paths`, `deny_tools`, `allow_paths`). **Agent `.md` files** layer additional restrictions. `deny_paths` and `deny_tools` merge additively with global config; `model`, `tools`, and `max_turns` override. For cost caps, audit, and confirmation rules, use the `policies` key in either file. Built-in guards for cost limits, credential redaction, and path boundaries are always active.

## License

MIT
