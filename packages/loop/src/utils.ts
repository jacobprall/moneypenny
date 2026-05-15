import type { HookContext } from "@moneypenny/ctx";
import type { ToolCallInfo } from "./types.js";

export function serializeToolCalls(calls: ToolCallInfo[]): string {
  return JSON.stringify(
    calls.map((c) => ({
      type: "tool_use" as const,
      id: c.id,
      name: c.name,
      input: c.input,
    })),
  );
}

export function makeHookCtx(base: {
  sessionCostUsd: number;
  turnCostUsd: number;
  turn: number;
  model: string;
  tokensIn?: number;
  tokensOut?: number;
}): HookContext {
  return {
    sessionCostUsd: base.sessionCostUsd,
    turnCostUsd: base.turnCostUsd,
    turnNumber: base.turn,
    model: base.model,
    tokensIn: base.tokensIn ?? 0,
    tokensOut: base.tokensOut ?? 0,
  };
}

export function extractTransformed(result: { action: string; transformed?: unknown }): string | undefined {
  if (result.action === "continue" && "transformed" in result && typeof result.transformed === "string") {
    return result.transformed;
  }
  return undefined;
}
