---
name: policy-builder
description: >-
  Create, edit, and debug moneypenny governance rules. Use when the user asks
  about permissions, security rules, blocking tools, protecting files,
  cost limits, audit logging, or agent governance. Covers the flat
  permission keys, advanced policy rules, resolution semantics, and
  common patterns.
---

# Policy Builder

moneypenny governance lives in two places:

1. **`agents/_global.yaml`** — repo-wide defaults applied to every agent
2. **Agent `.md` files** — per-agent overrides in YAML frontmatter

There is no separate `policies/` directory. Everything lives in `.mp/agents/`.

## Quick permissions (flat keys)

The most common use case — restricting paths and tools — uses flat,
human-readable keys. These work in both `_global.yaml` and agent
frontmatter.

| Key | Type | Behavior |
|-----|------|----------|
| `deny_paths` | glob list | Block read/write to matching paths |
| `deny_tools` | glob list | Block matching tool names |
| `allow_paths` | glob list | If set, ONLY these paths are accessible (allowlist mode) |

### Example: `_global.yaml`

```yaml
# .mp/agents/_global.yaml

deny_paths:
  - "**/.git/**"
  - "**/node_modules/**"

exclude_patterns:
  - "**/node_modules/**"
  - "**/.git/**"
  - "**/dist/**"
  - "**/*.lock"

max_turns: 64
```

### Example: agent frontmatter

```yaml
# in .mp/agents/security-reviewer.md
---
name: security-reviewer
deny_paths:
  - "**/.env*"
  - "**/secrets/**"
deny_tools:
  - "run_terminal_cmd"
---
```

### Composition

`_global.yaml` loads first. Agent-level `deny_paths`, `deny_tools`, and
`allow_paths` merge **additively** on top. Scalar settings like `model`,
`tools`, and `max_turns` **override** the global default.

At load time, flat keys are parsed into internal `Permission[]` types:

```
deny_paths: ["**/.git/**"] → [{ type: "path_deny", pattern: "**/.git/**" }]
deny_tools: ["rm_cmd"]     → [{ type: "tool_deny", pattern: "rm_cmd" }]
allow_paths: ["src/**"]    → [{ type: "path_allow", pattern: "src/**" }]
```

## Advanced policies

For rules beyond simple deny/allow — cost caps, audit trails, confirmation
prompts, regex argument matching, priority-based resolution — use the
`policies` key. This works in both `_global.yaml` and agent frontmatter.

### Policy fields

| Field | Required | Type | Description |
|-------|----------|------|-------------|
| `name` | yes | string | Human-readable policy name |
| `effect` | yes | `allow` \| `deny` \| `audit` \| `confirm` | What happens when the policy matches |
| `priority` | no | integer | Higher wins when multiple policies match (default 0) |
| `tool` | no | glob | Match against tool name (e.g. `bash`, `file_write`, `mcp__github__*`) |
| `path` | no | glob | Match against file path arguments (e.g. `*.env*`, `src/secret/**`) |
| `cost` | no | expression | Match cost thresholds: `session_cost > 5.00` or `turn_cost > 1.00` |
| `args` | no | regex | Match against serialized tool arguments |
| `actor` | no | glob | Match against actor identity |
| `message` | no | string | Reason shown to the agent when the policy triggers |
| `enabled` | no | boolean | Disable without deleting (default true) |

### Effects

- **`deny`** — block the action; the agent receives the `message` and adapts
- **`allow`** — explicitly permit (useful to override a broader deny at lower priority)
- **`audit`** — permit but log to the governance event trail
- **`confirm`** — pause and ask for human confirmation before proceeding

### Example: advanced policies in `_global.yaml`

```yaml
# .mp/agents/_global.yaml

deny_paths:
  - "**/.git/**"
  - "**/node_modules/**"

policies:
  - name: no-force-push
    effect: deny
    tool: bash
    args: "push.*--force|push.*-f"
    message: Block force-push to any remote

  - name: no-rm-rf
    effect: deny
    tool: bash
    args: "rm\\s+-rf\\s+/"
    message: Block recursive delete from root

  - name: audit-all-bash
    effect: audit
    tool: bash
    message: Log all shell commands for review

  - name: session-cost-limit
    effect: deny
    cost: "session_cost > 10.00"
    message: Session cost cap reached
```

### Example: agent-specific policies

```yaml
# in .mp/agents/deployer.md
---
name: deployer
policies:
  - name: confirm-deploys
    effect: confirm
    tool: bash
    args: "deploy|publish|release"
    message: Deployment commands require confirmation

  - name: allow-test-commands
    effect: allow
    tool: bash
    args: "npm test|bun test|pytest|jest"
    priority: 10
    message: Test commands are always allowed
---
```

## Evaluation

Policies are evaluated on every tool call. The first matching policy wins
(ordered by `priority` DESC). If no policy matches, the action is allowed.

A policy matches when ALL specified fields match:
- `tool` glob matches the tool name
- `path` glob matches any path argument
- `cost` condition is true
- `args` regex matches serialized arguments
- `actor` glob matches the actor identity

Unspecified fields match everything.

Flat permission keys (`deny_paths`, `deny_tools`, `allow_paths`) are
evaluated **before** advanced policies, as a fast path.

## Common patterns

### Read-only agent

```yaml
# .mp/agents/reviewer.md
---
name: reviewer
description: Read-only code reviewer
deny_tools:
  - "write_file"
  - "run_terminal_cmd"
allow_paths:
  - "src/**"
  - "tests/**"
---

You are a read-only code reviewer...
```

### Cost caps

```yaml
# in _global.yaml policies:
policies:
  - name: session-cost-limit
    effect: deny
    cost: "session_cost > 5.00"
    message: Session cost cap reached

  - name: turn-cost-warning
    effect: confirm
    cost: "turn_cost > 1.00"
    message: This turn is expensive — confirm to proceed
```

### Protect specific directories

```yaml
# in _global.yaml
deny_paths:
  - "**/.git/**"

policies:
  - name: protect-infrastructure
    effect: deny
    path: "terraform/**"
    message: Infrastructure files are managed by Terraform

  - name: protect-migrations
    effect: confirm
    path: "db/migrations/**"
    message: Modifying migrations requires confirmation
```

### Allow override (higher priority)

```yaml
policies:
  - name: allow-test-bash
    effect: allow
    tool: bash
    args: "npm test|bun test|pytest|jest"
    priority: 10
    message: Test commands are always allowed
```

This overrides a lower-priority `deny` on `bash` because `priority: 10`
is evaluated before `priority: 0`.

### Audit everything for a specific tool

```yaml
policies:
  - name: audit-git-operations
    effect: audit
    tool: bash
    args: "^git "
    message: Log all git operations
```

## Debugging policies

### List active policies

```bash
mp policy list
```

### Sync policy files manually

```bash
mp policy sync
```

### Check which policy blocks an action

When a tool call is denied, the agent receives the policy name and message.
Check the governance event log:

```bash
mp events --type policy
```

## Directory structure

```
.mp/
├── mp.db                      # single DB: sessions, skills, metrics
├── workspace.sqlite           # search index
└── agents/
    ├── _global.yaml           # repo-wide: deny_paths, policies, exclude_patterns
    ├── default.md             # default agent definition
    ├── security-reviewer.md   # specialized agent with extra deny rules
    └── deployer.md            # agent with confirmation policies
```
