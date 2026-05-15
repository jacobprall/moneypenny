import { getConversation, getSessionMetrics, setActiveSession, type AgentDB } from "@moneypenny/db";
import {
  CostLimitError,
  LoopError,
  type AgentLoop,
  type LoopErrorCode,
  type LoopEvent,
  type TokenUsage,
  type ToolCallInfo,
} from "@moneypenny/loop";
import type { AgentEvent, RunOptions } from "./types.js";

function jsonStable(a: unknown, b: unknown): boolean {
  try {
    return JSON.stringify(a) === JSON.stringify(b);
  } catch {
    return false;
  }
}

/** Resolves tool call ids when LoopEvents only carry tool names (and optionally input). */
class ToolCallCorrelation {
  private pool: ToolCallInfo[] = [];
  private pendingCompletionIds: string[] = [];

  resetFromAssistant(toolCalls: ToolCallInfo[]): void {
    this.pool = [...toolCalls];
    this.pendingCompletionIds = [];
  }

  takePreToolReject(name: string): string {
    const i = this.pool.findIndex((tc) => tc.name === name);
    if (i < 0) return `unknown-${name}`;
    const [tc] = this.pool.splice(i, 1);
    return tc!.id;
  }

  takeCalling(name: string, input: unknown): string {
    const i = this.pool.findIndex((tc) => tc.name === name && jsonStable(tc.input, input));
    if (i < 0) {
      const j = this.pool.findIndex((tc) => tc.name === name);
      if (j < 0) return `unknown-${name}`;
      const [tc] = this.pool.splice(j, 1);
      const id = tc!.id;
      this.pendingCompletionIds.push(id);
      return id;
    }
    const [tc] = this.pool.splice(i, 1);
    const id = tc!.id;
    this.pendingCompletionIds.push(id);
    return id;
  }

  takeCompleteOrPostError(): string {
    const id = this.pendingCompletionIds.shift();
    return id ?? "unknown";
  }

  resolveToolErrorId(name: string): string {
    if (this.pendingCompletionIds.length > 0) {
      return this.takeCompleteOrPostError();
    }
    return this.takePreToolReject(name);
  }
}

function classifyLlmBridgeError(code: LoopErrorCode, message: string): { code: string; retryable: boolean } {
  const lower = message.toLowerCase();
  const isRateLimited =
    lower.includes("429") || /\brate\s*limit/.test(lower) || lower.includes("too many requests");
  if (isRateLimited) {
    return { code: "rate_limited", retryable: true };
  }
  return { code, retryable: loopErrorRetryable(code, message) };
}

function loopErrorRetryable(code: LoopErrorCode, message: string): boolean {
  switch (code) {
    case "llm_api_error":
      return /\b5\d\d\b/.test(message) || lowerIncludes(message, "timeout") || lowerIncludes(message, "overloaded");
    case "llm_empty_response":
      return true;
    case "internal_error":
    case "hook_rejected":
    case "tool_execution_error":
    case "tool_rejected":
    case "max_iterations":
    case "aborted":
    case "context_assembly_error":
    case "no_conversation":
    case "cost_limit_exceeded":
      return false;
    default:
      return false;
  }
}

function lowerIncludes(haystack: string, needle: string): boolean {
  return haystack.toLowerCase().includes(needle);
}

function mapPausedToErrorCode(reason: string): { code: string; retryable: boolean } {
  if (reason.includes("maxCostPerTurn") || reason.includes("maxCostPerSession") || /session cost/i.test(reason)) {
    return { code: "cost_limit_exceeded", retryable: false };
  }
  return { code: "paused", retryable: false };
}

export class AgentBridge {
  private abortController: AbortController | null = null;
  private readonly tools = new ToolCallCorrelation();

  constructor(
    private readonly loop: AgentLoop,
    private readonly db: AgentDB,
  ) {}

  /**
   * Abort signal for the current `run()` iteration. The `AgentLoop` must be created with
   * `createAgentLoop({ ..., signal: bridge.runAbortSignal })` (or an `AbortSignal` that tracks
   * the same underlying controller) so provider/tool execution cancels promptly.
   */
  get runAbortSignal(): AbortSignal | undefined {
    return this.abortController?.signal;
  }

