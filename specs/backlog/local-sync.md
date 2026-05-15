# Backlog: Local SQLite sync (peer / path-based)

Status: **not implemented** — sqlite-sync is loaded and cloud URL sync exists; file-to-file and named-peer sync are product direction only.

---

## Context

Today Moneypenny:

- Loads `@sqliteai/sqlite-sync` and tracks `siteId` per database.
- Initializes CRDT-backed tables via `@moneypenny/cloud` (`cloudsync_init` on selected tables).
- Runs replication only through **`cloudsync_network_init` + `cloudsync_network_sync`** against a configured cloud URL (`mp cloud init`, `mp cloud sync`).

Public docs describe broader sync (peers, `mp sync push/pull`, paths like `/path/to/other.db`). That UX is **not** wired in the CLI or packages yet. This note captures what “local sync” should mean, who it is for, and how we might build it.

---

## Problem

Users with **multiple workspaces** (different git repos, machines without cloud, air-gapped flows, or “copy my brain to a USB stick”) need **deterministic merge** of the same CRDT tables **without** going through SQLite Cloud.

Cloud sync solves multi-machine coordination when a URL is acceptable. Local sync fills the gap when:

- There is no cloud account or network.
- Two databases live on one machine and should exchange deltas directly.
- Teams want explicit “push to backup DB” or “pull from canonical team file” semantics.

---

## Scope

### In scope (intent)

- **Merge semantics** consistent with existing CRDT tables (same tables as cloud path: policies, sync_config, agents, skills — extend if schema adds more sync-eligible tables).
- **Explicit user actions**: push, pull, or bidirectional sync between **two concrete database files** (or workspace-resolved paths).
- **Safety**: dry-run or summary of pending changes where the extension/API allows; clear errors when schema versions or sync eligibility mismatch.
- **Documentation** aligned with actual CLI once shipped (replace or narrow aspirational doc copy until then).

### Out of scope (for v1 of this backlog item)

- Automatic discovery of peers on LAN (mDNS, etc.) — optional later.
- Syncing tables that are intentionally local-only (messages, scratch, raw external events) unless product explicitly changes.
- Replacing cloud sync; this is an **additional** transport.
- Non–sqlite-sync ad-hoc SQL diff/merge (we stay on extension merge rules).

---

## User scenarios

| # | Scenario | Success criteria |
|---|----------|------------------|
| 1 | **Two repos, two `.mp` agent DBs** — developer wants facts/skills/policies from project A available while working in project B. | User runs a documented command pointing at B’s DB from A’s repo (or vice versa); merges complete without corruption; site IDs remain distinct. |
| 2 | **Backup / fork** — copy accumulated knowledge to a read-only archive file periodically. | One-way push to a path; archive can be opened later and merged back if needed. |
| 3 | **Air-gapped handoff** — export sync payload to removable media, import on another machine. | Offline-friendly steps (file bundle or single blob) with documented apply order. |
| 4 | **CI or headless** — script merges a golden `team.db` into per-job DBs without cloud credentials. | Non-interactive CLI flags; exit codes for automation. |
| 5 | **Conflict transparency** — user wants to know “what changed” before apply. | Status or diff-style output tied to whatever sqlite-sync exposes (or minimal custom bookkeeping). |

---

## Early implementation thoughts

### 1. Discover sqlite-sync surface area

Before designing CLI flags, confirm what the **loaded extension** supports beyond `cloudsync_network_*`:

- File-based apply / import-export APIs (if any).
- Peer connection strings that are `file:` or local sockets.
- Required ordering: `cloudsync_init` on both sides, WAL checkpoint, etc.

**Action:** read upstream `@sqliteai/sqlite-sync` documentation or headers shipped with the platform package; spike in a throwaway script opening two `Database` handles.

### 2. Layering

- **`@moneypenny/cloud` (or rename conceptually):** add something like `runLocalSync(sourceDb, targetDb, direction)` or thin wrappers around extension calls — keep `runCloudSync` as the network transport.
- **CLI:** e.g. `mp sync push --to <path>`, `mp sync pull --from <path>`, `mp sync status` — only after the library API is stable; avoid duplicating SQL in the CLI.

### 3. Path resolution

- Resolve relative paths from cwd vs `--repo` consistently with other commands.
- Validate both files are agent DBs (or workspace DBs) with compatible schema versions before merge.

### 4. Configuration (optional)

`moneypenny.toml` could list **named peers** as paths for ergonomics:

```toml
[sync.peers]
research = "/Users/me/other-repo/.mp/agents/default.db"
```

Aliases reduce typing; the actual operation remains path-based.

### 5. Risks and mitigations

| Risk | Mitigation |
|------|------------|
| Extension has no first-class “file merge” API | Fall back to documented offline payload flow from vendor, or temporary local relay process. |
| Schema drift between files | Hard-stop with message; document upgrade path (`mp migrate` / single schema version). |
| WAL not flushed | Document `PRAGMA` / checkpoint or use extension’s recommended close sequence before copying files wholesale. |
| User copies `.db` instead of merge | Keep “copy file” in docs as non-merge backup; CLI names emphasize merge/replicate. |

---

## Open questions

1. Should local sync operate on **agent DB only**, **workspace DB only**, or both — and do we need separate commands?
2. Do we require **sqlite-sync** on both sides at identical versions, or does merge tolerate skew?
3. Should **MCP tools** mirror CLI (`sync_push`, `sync_pull`) for parity with cloud sync storytelling?
4. How does this interact with **encrypted or sealed** DBs (if introduced later)?

---

## References

- `moneypenny/packages/cloud/src/sync.ts` — current cloud-only sync wiring.
- `moneypenny/packages/db/src/database.ts` — extension load and `siteId`.
- `mp-site/docs/src/content/docs/concepts/sync.mdx` — aspirational UX; reconcile when this ships.
