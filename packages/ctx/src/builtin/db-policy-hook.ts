import type { Database } from "bun:sqlite";
import { evaluatePolicy, type EvaluateContext } from "../policy.js";
import type { Hook, HookContext, PreHookResult } from "./types.js";

export interface DbPolicyHookConfig {
  db: () => Database;
  actor?: string;
  denyByDefault?: boolean;
}

export function dbPolicyHook(config: DbPolicyHookConfig): Hook {
  return {
    name: "db-policy",
    async preTool(
      context: HookContext,
      toolName: string,
      _input: unknown,
    ): Promise<PreHookResult> {
      const db = config.db();
      const evalCtx: EvaluateContext = {
        actor: config.actor ?? "agent",
        toolName,
        sessionCost: context.sessionCostUsd,
        turnCost: context.turnCostUsd,
      };

      const decision = evaluatePolicy(db, evalCtx);

      if (decision.effect === "deny") {
        return {
          action: "reject",
          reason: decision.reason,
        };
      }

      if (decision.effect === "confirm") {
        return {
          action: "pause",
          reason: decision.reason,
        };
      }

      if (decision.effect === "allow" && decision.matchedPolicy === null && config.denyByDefault) {
        return {
          action: "reject",
          reason: "No matching policy; deny by default",
        };
      }

      return { action: "continue" };
    },
  };
}
