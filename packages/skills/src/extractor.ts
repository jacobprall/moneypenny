/**
 * Session knowledge extractor.
 *
 * At the end of a session, reads the conversation history and calls a model
 * to distill durable project knowledge into per-topic "learned" skills.
 * Prefers local gen (zero-cost) with cloud Anthropic as fallback.
 */

import type { AgentDB, Skill, LocalGen } from "@moneypenny/db";
import { upsertSkill, listSkills } from "./skills.js";

const MIN_TURNS_FOR_EXTRACTION = 3;

export interface ExtractorConfig {
  apiKey?: string;
  model?: string;
  minTurns?: number;
  localGen?: LocalGen;
}

export interface ExtractionResult {
  skillsUpserted: number;
  skillNames: string[];
  model: string;
}

interface ExtractedSkill {
  name: string;
  description: string;
  instructions: string;
}

interface ConversationRow {
  turn: number;
  role: string;
  content: string | null;
}

function loadConversationText(db: AgentDB): { text: string; turns: number } {
  const sid = db.activeSessionId ?? null;

  let rows: ConversationRow[];
  if (sid) {
    rows = db.db
      .prepare(
        `SELECT turn, role, content FROM messages
         WHERE session_id = ? AND role IN ('user', 'assistant') AND content IS NOT NULL
         ORDER BY turn ASC, created_at ASC`,
      )
      .all(sid) as ConversationRow[];
  } else {
    rows = db.db
      .prepare(
        `SELECT turn, role, content FROM messages
         WHERE role IN ('user', 'assistant') AND content IS NOT NULL
         ORDER BY turn ASC, created_at ASC`,
      )
      .all() as ConversationRow[];
  }

  if (rows.length === 0) return { text: "", turns: 0 };

  const distinctTurns = new Set(rows.map((r) => r.turn));
  const lines = rows.map((r) => `[${r.role}] ${r.content}`);
  return { text: lines.join("\n\n"), turns: distinctTurns.size };
}

function buildExtractionPrompt(conversationText: string, existingSkills: Skill[]): string {
  const existingSection =
    existingSkills.length > 0
      ? [
          "## Existing learned skills (merge with these if topics overlap)\n",
          ...existingSkills.map(
            (s) => `### ${s.name}\n${s.description}\n\n${s.instructions}\n`,
          ),
        ].join("\n")
      : "No existing learned skills yet.";

  return `You are a knowledge extraction system. Read the conversation below and extract durable project knowledge into per-topic skills.

## What to extract
- Architecture decisions (e.g. "we use JWT, not sessions")
- Code conventions (e.g. "snake_case for DB columns, camelCase in TS")
- Project-specific patterns (e.g. "rate limiting uses token bucket in Redis")
- Debugging insights (e.g. "the auth middleware must run before CORS")
- User preferences (e.g. "always write tests first", "use Vitest not Jest")

## What to skip
- One-off Q&A with no lasting value
- File contents (already in the code index)
- Error messages and stack traces
- Conversation filler ("thanks", "ok", "let me think")

## Rules
- Each skill should be a coherent topic (e.g. "auth-patterns", "db-conventions", "test-strategy").
- Use kebab-case for skill names.
- If an existing learned skill covers the same topic, produce a merged/updated version that combines old and new knowledge. Keep the same name.
- Only create skills for genuinely durable knowledge, not ephemeral debugging.
- If there is nothing worth extracting, return an empty array.

${existingSection}

## Conversation

${conversationText}

## Output format

Respond with ONLY a JSON array of skill objects. No markdown fencing, no explanation.

[
  {
    "name": "topic-slug",
    "description": "One-line description of what this skill covers",
    "instructions": "Detailed knowledge in markdown format"
  }
]

If nothing is worth extracting, respond with: []`;
}

function buildLocalExtractionPrompt(conversationText: string, existingSkills: Skill[]): string {
  const base = buildExtractionPrompt(conversationText, existingSkills);
  return `<|im_start|>system
You are a knowledge extraction system. Respond with ONLY a JSON array.<|im_end|>
<|im_start|>user
${base}<|im_end|>
<|im_start|>assistant
`;
}

function parseExtractedSkills(raw: string): ExtractedSkill[] {
  const trimmed = raw.trim();

  const jsonStr = trimmed.startsWith("```")
    ? trimmed.replace(/^```(?:json)?\s*/, "").replace(/\s*```$/, "")
    : trimmed;

  try {
    const parsed = JSON.parse(jsonStr);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter(
      (item: unknown): item is ExtractedSkill =>
        typeof item === "object" &&
        item !== null &&
        typeof (item as Record<string, unknown>).name === "string" &&
        typeof (item as Record<string, unknown>).description === "string" &&
        typeof (item as Record<string, unknown>).instructions === "string",
    );
  } catch {
    return [];
  }
}

async function generateViaCloud(prompt: string, apiKey: string, model: string): Promise<string> {
  const Anthropic = (await import("@anthropic-ai/sdk")).default;
  const client = new Anthropic({ apiKey });
  const response = await client.messages.create({
    model,
    max_tokens: 4096,
    messages: [{ role: "user", content: prompt }],
  });
  return response.content
    .filter((b) => b.type === "text")
    .map((b) => (b as { type: "text"; text: string }).text)
    .join("");
}

/**
 * Run end-of-session knowledge extraction.
 *
 * Prefers local gen (zero-cost, no network). Falls back to cloud Anthropic
 * if local gen is unavailable and an API key is provided.
 */
export async function extractSessionKnowledge(
  db: AgentDB,
  config: ExtractorConfig,
): Promise<ExtractionResult | null> {
  const minTurns = config.minTurns ?? MIN_TURNS_FOR_EXTRACTION;

  const { text, turns } = loadConversationText(db);
  if (turns < minTurns || text.length === 0) return null;

  const existingLearned = listSkills(db).filter((s) => s.source === "learned");

  let textContent: string;
  let modelUsed: string;

  if (config.localGen?.isAvailable()) {
    const prompt = buildLocalExtractionPrompt(text, existingLearned);
    textContent = config.localGen.generate(prompt, { maxTokens: 256 });
    modelUsed = "local";
  } else if (config.apiKey) {
    const model = config.model ?? "claude-haiku-4-5-20251001";
    const prompt = buildExtractionPrompt(text, existingLearned);
    textContent = await generateViaCloud(prompt, config.apiKey, model);
    modelUsed = model;
  } else {
    return null;
  }

  const skills = parseExtractedSkills(textContent);
  if (skills.length === 0) return { skillsUpserted: 0, skillNames: [], model: modelUsed };

  const tx = db.db.transaction(() => {
    for (const skill of skills) {
      upsertSkill(db, {
        name: skill.name,
        description: skill.description,
        instructions: skill.instructions,
        source: "learned",
      });
    }
  });

  tx();

  return {
    skillsUpserted: skills.length,
    skillNames: skills.map((s) => s.name),
    model: modelUsed,
  };
}
