# Ideas

An **Idea** is a markdown file with YAML frontmatter representing a backlog item: a thought, a half-formed plan, a feature request, a bug. Ideas are filesystem-native — they're meant to be authored in your editor and version-controlled (or not) at your discretion.

Ideas are the primary artifact in the loop:

```
rough thought
  → idea (status: raw)
    → spec-creator agent (HITL) flesh it out
      → idea (status: spec-complete)
        → implementer agent launches a session against it
          → session links back via session.idea_id
            → on completion, idea status updates to 'done'
```

The loop is opt-in at every step. Nothing requires an idea; you can also just open a new session and chat.

## Location

```
~/.moneypenny/ideas/             # global
└── 2026-05-17-auth-redesign.md
└── api-versioning.md
└── ...
```

Repo-local ideas can also exist (filtered by cwd in the UI):

```
<repo>/.moneypenny/ideas/
└── feature-x.md
```

Filename convention: kebab-case. Optional date prefix `YYYY-MM-DD-` is honored for sorting but not required. The filename (without `.md`) is the idea's stable id.

## Format

```markdown
---
title: Auth Redesign
status: raw            # raw | spec-complete | in-progress | done | blocked | abandoned
priority: high         # low | medium | high (optional)
tags: [auth, security]
spec_session_id: null  # session id that produced the spec, if any
impl_session_ids: []   # session ids that implemented (or are implementing)
created_at: 2026-05-17
updated_at: 2026-05-17
links:
  - type: session
    id: abc-123
    note: "spec drafting"
  - type: idea
    id: jwt-rotation
    note: "depends on this"
custom_field: "anything you want"
---

# Auth Redesign

The current session-based auth is causing scaling issues. Move to JWT
with refresh tokens.

## Why
- Session table is hot
- ...

## Notes
- Look at how Linear does it
- Watch out for token leakage in URL params
```

The body is free-form markdown. The frontmatter has known keys (below) plus arbitrary user-defined keys (preserved on read/write).

## Frontmatter Schema (known keys)

```yaml
title: string                   # required; falls back to filename if missing
status: string                  # required; defaults to 'raw'; arbitrary string allowed
priority: string                # optional
tags: string[]                  # optional
spec_session_id: string | null  # session id (if a session produced its spec)
impl_session_ids: string[]      # sessions that implemented
created_at: string (date)
updated_at: string (date)
links: Array<{ type, id, note? }>  # cross-references
```

Status is freeform — the UI shows known statuses with badges and treats unknown values as plain strings. Suggested vocabulary:

| Status | Meaning |
|--------|---------|
| `raw` | Just an idea; not specced |
| `spec-complete` | Has a spec ready for implementation |
| `in-progress` | Implementation session(s) running |
| `done` | Completed |
| `blocked` | Stuck on dependency |
| `abandoned` | Decided not to pursue |

## API

See `04-api.md` for endpoint signatures. Summary:

| Endpoint | Behavior |
|----------|----------|
| `GET /ideas` | List all parsed ideas (global + repo-local merged) |
| `GET /ideas/:filename` | Read one |
| `POST /ideas` | Create new file `~/.moneypenny/ideas/<filename>.md` |
| `PATCH /ideas/:filename` | Update body and/or frontmatter (preserves unknown keys) |
| `DELETE /ideas/:filename` | Delete file |

The server uses a YAML library that preserves field ordering and comments where possible.

## Registry & Hot-Reload

```typescript
class IdeaRegistry {
  private items = new Map<string, Idea>();

  start() { /* watch ~/.moneypenny/ideas/ + repo-local */ }
  list(filter?: { status?: string; tag?: string; cwd?: string }): Idea[] { /* ... */ }
  get(filename: string): Idea | undefined { /* ... */ }
  upsert(filename: string, body: string, frontmatter: object): Promise<Idea> { /* writes file */ }
  delete(filename: string): Promise<void> { /* deletes file */ }
}
```

The registry is filesystem-watched; external edits to idea files (in your editor) are picked up and emitted as events.

## Linking Ideas to Sessions

Two directions of link:

1. **Idea → Session(s)**: stored in idea frontmatter (`spec_session_id`, `impl_session_ids`, `links`). Authored by either the user or by agents (via tool calls).

2. **Session → Idea**: stored on `sessions.idea_id` column. Set at session creation when the session is launched from an idea.

Lifecycle wiring:
- When an idea is opened in the UI and "Launch session from idea" is clicked, the dialog auto-fills the task with the idea body and sets `idea_id` on the session.
- When a `spec-creator` blueprint completes, it can call `update_idea` (a tool) to set `status: spec-complete` and `spec_session_id: <self>`.
- When an `implementer` session reaches `completed`, the custodian can append the session id to `impl_session_ids` (subject to convention; not enforced).

## UI Integration

The Ideas tab uses TanStack Table:

| Column | Source |
|--------|--------|
| Title | frontmatter.title |
| Status | frontmatter.status |
| Priority | frontmatter.priority |
| Tags | frontmatter.tags |
| Linked sessions | impl_session_ids count + spec_session_id presence |
| Updated | frontmatter.updated_at |
| Path | filename (small, mono) |

Row click opens a side drawer with rendered markdown body and frontmatter editor.

A "New Idea" button creates a new file with a template:

```yaml
---
title:
status: raw
created_at: <today>
updated_at: <today>
---

# <title>

```

The user is dropped into the markdown editor.

## Search

Ideas participate in unified search via filesystem grep. Not indexed in SQLite. The volume is small (dozens to hundreds of files); a `rg`-style scan is sufficient.

## Out of Scope (v2)

- Idea dependency graph visualization
- Idea status workflow rules (allowed transitions)
- Bulk operations (batch tag, batch status)
- Templates beyond the default new-idea
