import type { Database } from "bun:sqlite";
import { llmJson } from "./llm.js";

const DETECT_PROMPT = `Analyze these code samples and identify project conventions, patterns, and coding standards.

A "convention" is a consistent pattern observed across the codebase. Focus on:
- Naming conventions (files, variables, functions)
- Architecture patterns (file organization, module structure)
- Error handling approaches
- Testing patterns
- Import/export styles
- Code formatting choices

Return a JSON array of conventions. Each has:
- name: short name (3-6 words)
- category: one of "naming", "architecture", "error-handling", "testing", "style", "imports", "general"
- description: one-sentence description of the pattern

Return ONLY valid JSON array. If no clear conventions, return [].

Code samples:
`;

export async function detectConventions(
  db: Database,
  model?: string,
  sampleCount = 15,
): Promise<number> {
  const chunks = db
    .query<{ file_path: string; content: string; language: string | null }, [number]>(
      `SELECT file_path, content, language FROM code_chunks
       WHERE language IS NOT NULL
       ORDER BY RANDOM() LIMIT ?`,
    )
    .all(sampleCount);

  if (chunks.length < 3) return 0;

  const samples = chunks
    .map((c) => `### ${c.file_path} (${c.language})\n\`\`\`\n${c.content.slice(0, 3000)}\n\`\`\``)
    .join("\n\n");

  const conventions = await llmJson<Array<{
    name: string;
    category: string;
    description: string;
  }>>("fast", DETECT_PROMPT + samples.slice(0, 12000), { model });

  if (!conventions || !Array.isArray(conventions)) return 0;

  let inserted = 0;
  for (const conv of conventions) {
    if (!conv.name || !conv.description) continue;

    const existing = db
      .query<{ id: string; confidence: number }, [string]>(
        "SELECT id, confidence FROM conventions WHERE name = ?",
      )
      .get(conv.name);

    if (existing) {
      db.query(
        "UPDATE conventions SET confidence = MIN(1.0, confidence + 0.15) WHERE id = ?",
      ).run(existing.id);
    } else {
      db.query(
        `INSERT INTO conventions (id, name, category, description, confidence, created_at)
         VALUES (?, ?, ?, ?, 0.4, unixepoch())`,
      ).run(
        `detected:${crypto.randomUUID().slice(0, 8)}`,
        conv.name,
        conv.category ?? "general",
        conv.description,
      );
      inserted++;
    }
  }

  return inserted;
}
