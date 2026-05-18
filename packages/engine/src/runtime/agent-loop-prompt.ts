import type { CoreMessage } from "ai";
import type { Database } from "bun:sqlite";
import { listConventions, listSkills } from "@moneypenny/db";
import type { Message } from "@moneypenny/db";
import type { StoredSessionConfig } from "./types.js";

function sanitizePromptField(s: string): string {
  return s.replace(/[\x00-\x08\x0b\x0c\x0e-\x1f]/g, "").slice(0, 2000);
}

export function buildV2SystemPrompt(
  readDb: Database,
  cfg: StoredSessionConfig,
): string {
  const parts: string[] = [
    "--- BEGIN BLUEPRINT INSTRUCTIONS ---",
    cfg.instructions.trim(),
    "--- END BLUEPRINT INSTRUCTIONS ---",
    "",
    `Working directory (cwd): ${cfg.cwd}`,
  ];
  if (cfg.context.conventions) {
    const convs = listConventions(readDb).slice(0, 40);
    if (convs.length) {
      parts.push(
        "## Conventions (from project knowledge base)",
        ...convs.map((c) => `- **${sanitizePromptField(c.name)}**: ${sanitizePromptField(c.description)}`),
      );
    }
  }
  if (cfg.context.skills.length) {
    const all = listSkills(readDb);
    const want = new Set(cfg.context.skills);
    const picked = all.filter((s) => want.has(s.name));
    if (picked.length) {
      parts.push(
        "## Requested skills (from project knowledge base)",
        ...picked.map((s) => `- **${sanitizePromptField(s.name)}**: ${sanitizePromptField(s.description)}`),
      );
    }
  }
  return parts.join("\n");
}

function toolCallsFromRow(toolCalls: string | null): unknown {
  if (!toolCalls) return undefined;
  try {
    return JSON.parse(toolCalls);
  } catch {
    return undefined;
  }
}

export function messagesToCore(rows: Message[]): CoreMessage[] {
  const out: CoreMessage[] = [];
  for (const m of rows) {
    if (m.role === "user") {
      out.push({ role: "user", content: m.content ?? "" });
    } else if (m.role === "system") {
      out.push({ role: "system", content: m.content ?? "" });
    } else if (m.role === "assistant") {
      const tc = toolCallsFromRow(m.tool_calls);
      if (tc && Array.isArray(tc) && tc.length) {
        out.push({
          role: "assistant",
          content: m.content ?? "",
          toolCalls: tc,
        } as CoreMessage);
      } else {
        out.push({ role: "assistant", content: m.content ?? "" });
      }
    } else if (m.role === "tool" && m.tool_call_id) {
      out.push({
        role: "tool",
        content: m.content ?? "",
        toolCallId: m.tool_call_id,
      } as CoreMessage);
    }
  }
  return out;
}
