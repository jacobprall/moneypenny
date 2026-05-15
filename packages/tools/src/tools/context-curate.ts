import { z } from "zod";
import type { ToolDefinition } from "../types.js";

const ACTIONS = [
  "search_memory",
  "forget_memory",
  "review_costs",
  "list_skills",
  "update_skill",
  "list_sessions",
  "summarize_session",
  "index_status",
  "inspect_policies",
  "prune_stale_chunks",
] as const;

const inputSchema = z.object({
  action: z.enum(ACTIONS),
  params: z.record(z.unknown()).optional(),
});

function numParam(value: unknown): number | undefined {
  if (typeof value === "number" && Number.isFinite(value)) return value;
  if (typeof value === "string" && value.trim() !== "") {
    const n = Number(value);
    if (Number.isFinite(n)) return n;
  }
  return undefined;
}

function strParam(value: unknown): string | undefined {
  return typeof value === "string" ? value : undefined;
}

export const contextCurateTool: ToolDefinition = {
  name: "context_curate",
  description:
    "Query and curate workspace intelligence: search conversation memory and skills, review usage costs, list sessions and skills, inspect governance policies, check code index health, summarize a session, or run maintenance (forget memory, update skill text, prune stale index chunks). Destructive actions are governed by policy elsewhere.",
  inputSchema,
  async execute(input, context): Promise<string> {
    try {
      const parsed = inputSchema.parse(input);
      const rawParams = parsed.params ?? {};
      const cc = context.services.contextCurate;

      switch (parsed.action) {
        case "search_memory": {
          const query = strParam(rawParams.query)?.trim() ?? "";
          if (!query) {
            return "Error: params.query (non-empty string) is required for search_memory.";
          }
          const limit = numParam(rawParams.limit);
          const result = cc.searchMemory(query, limit);
          return JSON.stringify(result);
        }
        case "forget_memory": {
          const id = strParam(rawParams.id)?.trim();
          const query = strParam(rawParams.query)?.trim();
          if (!id && !query) {
            return "Error: forget_memory requires params.id and/or params.query.";
          }
          const result = cc.forgetMemory({ id, query });
          return JSON.stringify(result);
        }
        case "review_costs": {
          const result = cc.reviewCosts();
          return JSON.stringify(result);
        }
        case "list_skills": {
          const rows = cc.listSkillsForCuration();
          return JSON.stringify(rows);
        }
        case "update_skill": {
          const name = strParam(rawParams.name)?.trim();
          const instructions = strParam(rawParams.instructions);
          if (!name || instructions === undefined) {
            return "Error: update_skill requires params.name and params.instructions (strings).";
          }
          cc.updateSkillInstructions(name, instructions);
          return `Updated skill "${name}" instructions (${instructions.length} characters).`;
        }
        case "list_sessions": {
          const limit = numParam(rawParams.limit);
          const rows = cc.listSessionsForCuration(limit);
          return JSON.stringify(rows);
        }
        case "summarize_session": {
          const sessionId = strParam(rawParams.sessionId)?.trim();
          if (!sessionId) {
            return "Error: summarize_session requires params.sessionId.";
          }
          return cc.summarizeSession(sessionId);
        }
        case "index_status": {
          const status = cc.indexStatus(context.repoPath);
          return JSON.stringify(status);
        }
        case "inspect_policies": {
          const rows = cc.inspectPolicies();
          return JSON.stringify(rows);
        }
        case "prune_stale_chunks": {
          const result = cc.pruneStaleChunks(context.repoPath);
          return JSON.stringify(result);
        }
        default: {
          const _exhaustive: never = parsed.action;
          return `Error: unsupported action ${_exhaustive}`;
        }
      }
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      return `Error: ${msg}`;
    }
  },
};
