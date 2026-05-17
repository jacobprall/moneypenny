import { generateText } from "ai";
import { anthropic } from "@ai-sdk/anthropic";
import type { Database } from "bun:sqlite";

interface WorkItem {
  id: number;
  type: string;
  session_id: string | null;
  payload: string | null;
}

export interface WorkerConfig {
  db: Database;
  model?: string;
  pointerCap: number;
  batchSize: number;
  embedFn?: (db: Database, batchSize: number) => Promise<number>;
  detectConventionsFn?: (db: Database, model: string) => Promise<number>;
  extractSkillsFn?: (db: Database, sessionId: string, model: string) => Promise<number>;
}

const DEFAULT_MODEL = "claude-sonnet-4-20250514";

async function llmGenerate(
  model: string,
  prompt: string,
  json?: boolean,
): Promise<string> {
  const { text } = await generateText({
    model: anthropic(model),
    prompt,
  });
  if (json) {
    const match = text.match(/\{[\s\S]*\}/);
    return match ? match[0] : text;
  }
  return text;
}

export async function processWorkQueue(
  config: WorkerConfig,
): Promise<number> {
  const { db, pointerCap, batchSize } = config;
  const model = config.model ?? DEFAULT_MODEL;

  const items = db
    .query<WorkItem, [number]>(
      `SELECT id, type, session_id, payload
       FROM work_queue
       WHERE processed_at IS NULL
       ORDER BY created_at ASC
       LIMIT ?`,
    )
    .all(batchSize);

  let processed = 0;

  for (const item of items) {
    try {
      switch (item.type) {
        case "label":
          await processLabel(db, model, item);
          break;
        case "summarize":
          await processSummarize(db, model, item);
          break;
        case "consolidate":
          await processConsolidate(db, model, item, pointerCap);
          break;
        case "embed":
          if (config.embedFn) {
            await config.embedFn(db, 20);
          }
          break;
        case "detect_conventions":
          if (config.detectConventionsFn) {
            await config.detectConventionsFn(db, model);
          }
          break;
        case "learn_skills":
          if (config.extractSkillsFn && item.session_id) {
            await config.extractSkillsFn(db, item.session_id, model);
          }
          break;
        default:
          throw new Error(`Unknown work type: ${item.type}`);
      }
      db.query(
        "UPDATE work_queue SET processed_at = unixepoch() WHERE id = ?",
      ).run(item.id);
      db.query(
        `INSERT INTO events (type, session_id, detail, created_at)
         VALUES ('worker.' || ?, ?, json_object('work_id', ?), unixepoch())`,
      ).run(item.type, item.session_id, item.id);
      processed++;
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      db.query("UPDATE work_queue SET error = ? WHERE id = ?").run(
        msg,
        item.id,
      );
    }
  }

  maybeQueueConsolidation(db, pointerCap);

  return processed;
}

async function processLabel(
  db: Database,
  model: string,
  item: WorkItem,
): Promise<void> {
  if (!item.session_id) return;

  const messages = db
    .query<{ content: string }, [string]>(
      `SELECT content FROM messages
       WHERE session_id = ? AND role IN ('user', 'assistant') AND content IS NOT NULL
       ORDER BY turn ASC LIMIT 10`,
    )
    .all(item.session_id);

  if (messages.length === 0) return;

  const transcript = messages.map((m) => m.content).join("\n");
  const label = await llmGenerate(
    model,
    `Label this conversation in 3-5 words. Reply with ONLY the label, nothing else.\n\n${transcript.slice(0, 2000)}`,
  );

  db.query("UPDATE sessions SET label = ? WHERE id = ?").run(
    label.trim().slice(0, 100),
    item.session_id,
  );
}

