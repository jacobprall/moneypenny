# Session Lifecycle

### Problem

Long sessions accumulate hundreds of messages. Loading them into context
windows wastes tokens on verbose tool call/result pairs. Over months of
use, the database grows unboundedly — a solo developer with 500 sessions
and 50K messages has a bloated `mp.db` that degrades read performance
even for recent data.

The session lifecycle solves this with a three-stage pipeline:
**compact → embed → archive/purge**. Sessions graduate from hot (full
messages) to warm (embedded summary in SQLite) to cold (JSONL archive
on disk), keeping the database lean while preserving searchable
intelligence.

### Three storage tiers

```
HOT (full messages in SQLite)
  Active sessions, recent history
  Agent loads: summaries + recent messages
    │
    ├── Trigger: idle > 10min AND messages > 40
    ▼
WARM (summary + embedding in SQLite, raw messages still present)
  Compacted sessions with embedded summaries
  Searchable via unified query session surface
  Raw messages available but not loaded into context
    │
    ├── Trigger: age > N days OR db size > M MB
    ▼
COLD (JSONL archive on disk, raw messages purged from SQLite)
  Session metadata + embedded summary remain in DB
  Raw messages archived to .mp/archives/<session-id>.jsonl.gz
  Still searchable via embedded summary
  Recoverable via mp sessions restore
```

### Stage 1: Compact (summarize)

```typescript
export interface CompactionConfig {
  triggerThreshold: number;     // messages before compaction triggers (default 40)
  keepRecent: number;           // messages to keep uncompacted (default 10)
  model: string;                // model for summary generation
  maxSummaryTokens: number;     // token budget for summary (default 2000)
}
```

Compaction uses **claude-3-5-haiku** (cheapest Anthropic model). At ~$0.25
per million input tokens, a 50-message session (~20K tokens) costs ~$0.005
to compact. The custodian (§6) has a $0.05 budget, so it can compact ~10
sessions per run.

**Context window handling:** If the session exceeds the model's context
window (200K for Haiku), split messages into context-window-sized chunks,
compact each independently, and store multiple `compaction_markers`.

#### Compaction flow

```
Session has 60 messages (turns 1..60)
  │
  ├── keepRecent = 10 → keep messages 51..60 intact
  │
  ├── Compact messages 1..50:
  │   1. Group by turns (user + assistant + tool calls)
  │   2. Send to LLM with structured summary prompt
  │   3. Store summary in compaction_markers table
  │
  └── When loading session for context:
      1. Load compaction_markers for session (ordered by up_to_turn)
      2. Load messages after latest marker's up_to_turn
      3. Assemble: [system prompt] + [compacted summaries] + [recent messages]
```

#### Summary prompt

```
You are summarizing a coding conversation. Preserve:
- All decisions made and their rationale
- Files created, modified, or deleted (with paths)
- Errors encountered and how they were resolved
- Key code patterns or approaches chosen
- Tool calls that produced important results (not routine file reads)
- Any commitments or TODOs mentioned

Omit:
- Verbose tool call arguments and raw output
- Routine file reads that didn't change the approach
- Redundant back-and-forth on resolved issues

Use this structured format:
## Decisions
- ...
## Changes
- file: path/to/file — created/modified/deleted — brief description
- ...
## Issues & Resolutions
- ...
## Open Items
- ...
```

### Stage 2: Embed (make summary searchable)

After compaction, the structured summary is embedded as a vector and
stored in `session_summaries`:

```typescript
export interface SessionSummary {
  id: string;
  sessionId: string;
  summary: string;              // the compacted text
  embedding: Float32Array | null; // vector embedding of summary
  createdAt: number;
}
```

```sql
CREATE TABLE session_summaries (
  id TEXT PRIMARY KEY NOT NULL,
  session_id TEXT NOT NULL REFERENCES sessions(id),
  summary TEXT NOT NULL,
  embedding BLOB,                   -- vector from embedChunks()
  created_at INTEGER NOT NULL DEFAULT (unixepoch()),
  UNIQUE(session_id)
);
```

The embedding uses the same `embedChunks` function from §1 (embedding
pipeline). This is what makes the unified query engine's `session`
surface work — it searches embedded summaries, not raw messages.

**Embedding happens immediately after compaction** as part of the same
lifecycle step. If the embedding extension is unavailable, the summary
is still stored (FTS5 search works), but vector search won't find it.

### Stage 3: Archive and purge

After a configurable hold period, raw messages are archived to JSONL
and purged from SQLite:

