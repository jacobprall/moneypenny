import type { Database } from "bun:sqlite";
import { llm, llmJson } from "./llm.js";

export interface CustodianConfig {
  model?: string;
  pointerCap: number;
  compactAfterTurns: number;
  archiveAfterDays: number;
  purgeAfterDays: number;
  chunkPruneAfterDays: number;
}

export interface CustodianResult {
  labeled: number;
  compacted: number;
  archived: number;
  purged: number;
  summarized: number;
  consolidated: number;
  chunksP: number;
  durationMs: number;
}

const DEFAULT_CONFIG: CustodianConfig = {
  pointerCap: 20,
  compactAfterTurns: 50,
  archiveAfterDays: 30,
  purgeAfterDays: 90,
  chunkPruneAfterDays: 14,
};

export async function runCustodian(
  db: Database,
  config: Partial<CustodianConfig> = {},
): Promise<CustodianResult> {
  const cfg = { ...DEFAULT_CONFIG, ...config };
  const start = performance.now();
  const result: CustodianResult = {
    labeled: 0, compacted: 0, archived: 0,
    purged: 0, summarized: 0, consolidated: 0,
    chunksP: 0, durationMs: 0,
  };

  result.labeled = await labelUnlabeled(db, cfg.model);
  result.compacted = await compactLongSessions(db, cfg.compactAfterTurns, cfg.model);
  result.archived = archiveStaleSessions(db, cfg.archiveAfterDays);
  result.purged = await purgeArchivedSessions(db, cfg.purgeAfterDays);
  result.summarized = await summarizeUnsummarized(db, cfg.model);
  result.consolidated = await consolidatePointers(db, cfg.pointerCap, cfg.model);
  result.chunksP = pruneStaleChunks(db, cfg.chunkPruneAfterDays);

  result.durationMs = performance.now() - start;

  db.query(
    `INSERT INTO events (type, detail, created_at)
     VALUES ('custodian.run', ?, unixepoch())`,
  ).run(JSON.stringify(result));

  return result;
}

async function labelUnlabeled(db: Database, model?: string): Promise<number> {
  const sessions = db
    .query<{ id: string }, []>(
      `SELECT id FROM sessions WHERE label IS NULL AND is_active = 0
       ORDER BY created_at DESC LIMIT 10`,
    )
    .all();

  let count = 0;
  for (const session of sessions) {
    const messages = db
      .query<{ content: string }, [string]>(
        `SELECT content FROM messages WHERE session_id = ? AND content IS NOT NULL
         ORDER BY turn ASC LIMIT 6`,
      )
      .all(session.id);

    if (messages.length === 0) continue;

    const transcript = messages.map((m: { content: string }) => m.content).join("\n").slice(0, 2000);
    const label = await llm(
      "local",
      `Label this conversation in 3-5 words. Reply with ONLY the label.\n\n${transcript}`,
      { model },
    );

    db.query("UPDATE sessions SET label = ? WHERE id = ?").run(label.trim().slice(0, 100), session.id);
    count++;
  }
  return count;
}

async function compactLongSessions(
  db: Database,
  threshold: number,
  model?: string,
): Promise<number> {
  const sessions = db
    .query<{ id: string; cnt: number }, [number]>(
      `SELECT s.id, COUNT(*) as cnt FROM sessions s
       JOIN messages m ON m.session_id = s.id
       WHERE s.is_active = 0
       GROUP BY s.id HAVING cnt > ? LIMIT 5`,
    )
    .all(threshold);

  let count = 0;
  for (const session of sessions) {
    const messages = db
      .query<{ role: string; content: string }, [string]>(
        `SELECT role, content FROM messages WHERE session_id = ? AND content IS NOT NULL
         ORDER BY turn ASC`,
      )
      .all(session.id);

    const transcript = messages
      .map((m: { role: string; content: string }) => `${m.role}: ${m.content}`)
      .join("\n")
      .slice(0, 12000);

    const summary = await llm(
      "local",
      `Compress this conversation into a concise summary preserving all key decisions, code changes, and outcomes. Max 500 words.\n\n${transcript}`,
      { model },
    );

    const oldCount = messages.length;
    const keepCount = 6;
    if (oldCount > keepCount) {
      db.query(
        `DELETE FROM messages WHERE session_id = ? AND turn < ?`,
      ).run(session.id, oldCount - keepCount);

      db.query(
        `INSERT INTO messages (id, turn, role, content, session_id, created_at)
         VALUES (?, 0, 'system', ?, ?, unixepoch())`,
      ).run(
        crypto.randomUUID(),
        `[Compacted from ${oldCount} messages]\n\n${summary}`,
        session.id,
      );
    }
    count++;
  }
  return count;
}

function archiveStaleSessions(db: Database, days: number): number {
  const result = db.prepare(
    `UPDATE sessions SET is_active = 0
     WHERE is_active = 1 AND last_active_at < unixepoch() - ? * 86400`,
  ).run(days);
  return (result as any).changes ?? 0;
}