async function processSummarize(
  db: Database,
  model: string,
  item: WorkItem,
): Promise<void> {
  if (!item.session_id) return;

  const existing = db
    .query<{ id: string }, [string]>(
      "SELECT id FROM session_pointers WHERE session_id = ?",
    )
    .get(item.session_id);
  if (existing) return;

  const messages = db
    .query<{ role: string; content: string }, [string]>(
      `SELECT role, content FROM messages
       WHERE session_id = ? AND content IS NOT NULL
       ORDER BY turn ASC`,
    )
    .all(item.session_id);

  if (messages.length === 0) return;

  const transcript = messages
    .map((m) => `${m.role}: ${m.content}`)
    .join("\n")
    .slice(0, 8000);

  const summary = await llmGenerate(
    model,
    `Summarize this developer conversation. Focus on decisions made, problems solved, and key outcomes. Be concise (2-4 sentences).\n\n${transcript}`,
  );

  const pointerJson = await llmGenerate(
    model,
    `Given this conversation summary, generate a kebab-case key (3-5 words) and a short phrase (under 10 words) that captures the essence.\n\nSummary: ${summary}\n\nRespond with JSON only: {"key": "example-key", "phrase": "what happened"}`,
    true,
  );

  let key: string;
  let phrase: string;
  try {
    const parsed = JSON.parse(pointerJson);
    key = sanitizeKey(parsed.key);
    phrase = String(parsed.phrase).slice(0, 200);
  } catch {
    key = sanitizeKey(item.session_id.slice(0, 20));
    phrase = summary.slice(0, 100);
  }

  const id = crypto.randomUUID();
  db.query(
    `INSERT INTO session_pointers (id, session_id, key, phrase, summary, created_at)
     VALUES (?, ?, ?, ?, ?, unixepoch())`,
  ).run(id, item.session_id, key, phrase, summary.trim());

  db.query(
    "UPDATE sessions SET label = ? WHERE id = ? AND label IS NULL",
  ).run(phrase.slice(0, 100), item.session_id);

  // Queue skill extraction for this session
  db.query(
    "INSERT INTO work_queue (type, session_id, created_at) VALUES ('learn_skills', ?, unixepoch())",
  ).run(item.session_id);
}

async function processConsolidate(
  db: Database,
  model: string,
  _item: WorkItem,
  pointerCap: number,
): Promise<void> {
  const activeCount =
    db
      .query<{ cnt: number }, []>(
        "SELECT COUNT(*) as cnt FROM session_pointers WHERE archived = 0 AND pinned = 0",
      )
      .get()?.cnt ?? 0;

  if (activeCount <= pointerCap) return;

  const oldest = db
    .query<
      {
        id: string;
        session_id: string;
        key: string;
        phrase: string;
        summary: string | null;
      },
      [number]
    >(
      `SELECT id, session_id, key, phrase, summary FROM session_pointers
       WHERE archived = 0 AND pinned = 0
       ORDER BY created_at ASC LIMIT ?`,
    )
    .all(Math.min(5, activeCount - pointerCap + 2));

  if (oldest.length < 2) return;

  const descriptions = oldest
    .map(
      (p) => `- ${p.key}: ${p.phrase}${p.summary ? ` (${p.summary})` : ""}`,
    )
    .join("\n");

  const mergedJson = await llmGenerate(
    model,
    `These session pointers describe overlapping or related work. Merge them into ONE consolidated pointer.\n\nPointers:\n${descriptions}\n\nRespond with JSON only: {"key": "merged-key", "phrase": "consolidated phrase", "summary": "merged summary"}`,
    true,
  );

  let merged: { key: string; phrase: string; summary: string };
  try {
    merged = JSON.parse(mergedJson);
    merged.key = sanitizeKey(merged.key);
  } catch {
    return;
  }

  const sourceIds = oldest.map((p) => p.session_id);
  const sourcePointerIds = oldest.map((p) => p.id);

  db.transaction(() => {
    const id = crypto.randomUUID();
    db.query(
      `INSERT INTO session_pointers (id, session_id, key, phrase, summary, created_at, consolidated_from)
       VALUES (?, ?, ?, ?, ?, unixepoch(), ?)`,
    ).run(
      id,
      sourceIds[sourceIds.length - 1],
      merged.key,
      merged.phrase,
      merged.summary,
      JSON.stringify(sourceIds),
    );

    for (const pid of sourcePointerIds) {
      db.query(
        "UPDATE session_pointers SET archived = 1 WHERE id = ?",
      ).run(pid);
    }
  })();
}

function maybeQueueConsolidation(db: Database, pointerCap: number): void {
  const activeCount =
    db
      .query<{ cnt: number }, []>(
        "SELECT COUNT(*) as cnt FROM session_pointers WHERE archived = 0 AND pinned = 0",
      )
      .get()?.cnt ?? 0;

  if (activeCount > pointerCap) {
    const pending = db
      .query<{ id: number }, []>(
        "SELECT id FROM work_queue WHERE type = 'consolidate' AND processed_at IS NULL",
      )
      .get();

    if (!pending) {
      db.query(
        "INSERT INTO work_queue (type, created_at) VALUES ('consolidate', unixepoch())",
      ).run();
    }
  }
}

function sanitizeKey(raw: string): string {
  return raw
    .toLowerCase()
    .replace(/[^a-z0-9-]/g, "-")
    .replace(/-+/g, "-")
    .replace(/^-|-$/g, "")
    .slice(0, 50);
}
