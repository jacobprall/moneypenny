import type { Database } from "bun:sqlite";
import type { EventBus } from "../../events/index.js";
import { insertPointer, insertSkill, listMessages, getSession } from "@moneypenny/db";
import { detectConventions } from "../../conventions.js";
import { llmJson } from "../../llm.js";

const LABEL_PROMPT = `Return JSON only: {"label": "<8 word session title>"} summarizing this transcript:\n`;

const POINTER_PROMPT = `From the transcript, list memory pointers as JSON array {"pointers":[{"key":"short-id","phrase":"what to remember"}]}:\n`;

const SKILL_PROMPT = `Return JSON array of skills {"name","description","instructions?"} from this conversation (or []):\n`;

export async function extractOnArchive(
  db: Database,
  sessionId: string,
  events?: EventBus,
): Promise<void> {
  const s = getSession(db, sessionId);
  if (!s) return;
  const msgs = listMessages(db, { sessionId, direction: "before", limit: 200 });
  const transcript = msgs
    .map((m) => `${m.role}: ${m.content ?? ""}`)
    .join("\n")
    .slice(0, 14_000);

  const label = await llmJson<{ label?: string }>(
    "fast",
    LABEL_PROMPT + transcript,
  );
  if (label?.label) {
    db.query<unknown, [string | null, string]>(
      `UPDATE sessions SET label = ? WHERE id = ?`,
    ).run(label.label, sessionId);
  }

  const pointers = await llmJson<{
    pointers?: Array<{ key: string; phrase: string }>;
  }>("fast", POINTER_PROMPT + transcript);
  if (pointers?.pointers) {
    for (const p of pointers.pointers) {
      if (!p.key || !p.phrase) continue;
      insertPointer(db, { sessionId, key: p.key, phrase: p.phrase });
      events?.emit({
        type: "knowledge.pointer_created",
        session_id: sessionId,
        detail: { key: p.key, session_id: sessionId },
      });
    }
  }

  const skills = await llmJson<
    Array<{ name: string; description: string; instructions?: string }>
  >("fast", SKILL_PROMPT + transcript);
  if (skills?.length) {
    for (const sk of skills) {
      if (!sk.name || !sk.description) continue;
      insertSkill(db, {
        name: sk.name,
        description: sk.description,
        instructions: sk.instructions ?? null,
        confidence: 0.45,
        source_session_id: sessionId,
      });
      events?.emit({
        type: "knowledge.skill_extracted",
        session_id: sessionId,
        detail: { skill_name: sk.name, session_id: sessionId },
      });
    }
  }

  const conventionCount = await detectConventions(db);
  if (events && conventionCount > 0) {
    events.emit({ type: "knowledge.convention_detected", session_id: sessionId, detail: { count: conventionCount } });
  }
}
