import type { Database } from "bun:sqlite";

interface SessionPointer {
  date: string;
  key: string;
  phrase: string;
  pinned: number;
}

interface Skill {
  name: string;
  description: string;
}

interface Convention {
  name: string;
  description: string;
}

interface Policy {
  name: string;
  effect: string;
  description: string;
}

interface AgentContext {
  previous_sessions: SessionPointer[] | null;
  skills: Skill[] | null;
  conventions: Convention[] | null;
  policies: Policy[] | null;
  pending_work: number;
}

export function assembleSystemPrompt(
  db: Database,
  agentName: string,
  customInstructions?: string,
): string {
  const row = db
    .query<{ context: string }, []>("SELECT context FROM v_agent_context")
    .get();

  if (!row) return buildBasePrompt(agentName, customInstructions);

  const ctx: AgentContext = JSON.parse(row.context);
  const parts: string[] = [];

  parts.push(buildBasePrompt(agentName, customInstructions));

  if (ctx.previous_sessions?.length) {
    parts.push(formatPreviousSessions(ctx.previous_sessions));
  }

  if (ctx.skills?.length) {
    parts.push(formatSkills(ctx.skills));
  }

  if (ctx.conventions?.length) {
    parts.push(formatConventions(ctx.conventions));
  }

  if (ctx.policies?.length) {
    parts.push(formatPolicies(ctx.policies));
  }

  return parts.join("\n\n");
}

function buildBasePrompt(
  agentName: string,
  customInstructions?: string,
): string {
  const lines = [
    `You are ${agentName}, a developer assistant with persistent memory across sessions and learned skills.`,
    "You can expand previous session pointers for details and search the codebase.",
  ];
  if (customInstructions) {
    lines.push("", "## Custom Instructions", customInstructions);
  }
  return lines.join("\n");
}

function formatPreviousSessions(pointers: SessionPointer[]): string {
  const lines = ["## Previous Sessions"];
  for (const p of pointers) {
    const pin = p.pinned ? " [pinned]" : "";
    lines.push(`- ${p.date} **${p.key}**: ${p.phrase}${pin}`);
  }
  lines.push(
    "",
    "_Use `expand_previous_session` with a key to see the full summary._",
  );
  return lines.join("\n");
}

function formatSkills(skills: Skill[]): string {
  const lines = ["## Learned Skills"];
  for (const s of skills) {
    lines.push(`- **${s.name}**: ${s.description}`);
  }
  return lines.join("\n");
}

function formatConventions(conventions: Convention[]): string {
  const lines = ["## Project Conventions"];
  for (const c of conventions) {
    lines.push(`- **${c.name}**: ${c.description}`);
  }
  return lines.join("\n");
}

function formatPolicies(policies: Policy[]): string {
  const lines = ["## Active Policies"];
  for (const p of policies) {
    lines.push(`- [${p.effect}] **${p.name}**: ${p.description}`);
  }
  return lines.join("\n");
}
