import type { Database } from "bun:sqlite";
import { insertMessage, nextSeq } from "@moneypenny/db";
import { llm } from "../../llm.js";

const DEFAULT_THRESHOLD = 120;

export async function maybeCompactRunningSession(
  db: Database,
  sessionId: string,
  threshold: number = DEFAULT_THRESHOLD,
): Promise<void> {
  const row = db
    .query<{ c: number }, [string]>(
      `SELECT COUNT(*) AS c FROM messages WHERE session_id = ?`,
    )
    .get(sessionId);
  const c = row?.c ?? 0;
  if (c < threshold) return;
  const mid = db
    .query<{ seq: number }, [string, number]>(
      `SELECT seq FROM messages WHERE session_id = ? ORDER BY seq ASC LIMIT 1 OFFSET ?`,
    )
    .get(sessionId, Math.floor(c / 2));
  if (mid?.seq == null) return;
  const chunk = db
    .query<{ role: string; content: string | null }, [string, number]>(
      `SELECT role, content FROM messages WHERE session_id = ? AND seq < ? ORDER BY seq ASC`,
    )
    .all(sessionId, mid.seq);
  const text = chunk
    .map((m) => `${m.role}: ${m.content ?? ""}`)
    .join("\n")
    .slice(0, 12_000);
  const summary = await llm(
    "fast",
    `Summarize the following conversation segment for the agent's memory (concise bullet points):\n${text}`,
  );
  insertMessage(db, {
    sessionId,
    seq: nextSeq(db, sessionId),
    role: "system",
    content: `[compact] ${summary}`,
  });
}
