import { streamText, generateText } from "ai";
import type { Database } from "bun:sqlite";
import { assembleSystemPrompt } from "@moneypenny/db";
import { createToolSet } from "./tools.js";
import { calculateCost } from "./cost.js";
import { resolveModel } from "./llm.js";

export interface AgentConfig {
  db: Database;
  model: string;
  agentName: string;
  maxSteps?: number;
  toolFilter?: string[];
}

export { resolveModel };

export function runAgentTurn(
  config: AgentConfig,
  messages: Array<{ role: "user" | "assistant"; content: string }>,
) {
  const system = assembleSystemPrompt(config.db, config.agentName);
  const tools = createToolSet(config.db, config.toolFilter);

  return streamText({
    model: resolveModel(config.model),
    system,
    messages,
    tools,
    maxSteps: config.maxSteps ?? 5,
  });
}

export async function runAgentOnce(
  config: AgentConfig,
  messages: Array<{ role: "user" | "assistant"; content: string }>,
) {
  const system = assembleSystemPrompt(config.db, config.agentName);
  const tools = createToolSet(config.db, config.toolFilter);

  return generateText({
    model: resolveModel(config.model),
    system,
    messages,
    tools,
    maxSteps: config.maxSteps ?? 5,
  });
}

export function recordTurn(
  db: Database,
  sessionId: string,
  role: string,
  content: string,
  model?: string,
  usage?: { promptTokens: number; completionTokens: number },
): void {
  const row = db
    .query<{ maxTurn: number | null }, [string]>(
      "SELECT MAX(turn) as maxTurn FROM messages WHERE session_id = ?",
    )
    .get(sessionId);
  const turn = (row?.maxTurn ?? -1) + 1;

  const costUsd =
    model && usage ? calculateCost(model, usage) : null;

  db.query(
    `INSERT INTO messages (id, turn, role, content, tokens_in, tokens_out, cost_usd, session_id, created_at)
     VALUES (?, ?, ?, ?, ?, ?, ?, ?, unixepoch())`,
  ).run(
    crypto.randomUUID(),
    turn,
    role,
    content,
    usage?.promptTokens ?? null,
    usage?.completionTokens ?? null,
    costUsd,
    sessionId,
  );
}
