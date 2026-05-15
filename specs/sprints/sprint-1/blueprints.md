# Richer Blueprint System

### New frontmatter fields

```yaml
---
name: research-assistant
description: Autonomous research agent with fact-checking
model: claude-sonnet-4-6
tools:
  - web_fetch
  - code_search
  - memory_search
  - memory_add
deny_paths:
  - ".env*"
  - "credentials.*"
max_turns: 50

# NEW: Sub-agent declarations
sub_agents:
  - name: fact-checker
    blueprint: ./fact-checker.md
    model: claude-3-5-haiku-20241022
    history: fresh              # fresh | persistent
    memory: read_only           # shared | isolated | read_only
  - name: summarizer
    blueprint: ./summarizer.md
    model: gemini-2.5-flash

# NEW: Iteration strategy
strategy: research
research:
  max_iterations: 5

# NEW: Memory configuration
memory:
  context: "research"
  inject: true
  extract: true

# NEW: Guardrail overrides (per-blueprint)
guardrails:
  max_cost_usd: 0.50
  max_iterations: 15
  filesystem_sandbox:
    - "./src"
    - "./docs"

# Existing schedule, enhanced
schedule:
  cron: "0 */6 * * *"
  trigger: cron
  input_template: "Review and summarize recent activity in the codebase"
  enabled: true
---
```

### Sub-agent execution

Each sub-agent in `sub_agents` is registered as a tool via the existing
`delegate` tool infrastructure. When the parent LLM calls the sub-agent
tool, the executor:

1. Loads the sub-agent blueprint
2. Creates a provider with the sub-agent's model (or parent's if not set)
3. Creates fresh or persistent history based on `history` mode
4. Configures memory sharing based on `memory` mode
5. Runs `createAgentLoop` + `loop.run()` with the sub-agent config
6. Returns the response as the tool result

Nesting is limited to 3 levels. Each level inherits the parent's cost
budget minus what has been consumed.

### Acceptance criteria

- [ ] New frontmatter fields parse without breaking existing blueprints
- [ ] Sub-agent tool calls work through the delegate executor
- [ ] Sub-agent nesting respects 3-level depth limit
- [ ] Cost budget propagates correctly to sub-agents
- [ ] `guardrails.filesystem_sandbox` restricts tool access
- [ ] `memory.inject` enriches system prompt from stored knowledge

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 5.1 | Parse new frontmatter fields (extend Zod schema in `agents/schema.ts`) | 1 day |
| 5.2 | Sub-agent tool registration and execution via `delegate` | 2 days |
| 5.3 | Memory config: inject (system prompt enrichment) and extract (post-session) | 1.5 days |
| 5.4 | Guardrail override wiring (cost guard, max iterations, sandbox) | 1 day |
