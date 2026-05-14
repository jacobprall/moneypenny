/**
 * Default blueprint scaffolded into `.swe/blueprints/` on first init.
 */

export const HELLO_AGENT_MD = `---
name: Hello World
description: >
  A starter blueprint that proves the runtime is working. Run it with
  \`swe agents run hello\` or uncomment the schedule to fire it on a cron.
enabled: true

# Uncomment the next two lines to run on a schedule:
# schedule: "*/5 * * * *"
# timezone: America/New_York

tools: []
permissions:
  allow: []
  deny: []

max_turns: 5
timeout_ms: 60000
---

# Hello Agent

You are a friendly smoke-test agent. Your job is to confirm the agent
runtime is working end-to-end: load, validate, run, and record output.

## Steps

1. Print the current date and time in ISO-8601 format.
2. Say "Hello from swe!" and briefly describe what you are.

## Output

Reply in a single short paragraph. Keep it under 50 words.

<!--
This file is an agent blueprint. Every folder inside \`.swe/blueprints/\`
becomes a blueprint whose id is the folder name (e.g. this one is "hello").

Frontmatter reference (all fields except \`name\` are optional):

  name            — Human-readable display name (required).
  description     — What the agent does.
  enabled         — true | false (default true).
  schedule        — 5- or 6-field cron expression.
  timezone        — IANA timezone; required when schedule is set.
  catch_up        — Run missed scheduled invocations (default false).
  model           — LLM model override (e.g. "claude-sonnet-4-20250514").
  max_turns       — Max agent loop iterations (default 30, max 500).
  timeout_ms      — Hard timeout in ms (default 15 min).
  tools           — List of tool names the agent may use.
  permissions     — { allow: [...], deny: [...] } for fine-grained control.
  skills          — List of skill names to attach.
  on_complete     — Agent ids to chain on success, e.g. [summarizer].
  on_failure      — Agent ids to chain on failure.

Everything below the frontmatter is the agent's system prompt.

To create a new blueprint:

  mkdir .swe/blueprints/my-agent
  cp .swe/blueprints/hello/agent.md .swe/blueprints/my-agent/agent.md
  # edit the new agent.md, then:
  swe agents reload   # or POST /api/agents/reload
  swe agents list     # verify it loaded
  swe agents run my-agent
-->
`;

export const DEFAULT_AGENTS: Record<string, string> = {
  hello: HELLO_AGENT_MD,
};
