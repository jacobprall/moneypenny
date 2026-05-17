import type { Database } from "bun:sqlite";
import { llmJson } from "./llm.js";

const EXTRACT_PROMPT = `Analyze this developer conversation and extract reusable skills or patterns the user demonstrated or requested.

A "skill" is a reusable technique, workflow, or preference that should be remembered for future conversations.

Examples:
- "Prefers functional components over class components in React"
- "Uses Zod for runtime validation at API boundaries"
- "Runs tests before committing"

Return a JSON array of skills. Each skill has:
- name: short name (3-6 words)
- description: one-sentence description
- instructions: detailed how-to (1-3 sentences)

Return ONLY valid JSON array. If no skills found, return [].

Conversation:
`;

export async function extractSkills(
  db: Database,
  sessionId: string,
  model?: string,
): Promise<number> {
  const messages = db
    .query<{ role: string; content: string }, [string]>(
      `SELECT role, content FROM messages
       WHERE session_id = ? AND content IS NOT NULL
       ORDER BY turn ASC`,
    )
    .all(sessionId);

  if (messages.length < 3) return 0;

  const transcript = messages
    .map((m) => `${m.role}: ${m.content}`)
    .join("\n")
    .slice(0, 8000);

  const skills = await llmJson<Array<{
    name: string;
    description: string;
    instructions?: string;
  }>>("fast", EXTRACT_PROMPT + transcript, { model });

  if (!skills || !Array.isArray(skills)) return 0;

  let inserted = 0;
  for (const skill of skills) {
    if (!skill.name || !skill.description) continue;

    const existing = db
      .query<{ id: string; confidence: number }, [string]>(
        "SELECT id, confidence FROM skills WHERE name = ?",
      )
      .get(skill.name);

    if (existing) {
      db.query(
        "UPDATE skills SET confidence = MIN(1.0, confidence + 0.2), updated_at = unixepoch() WHERE id = ?",
      ).run(existing.id);
    } else {
      db.query(
        `INSERT INTO skills (id, name, description, instructions, confidence, source_session_id, created_at, updated_at)
         VALUES (?, ?, ?, ?, 0.5, ?, unixepoch(), unixepoch())`,
      ).run(
        crypto.randomUUID(),
        skill.name,
        skill.description,
        skill.instructions ?? null,
        sessionId,
      );
      inserted++;
    }
  }

  return inserted;
}
