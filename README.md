# Moneypenny

Local-first coding agent platform. The brain is the database.

Moneypenny gives your coding agent persistent memory, learned skills, project awareness, file manipulation, and a web dashboard — all stored in a single SQLite database on your machine. It can read, write, and run code; remember facts across sessions; detect project conventions; and manage its own context autonomously.

## Quickstart

```bash
cd your-project

# Start everything: watcher + dashboard + background work loop
bun run /path/to/moneypenny/apps/cli/src/main.ts start

# Or: interactive coding chat
bun run /path/to/moneypenny/apps/cli/src/main.ts chat

# Or: MCP server for Cursor/Claude Desktop
bun run /path/to/moneypenny/apps/cli/src/main.ts serve
```

## Commands

### Core
| Command | Description |
|---------|-------------|
| `mp start` | Start watcher + dashboard (`:4966`) + background work loop |
| `mp serve` | Start MCP server (stdio) for Cursor/Claude Desktop |
| `mp chat [agent]` | Interactive streaming chat REPL |
| `mp chat --resume <id>` | Resume a previous session |
| `mp dashboard` | Start web dashboard only |

### Intelligence
| Command | Description |
|---------|-------------|
| `mp index [path]` | Index codebase with AST-aware chunking |
| `mp embed [batch]` | Generate embeddings for semantic search |
| `mp detect` | Detect project conventions from code |
| `mp skills` | List learned skills |
| `mp work` | Process pending work queue |
| `mp custodian` | Run custodian maintenance pipeline |

### Agent Pool
| Command | Description |
|---------|-------------|
| `mp pool run <agent> <task>` | Run background agent |
| `mp pool schedule` | Process scheduled jobs |

### Inspect
| Command | Description |
|---------|-------------|
| `mp status` | Database health |
| `mp context [agent]` | System prompt |
| `mp agents` | List agents |
| `mp policies` | List policies |
| `mp sessions` | List/view/search sessions |
| `mp costs` | Cost tracking |

## Chat Slash Commands

Type these during `mp chat`:

| Command | Description |
|---------|-------------|
| `/help` | Show available commands |
| `/context` | Show current system prompt |
| `/cost` | Session + daily cost |
| `/sessions` | Recent sessions |
| `/status` | System health |
| `/skills` | Learned skills |
| `/conventions` | Project conventions |
| `/clear` | Clear conversation |
| `/quit` | Exit |

## Architecture

```
The brain is the database.
┌──────────────────────────────────────────────────┐
│                    SQLite DB                      │
│  sessions → messages → session_pointers           │
│  code_chunks (AST-chunked, FTS5, embeddings)     │
│  skills, conventions, policies, agent_defs       │
│  work_queue, events, jobs, config                │
└──────────────────────────────────────────────────┘
        ↑               ↑              ↑
   @moneypenny/db   @moneypenny/engine  @moneypenny/mcp
   migrations       agent loop          stdio server
   context asm      19 tools            7+ resources
   work queue       LLM abstraction     client manager
                    embeddings
                    hooks + pool
                    strategies
                    custodian
                    governance
        ↑               ↑              ↑
                  apps/cli
          config sync, watcher, REPL
          AST indexer, scaffold
          web dashboard (Hono, 10 pages)
          root config (TOML)
```

## Tools (19)

The agent has 19 built-in tools that make it a coding agent, not just a chat wrapper:

### File Operations
- **read_file** — Read file contents with optional line range
- **read_files** — Batch read multiple files
- **write_file** — Write/create files with auto directory creation
- **list_directory** — List files and directories (recursive)
- **run_command** — Execute shell commands (builds, tests, git)

### Search
- **search_code** — Hybrid search (FTS5 + semantic) across indexed code
- **search_messages** — Full-text search across conversation history

### Memory
- **save_memory** — Persist knowledge/observations for future recall
- **recall_memory** — Search saved memories across sessions

### Session Management
- **expand_previous_session** — Retrieve session pointer summary
- **get_full_session** — Load full transcript
- **pin_session** / **unpin_session** — Pin important sessions
- **list_sessions** — Browse session history

### Knowledge
- **learn_skill** — Teach the agent a new skill
- **add_convention** — Add a project convention

### Introspection
- **query_db** — Read-only SQL against the intelligence database
- **context_curate** — 13-action super-tool for self-management (memory search/forget, cost review, skills CRUD, sessions manage, policy inspect, index status, prune stale chunks, conventions list)
- **run_maintenance** — Check work queue status

## LLM Provider Abstraction

Moneypenny routes LLM calls through a tier system so you can use expensive models for interactive work and cheap/local models for background housekeeping:

| Tier | Default | Used for |
|------|---------|----------|
| `strong` | claude-sonnet-4 | Interactive chat, complex reasoning |
| `fast` | claude-sonnet-4 | Summarization, convention detection, skill extraction |
| `local` | *(falls back to fast)* | Labeling, compaction, pointer key generation |

### Supported providers