async function purgeArchivedSessions(db: Database, days: number): Promise<number> {
  const stale = db
    .query<{ id: string }, [number]>(
      `SELECT id FROM sessions
       WHERE is_active = 0 AND last_active_at < unixepoch() - ? * 86400
       LIMIT 10`,
    )
    .all(days);

  for (const session of stale) {
    const hasPointer = db
      .query<{ id: string }, [string]>(
        "SELECT id FROM session_pointers WHERE session_id = ?",
      )
      .get(session.id);
    if (!hasPointer) {
      db.query("INSERT INTO work_queue (type, session_id, created_at) VALUES ('summarize', ?, unixepoch())")
        .run(session.id);
    }
  }

  return stale.length;
}

async function summarizeUnsummarized(db: Database, model?: string): Promise<number> {
  const sessions = db
    .query<{ id: string }, []>(
      `SELECT s.id FROM sessions s
       WHERE s.is_active = 0
       AND NOT EXISTS (SELECT 1 FROM session_pointers WHERE session_id = s.id)
       AND EXISTS (SELECT 1 FROM messages WHERE session_id = s.id AND content IS NOT NULL)
       ORDER BY s.created_at DESC LIMIT 5`,
    )
    .all();

  let count = 0;
  for (const session of sessions) {
    const messages = db
      .query<{ role: string; content: string }, [string]>(
        `SELECT role, content FROM messages WHERE session_id = ? AND content IS NOT NULL
         ORDER BY turn ASC`,
      )
      .all(session.id);

    if (messages.length === 0) continue;

    const transcript = messages
      .map((m: { role: string; content: string }) => `${m.role}: ${m.content}`)
      .join("\n")
      .slice(0, 8000);

    const summary = await llm(
      "fast",
      `Summarize this developer conversation. Focus on decisions, problems solved, outcomes. 2-4 sentences.\n\n${transcript}`,
      { model },
    );

    const pointerData = await llmJson<{ key: string; phrase: string }>(
      "local",
      `Given this summary, generate a kebab-case key (3-5 words) and a short phrase (under 10 words).\n\nSummary: ${summary}\n\nRespond JSON only: {"key": "example-key", "phrase": "what happened"}`,
      { model },
    );

    const key = sanitizeKey(pointerData?.key ?? session.id.slice(0, 20));
    const phrase = String(pointerData?.phrase ?? summary.slice(0, 100)).slice(0, 200);

    db.query(
      `INSERT INTO session_pointers (id, session_id, key, phrase, summary, created_at)
       VALUES (?, ?, ?, ?, ?, unixepoch())`,
    ).run(crypto.randomUUID(), session.id, key, phrase, summary.trim());

    count++;
  }
  return count;
}

async function consolidatePointers(
  db: Database,
  cap: number,
  model?: string,
): Promise<number> {
  const activeCount = db
    .query<{ cnt: number }, []>(
      "SELECT COUNT(*) as cnt FROM session_pointers WHERE archived = 0 AND pinned = 0",
    )
    .get()?.cnt ?? 0;

  if (activeCount <= cap) return 0;

  type Pointer = { id: string; session_id: string; key: string; phrase: string; summary: string | null };
  const oldest = db
    .query<Pointer, [number]>(
      `SELECT id, session_id, key, phrase, summary FROM session_pointers
       WHERE archived = 0 AND pinned = 0
       ORDER BY created_at ASC LIMIT ?`,
    )
    .all(Math.min(5, activeCount - cap + 2));

  if (oldest.length < 2) return 0;

  const descriptions = oldest
    .map((p: Pointer) => `- ${p.key}: ${p.phrase}${p.summary ? ` (${p.summary})` : ""}`)
    .join("\n");

  const merged = await llmJson<{ key: string; phrase: string; summary: string }>(
    "fast",
    `Merge these session pointers into ONE consolidated pointer.\n\nPointers:\n${descriptions}\n\nJSON only: {"key": "merged-key", "phrase": "phrase", "summary": "merged summary"}`,
    { model },
  );

  if (!merged) return 0;

  try {
    db.transaction(() => {
      db.query(
        `INSERT INTO session_pointers (id, session_id, key, phrase, summary, created_at, consolidated_from)
         VALUES (?, ?, ?, ?, ?, unixepoch(), ?)`,
      ).run(
        crypto.randomUUID(),
        oldest[oldest.length - 1].session_id,
        sanitizeKey(merged.key),
        String(merged.phrase).slice(0, 200),
        merged.summary,
        JSON.stringify(oldest.map((p: Pointer) => p.session_id)),
      );

      for (const p of oldest) {
        db.query("UPDATE session_pointers SET archived = 1 WHERE id = ?").run(p.id);
      }
    })();

    return oldest.length;
  } catch {
    return 0;
  }
}

function pruneStaleChunks(db: Database, days: number): number {
  const result = db.prepare(
    `DELETE FROM code_chunks WHERE updated_at < unixepoch() - ? * 86400`,
  ).run(days);
  return (result as any).changes ?? 0;
}

function sanitizeKey(raw: string): string {
  return raw
    .toLowerCase()
    .replace(/[^a-z0-9-]/g, "-")
    .replace(/-+/g, "-")
    .replace(/^-|-$/g, "")
    .slice(0, 50);
}