  abort(): void {
    this.abortController?.abort();
  }

  async *run(message: string, options: RunOptions): AsyncGenerator<AgentEvent> {
    this.abortController = new AbortController();
    setActiveSession(this.db, options.sessionId);

    const convo = getConversation(this.db, { sessionId: options.sessionId });
    yield { type: "session_loaded", sessionId: options.sessionId, messageCount: convo.length };

    try {
      for await (const event of this.loop.run(this.db, message)) {
        if (this.abortController.signal.aborted) {
          yield {
            type: "error",
            code: "aborted",
            message: "Aborted",
            retryable: false,
          };
          return;
        }

        yield* this.mapLoopEvent(event);
      }
    } catch (e) {
      yield this.mapCaughtError(e);
    }
  }

  private *mapLoopEvent(event: LoopEvent): Generator<AgentEvent> {
    switch (event.type) {
      case "turn.started":
        return;
      case "llm.streaming":
        yield { type: "stream_token", text: event.delta };
        return;
      case "strategy.progress":
        yield {
          type: "strategy_progress",
          update: {
            strategy: event.strategy,
            iteration: event.iteration,
            maxIterations: event.maxIterations,
            findingsCount: event.findingsCount,
            status: event.status,
          },
        };
        return;
      case "llm.complete": {
        this.tools.resetFromAssistant(event.message.toolCalls);
        return;
      }
      case "tool.calling": {
        const id = this.tools.takeCalling(event.name, event.input);
        yield { type: "tool_call_start", id, name: event.name, args: event.input };
        return;
      }
      case "tool.complete": {
        const id = this.tools.takeCompleteOrPostError();
        yield {
          type: "tool_call_result",
          id,
          result: event.output,
          success: true,
          durationMs: event.durationMs,
        };
        return;
      }
      case "tool.error": {
        const id = this.tools.resolveToolErrorId(event.name);
        yield {
          type: "tool_call_result",
          id,
          result: event.error,
          success: false,
          durationMs: 0,
        };
        return;
      }
      case "turn.complete": {
        const metrics = getSessionMetrics(this.db, undefined);
        yield {
          type: "turn_complete",
          usage: turnCompleteUsage(event),
          costUsd: event.cost.costUsd,
        };
        yield {
          type: "cost_update",
          sessionCostUsd: metrics.totalCostUsd,
          turnCostUsd: event.cost.costUsd,
        };
        return;
      }
      case "error": {
        yield this.mapLoopErrorEvent(event.error);
        return;
      }
      case "paused": {
        const { code, retryable } = mapPausedToErrorCode(event.reason);
        yield { type: "error", code, message: event.reason, retryable };
        return;
      }
      default: {
        const exhaustive: never = event;
        throw new Error(`Unhandled loop event type: ${(exhaustive as LoopEvent).type}`);
      }
    }
  }

  private mapCaughtError(e: unknown): AgentEvent {
    if (e instanceof LoopError) {
      return this.mapLoopErrorInstance(e);
    }
    const message = e instanceof Error ? e.message : String(e);
    return {
      type: "error",
      code: "bridge_error",
      message,
      retryable: false,
    };
  }

  private mapLoopErrorEvent(err: LoopError): AgentEvent {
    return this.mapLoopErrorInstance(err);
  }

  private mapLoopErrorInstance(err: LoopError): AgentEvent {
    const { code, retryable } =
      err instanceof CostLimitError
        ? { code: "cost_limit_exceeded", retryable: false }
        : classifyLlmBridgeError(err.code, err.message);
    return { type: "error", code, message: err.message, retryable };
  }
}

function turnCompleteUsage(event: LoopEvent & { type: "turn.complete" }): TokenUsage {
  return {
    inputTokens: event.cost.inputTokens,
    outputTokens: event.cost.outputTokens,
    cacheReadInputTokens: event.cost.cachedInputTokens,
  };
}