```typescript
export interface ArchiveConfig {
  archiveAfterDays: number;       // days after compaction before archival (default 30)
  archivePath: string;            // default: ".mp/archives/"
  archiveFormat: "jsonl" | "jsonl.gz";  // default: "jsonl.gz"
  purgeAfterArchiveDays: number;  // days after archive before purge (default 7)
  maxDbSizeMb: number;            // trigger early archival if DB exceeds this (default 500)
}
```

#### Archive flow

```
1. Select eligible sessions:
   - Has compaction_markers
   - compaction_markers.created_at < now - archiveAfterDays
   - archived_at IS NULL
   - NOT the active session
   - NOT a _custodian session

2. For each eligible session:
   a. Write all messages to JSONL:
      .mp/archives/<session-id>.jsonl.gz
      Each line: { "role": "...", "content": "...", "created_at": ..., "turn": ... }

   b. Compute SHA-256 checksum of the archive file

   c. Verify: re-read archive, count lines, compare to message count

   d. Update session:
      UPDATE sessions SET
        archived_at = unixepoch(),
        archive_path = '.mp/archives/<session-id>.jsonl.gz',
        archive_checksum = '<sha256>'
      WHERE id = '<session-id>'

   e. After purge hold period (purgeAfterArchiveDays):
      DELETE FROM messages WHERE session_id = '<session-id>'
      DELETE FROM events WHERE session_id = '<session-id>'
```

#### DB size trigger

In addition to age-based archival, the custodian checks the database
file size. If `mp.db` exceeds `maxDbSizeMb`, it archives the oldest
warm sessions (those with compaction markers) regardless of age:

```typescript
function shouldTriggerSizeArchival(dbPath: string, config: ArchiveConfig): boolean {
  const stats = Bun.file(dbPath).size;
  return stats / (1024 * 1024) > config.maxDbSizeMb;
}
```

#### Safety guarantees

| Concern | Mitigation |
|---------|-----------|
| Archive file corrupt / incomplete | Checksum verification before marking archived |
| Premature purge | `purgeAfterArchiveDays` hold period (default 7 days) |
| Need to recover old messages | `mp sessions restore <id>` re-imports from JSONL |
| Compacting while session is active | Only compact sessions with no activity for 10+ minutes |
| Archiving active session | Exclude sessions with activity in last 24 hours |
| Disk full during archive | Catch write errors, skip session, report in custodian log |

### Restore command

```bash
# Restore a single archived session
mp sessions restore <session-id>

# List archived sessions
mp sessions list --archived

# Export a session to JSONL (manual, works on any session)
mp sessions export <session-id> --format jsonl --output ./my-session.jsonl
```

`mp sessions restore` reads the JSONL archive and re-inserts messages
into the database. The session moves from cold back to warm (compaction
markers and summary remain). This is idempotent — if messages already
exist, they're skipped.

### Acceptance criteria

- [ ] Sessions with 50+ messages trigger compaction after 10 min idle
- [ ] Compacted summary fits within 2000 tokens
- [ ] Summary is embedded as a vector immediately after compaction
- [ ] Embedded summaries are searchable via the session surface in unified query
- [ ] Loading a compacted session assembles: summaries + recent messages
- [ ] Sessions older than `archiveAfterDays` are archived to JSONL
- [ ] Archive files have SHA-256 checksums verified before marking archived
- [ ] Messages are purged only after `purgeAfterArchiveDays` hold period
- [ ] DB size exceeding `maxDbSizeMb` triggers early archival of oldest warm sessions
- [ ] `mp sessions restore <id>` re-imports messages from JSONL archive
- [ ] `mp sessions export <id>` works on any session (not just archived)
- [ ] `mp sessions list --archived` shows archived sessions with archive paths
- [ ] Compaction cost < $0.01 per session (Haiku pricing)
- [ ] `_custodian` sessions are excluded from archival (self-exclusion)

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 5.1 | `CompactionConfig`, `ArchiveConfig` types, `session_summaries` + schema additions | 1 day |
| 5.2 | Summary generation: prompt, LLM call, structured extraction | 2 days |
| 5.3 | Summary embedding (reuse `embedChunks` from §1) | 0.5 days |
| 5.4 | Automatic compaction trigger (threshold check, idle detection) | 1 day |
| 5.5 | Context assembly integration (load markers + recent messages) | 1 day |
| 5.6 | JSONL archive writer with checksum verification | 1.5 days |
| 5.7 | Message purge with hold period and DB size trigger | 1 day |
| 5.8 | `mp sessions restore`, `mp sessions export`, `mp sessions list --archived` | 1.5 days |
| 5.9 | Manual trigger via `context_curate.summarize_session` and `context_curate.archive_session` | 0.5 days |
