import type { TokenUsage } from "@moneypenny/loop";

export interface StrategyUpdate {
  strategy: string;
  iteration: number;
  maxIterations: number;
  findingsCount: number;
  status: string;
}

export interface RunOptions {
  sessionId: string;
  blueprint?: string;
}

export type AgentEvent =
  | { type: "stream_token"; text: string }
  | { type: "tool_call_start"; id: string; name: string; args: unknown }
  | { type: "tool_call_result"; id: string; result: string; success: boolean; durationMs: number }
  | {
      type: "governance_decision";
      toolCallId: string;
      effect: string;
      policyName?: string;
      reason: string;
    }
  | { type: "strategy_progress"; update: StrategyUpdate }
  | { type: "cost_update"; sessionCostUsd: number; turnCostUsd: number }
  | { type: "turn_complete"; usage: TokenUsage; costUsd: number }
  | { type: "error"; code: string; message: string; retryable: boolean }
  | { type: "session_loaded"; sessionId: string; messageCount: number };
