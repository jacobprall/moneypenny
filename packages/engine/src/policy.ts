import type { Database } from "bun:sqlite";

export type PolicyEffect = "allow" | "warn" | "deny";

export interface PolicyResult {
  effect: PolicyEffect;
  policy: string;
  reason: string;
}

export interface BudgetConfig {
  maxDailyUsd: number;
  maxSessionUsd: number;
}

export function evaluateToolPolicy(
  db: Database,
  agentName: string,
  toolName: string,
): PolicyResult {
  const row = db
    .query<{ tools: string | null }, [string]>(
      "SELECT tools FROM agent_defs WHERE name = ?",
    )
    .get(agentName);

  if (row?.tools) {
    const tools: string[] = JSON.parse(row.tools);
    if (!tools.includes(toolName)) {
      return {
        effect: "deny",
        policy: "tool-restriction",
        reason: `Agent '${agentName}' is not authorized to use tool '${toolName}'`,
      };
    }
  }

  return { effect: "allow", policy: "tool-restriction", reason: "allowed" };
}

export function checkBudget(
  db: Database,
  sessionId: string,
  config: BudgetConfig,
): PolicyResult | null {
  const daily = db
    .query<{ total: number }, []>(
      "SELECT COALESCE(total, 0) as total FROM v_cost_today",
    )
    .get();
  const total = daily?.total ?? 0;

  if (total >= config.maxDailyUsd) {
    return {
      effect: "deny",
      policy: "budget-guard",
      reason: `Daily budget exceeded ($${total.toFixed(2)}/$${config.maxDailyUsd})`,
    };
  }

  const session = db
    .query<{ cost: number }, [string]>(
      "SELECT COALESCE(SUM(cost_usd), 0) as cost FROM messages WHERE session_id = ?",
    )
    .get(sessionId);
  const sessionCost = session?.cost ?? 0;

  if (sessionCost >= config.maxSessionUsd) {
    return {
      effect: "deny",
      policy: "budget-guard",
      reason: `Session budget exceeded ($${sessionCost.toFixed(2)}/$${config.maxSessionUsd})`,
    };
  }

  if (total > config.maxDailyUsd * 0.8) {
    return {
      effect: "warn",
      policy: "budget-guard",
      reason: `Approaching daily budget ($${total.toFixed(2)}/$${config.maxDailyUsd})`,
    };
  }

  return null;
}

export function loadBudgetConfig(db: Database): BudgetConfig {
  const row = db
    .query<{ conditions: string | null }, [string]>(
      "SELECT conditions FROM policies WHERE name = ?",
    )
    .get("Budget Guard");

  if (row?.conditions) {
    const conditions = JSON.parse(row.conditions);
    return {
      maxDailyUsd: conditions.maxDailyUsd ?? 10.0,
      maxSessionUsd: conditions.maxSessionUsd ?? 1.0,
    };
  }

  return { maxDailyUsd: 10.0, maxSessionUsd: 1.0 };
}
