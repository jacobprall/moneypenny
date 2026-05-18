# Blueprints

A **Blueprint** is a markdown file with YAML frontmatter that defines an agent's prompt, tools, permissions, and strategy. Blueprints live on the filesystem and are read at session-creation time. The file is the source of truth.

## Location

```
~/.moneypenny/blueprints/         # global
└── default.md
└── spec-creator.md
└── implementer.md
└── reviewer.md
└── custodian.md

<repo>/.moneypenny/blueprints/    # repo-local (overrides global)
└── domain-specific.md
```

## Resolution

```typescript
function resolveBlueprint(name: string, cwd?: string): Blueprint
```

Order:
1. Repo-local: `{cwd}/.moneypenny/blueprints/{name}.md` (if cwd given)
2. Global: `~/.moneypenny/blueprints/{name}.md`
3. `BlueprintNotFoundError`

The resolved blueprint is **snapshotted** into `session.config` at creation. Editing the file does NOT affect existing sessions; only new sessions pick up changes.

A `BlueprintRegistry` watches the directories with chokidar and maintains an in-memory cache. Resolution reads the cache, not disk, on every call.

## Format

```markdown
---
name: spec-creator
model: claude-sonnet-4-20250514
tools:
  - search_code
  - read_file
  - write_file
  - request_human_input
permissions:
  filesystem: readwrite
  network: false
  shell: false
strategy: hitl
pause_after:
  - draft_complete
  - design_complete
max_turns: 50
context:
  conventions: true
  skills: ['spec_writing']
trigger_on: manual
---

You are a specification writer. Given an idea, produce a detailed
technical spec ready for implementation.

## Behavior
- Ask clarifying questions before writing
- Structure: Overview, Requirements, API Design, Data Model, Edge Cases
- Reference existing code patterns when relevant (use search_code)
- Pause after drafting and after design (declared via pause_after)

## Output
Write the spec as markdown. When complete, save to ideas with
status: spec-complete in frontmatter.
```

## Frontmatter Schema

```yaml
name: string              # identifier; must be unique within scope; matches filename
model: string             # optional override
tools: string[] | null    # whitelist; null = all permitted by permissions
permissions:              # see Permissions section
  filesystem: read | readwrite
  network: boolean
  shell: boolean
strategy: autonomous | hitl | review   # default: autonomous
pause_after: string[]     # named checkpoints (see HITL Mechanism)
max_turns: number         # default: 50; behavior at limit defined below
context:
  conventions: boolean    # auto-load detected conventions into system prompt (default true)
  skills: string[]        # always-load skills by name
trigger_on: manual | session_close | schedule | file_change   # default: manual
schedule: string          # cron expression, required if trigger_on=schedule
file_glob: string[]       # required if trigger_on=file_change
```

Validation runs on registry load. Invalid frontmatter → blueprint excluded from registry, error emitted to events table.

## HITL Mechanism

Two complementary signals — **declarative** and **explicit** — both active simultaneously:

### Declarative: `pause_after` checkpoints

Blueprint frontmatter declares named checkpoints. The agent emits a checkpoint by writing a special line in its response:

```
[[checkpoint: draft_complete]]
```

Runtime parser detects the marker, sets session status to `paused`, emits `status` event. User must inject a message to resume. Checkpoint name is also recorded as the pause reason.

If a checkpoint name is not in `pause_after`, the marker is treated as a no-op.

### Explicit: `request_human_input` tool

Always available regardless of frontmatter (subject to tool whitelist if specified). Lets the agent pause ad-hoc:

```typescript
{
  name: 'request_human_input',
  description: 'Pause the session and request guidance from the user',
  parameters: {
    reason: { type: 'string' },
    options: { type: 'array', items: { type: 'string' } }  // optional preset replies
  },
  effect: 'transition session to paused, emit status event'
}
```

The UI surfaces `reason` and (if present) `options` as quick-reply buttons.

## `strategy: review`

Runs to completion autonomously, then transitions to `paused` instead of `active`/`completed` and waits for user approval. The "review" mode adds an implicit final checkpoint after the last run.

## `max_turns` Behavior

When a session's run count for a given launch exceeds `max_turns`:
- session transitions to `paused` with reason `max_turns_exceeded`
- user can inject "continue" (raises a per-session override of `max_turns + 50`) or archive

## Permission Inheritance

When a session spawns a child via `spawn_agent`:

```
child.permissions = intersect(parent.permissions, requested.permissions)
```

A child can never have permissions broader than its parent. Attempting to spawn with broader permissions raises a `permission_inheritance_error` and the spawn fails.

The same rule applies to `tools`: child's effective tool set is `intersect(parent.tools, requested.tools)` (with `null` treated as "all permitted").

> **Note**: Permission granularity (allowlists, path globs, command lists) is deferred. v2 ships with the coarse boolean/enum model. Revisit when actual misuse cases emerge.

## Sub-Agent Invocation

The `spawn_agent` tool is the canonical way to launch a child session. See `07-tools.md` for the registration and `01-sessions.md` for parent/child semantics.

## Default Blueprint

A `default.md` is shipped with the package and copied to `~/.moneypenny/blueprints/default.md` on first run if absent. If the user deletes it, sessions created without an explicit blueprint use a hard-coded fallback identical to the shipped default.

## Validation Rules

| Rule | Effect on failure |
|------|-------------------|
| Frontmatter parses as YAML | Excluded from registry, error event |
| `name` is non-empty string | Excluded |
| `name` matches filename (`name.md`) | Warning, name from frontmatter wins |
| `name` is unique within scope | Repo-local wins; global excluded with warning |
| `tools` references known tools | Unknown tools dropped with warning |
| `model` is in supported provider list | Warning, will fail at runtime |
| `schedule` is valid cron (if present) | Excluded |
| `file_glob` is valid (if present) | Excluded |
