import { evaluatePolicy, type EvaluateContext } from "../policy.js";
import type { Hook, HookContext, PreHookResult } from "./types.js";

import type { AgentDB } from "@moneypenny/db";

export interface DbPolicyHookConfig {
  db: () => AgentDB;
  actor?: string;
  denyByDefault?: boolean;
  onAudit?: (toolName: string, reason: string, matchedPolicy: unknown) => void;
}

const PATH_KEYS = ["path", "file_path", "file", "target", "destination", "source"];

function extractPath(input: unknown): string | undefined {
  if (typeof input !== "object" || input === null) return undefined;
  const record = input as Record<string, unknown>;
  for (const key of PATH_KEYS) {
    const val = record[key];
    if (typeof val === "string" && val.length > 0) return val;
  }
  return undefined;
}

export function dbPolicyHook(config: DbPolicyHookConfig): Hook {
  return {
    name: "db-policy",
    async preTool(
      context: HookContext,
      toolName: string,
      input: unknown,
    ): Promise<PreHookResult> {
      const db = config.db();
      const evalCtx: EvaluateContext = {
        actor: config.actor ?? "agent",
        toolName,
        args: input,
        path: extractPath(input),
        sessionCost: context.sessionCostUsd,
        turnCost: context.turnCostUsd,
      };

      const decision = evaluatePolicy(db, evalCtx);

      if (decision.effect === "deny") {
        return { action: "reject", reason: decision.reason };
      }

      if (decision.effect === "confirm") {
        return { action: "pause", reason: decision.reason };
      }

      if (decision.effect === "audit") {
        if (config.onAudit) {
          config.onAudit(toolName, decision.reason, decision.matchedPolicy);
        }
        return { action: "continue" };
      }

      if (decision.effect === "allow" && decision.matchedPolicy === null && config.denyByDefault) {
        return { action: "reject", reason: "No matching policy; deny by default" };
      }

      return { action: "continue" };
    },
  };
}