| Prefix | Provider | Example |
|--------|----------|---------|
| `ollama:` | Local Ollama | `ollama:llama3.2`, `ollama:qwen2.5-coder` |
| `anthropic:` | Anthropic | `anthropic:claude-sonnet-4-20250514` |
| `openai:` | OpenAI | `openai:gpt-4o` |
| `google:` | Google | `google:gemini-2.0-flash` |
| *(bare)* | Auto-detected | `claude-sonnet-4-20250514`, `gpt-4o`, `gemini-2.0-flash` |

Configure in `moneypenny.toml`:

```toml
[models]
strong = "claude-sonnet-4-20250514"
fast   = "claude-sonnet-4-20250514"
local  = "ollama:llama3.2"
ollama_base_url = "http://localhost:11434/v1"
```

Or change live from the dashboard Settings page.

## Web Dashboard

`mp start` launches a web dashboard at `http://localhost:4966` with 10 pages:

| Page | What it shows |
|------|--------------|
| **Overview** | Session count, messages, code chunks, today's cost, work queue status |
| **Sessions** | Browse all sessions, FTS search, view full transcripts |
| **Agents** | Defined agents with models, triggers, tool sets, system prompts |
| **Skills** | Learned skills with confidence levels and instructions |
| **Conventions** | Detected and user-defined project patterns by category |
| **Events** | Audit log stream with type filtering |
| **Costs** | Today's spend + daily cost breakdown by agent |
| **Work Queue** | Pending and recently processed work items with status |
| **MCP Servers** | Manage external MCP server connections (add/remove, command, args, env) |
| **Settings** | Model tier config (strong/fast/local), API key status, policies, config store |

API endpoints: `/api/health`, `/api/config`, `/api/models`

## Context Views

Named context views assemble different system prompts for different tasks:

| View | Focus |
|------|-------|
| `default` | Full context: sessions, skills, conventions, policies |
| `coding` | Conventions, policies, codebase profile |
| `research` | Skills, session history, structured finding/source format |
| `refactoring` | Conventions, policies, structural focus |
| `review` | Conventions, correctness, edge cases, maintainability |

## Agent Strategies

| Strategy | Behavior |
|----------|----------|
| `standard` | Single-shot reply with tool use |
| `research` | Structured investigation with FINDING/SOURCE/GAPS + RESEARCH_COMPLETE report |
| `evolution` | Iterative self-improvement with plateau detection |

## Custodian Pipeline

`mp custodian` runs automated maintenance:

1. **Label** unlabeled sessions (LLM via `local` tier)
2. **Compact** long sessions (compress old messages, `local` tier)
3. **Archive** stale sessions (inactive > N days)
4. **Purge** old archived sessions (ensure pointers exist first)
5. **Summarize** sessions without pointers (`fast` tier)
6. **Consolidate** excess pointers (merge oldest, `fast` tier)
7. **Prune** stale code chunks

## Governance

Built-in hooks for security and cost control:

- **Credential Redactor** — strips API keys, tokens, passwords before LLM context
- **Budget Enforcer** — daily spend limits with warn/deny thresholds
- **Operation Logger** — per-tool latency and error tracking

## Configuration

### moneypenny.toml (repo root)

```toml
[agent]
name = "Moneypenny"
model = "claude-sonnet-4-20250514"
strategy = "standard"           # standard | research | evolution

[models]
strong = "claude-sonnet-4-20250514"     # interactive chat
fast   = "claude-sonnet-4-20250514"     # summarization, conventions, skills
local  = ""                              # labeling, compaction (e.g. "ollama:llama3.2")
ollama_base_url = "http://localhost:11434/v1"

[pointers]
cap = 20                        # max active session pointers
auto_summarize = true
auto_consolidate = true

[worker]
interval_ms = 30000             # background work loop interval
batch_size = 10

[custodian]
compact_after_turns = 50
archive_after_days = 30
purge_after_days = 90
chunk_prune_after_days = 14

[search]
fts_weight = 0.4
semantic_weight = 0.6
```

### .moneypenny/ (per-project)

```
.moneypenny/
  agents/
    default.toml      # Agent definition (model, tools, system prompt)
  policies/
    budget.toml       # Daily + session cost limits
  conventions.toml    # Project conventions
```

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `ANTHROPIC_API_KEY` | — | Required for chat |
| `OPENAI_API_KEY` | — | Required for embeddings |
| `GOOGLE_GENERATIVE_AI_API_KEY` | — | Optional, for Gemini models |
| `MP_DATA` | `~/.moneypenny` | Database directory |
| `MP_MODEL` | `claude-sonnet-4-20250514` | Default model |
| `MP_PORT` | `4966` | Dashboard port |
| `MP_WORK_INTERVAL` | `30000` | Work loop interval (ms) |

## Tech Stack

- **Runtime**: Bun
- **Database**: SQLite (bun:sqlite) with WAL mode
- **LLM**: Vercel AI SDK with tiered routing (Anthropic, OpenAI, Google, Ollama)
- **Embeddings**: OpenAI text-embedding-3-small (1536d)
- **Search**: FTS5 + cosine similarity hybrid
- **MCP**: @modelcontextprotocol/sdk (server + client)
- **Dashboard**: Hono + JSX (10 pages)
- **Config**: TOML with hot-reload

## License

MIT
