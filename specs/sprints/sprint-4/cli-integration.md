# CLI Integration

### `mp eval` subcommands

```
mp eval run          Run end-to-end evaluation trials
mp eval sequence     Run a multi-session sequence evaluation
mp eval context      Run context-quality evaluation (no LLM)
mp eval report       Generate evaluation reports
mp eval import       Import tasks from SWE-bench JSON dataset
mp eval setup        Clone repos, build Docker images, verify environment
mp eval list-runners List available runners with capability info
mp eval leaderboard  Generate a ranked leaderboard
mp eval ci           CI regression gate
```

### Common options

```
mp eval run
  --tasks <dir>          Directory containing task YAML/JSON files
  --runners <names...>   Runner names to evaluate
  --model <model>        Model to use (default: claude-sonnet-4-6)
  --trials <n>           Number of trials per task per runner (default: 3)
  --repos-dir <dir>      Directory containing cloned repos (default: ./repos)
  --results-db <path>    Path to results SQLite database (default: ./results.sqlite)
  --timeout <ms>         Timeout per trial in ms (default: 300000)
  --filter <regex>       Only run tasks matching this regex
  --difficulty <level>   Only run tasks with this difficulty
  --parallelism <n>      Number of parallel trials (default: 1)
  --docker               Run each trial in a Docker container
  --docker-image <name>  Docker image (default: mp-eval-sandbox)
  --tools <names...>     Tool allowlist for mp-agent runner
```

### `mp eval list-runners`

Shows capability info per runner:

```
Available Runners
┌────────────┬───────────────┬───────┬────────┬───────┐
│ Runner     │ Persistence   │ Cost  │ Tokens │ Cache │
├────────────┼───────────────┼───────┼────────┼───────┤
│ mp-agent   │ Yes           │ ✓     │ ✓      │ ✓     │
│ claude     │ No            │ ✓     │ ✓      │ ✗     │
│ claude-mp  │ Yes           │ ✓     │ ✓      │ ✗     │
│ cursor     │ No            │ ✗     │ ✗      │ ✗     │
│ aider      │ No            │ ✗     │ ✗      │ ✗     │
│ codex      │ No            │ ~     │ ~      │ ✗     │
│ shell      │ No            │ ✗     │ ✗      │ ✗     │
│ http       │ No            │ ?     │ ?      │ ?     │
│ context    │ N/A           │ N/A   │ N/A    │ N/A   │
└────────────┴───────────────┴───────┴────────┴───────┘
✓ = fully tracked  ~ = partial  ✗ = not available  ? = depends on endpoint
```

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 13.1 | CLI arg parsing (all subcommands) | 1.5 days |
| 13.2 | Wire subcommands to harness/reporter/importer | 1 day |
| 13.3 | `list-runners` with capability table | 0.5 days |
| 13.4 | Makefile targets + documentation | 0.5 days |
