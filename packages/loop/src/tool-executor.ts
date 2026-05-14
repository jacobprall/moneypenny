import {
  appendEvent,
  appendMessage,
  type AgentDB,
} from "@swe/db";
import type { HookContext, HookPipeline } from "@swe/ctx";
import type { ToolContext, ToolRegistry } from "@swe/tools";
import { LoopError, type LoopEvent, type ToolCallInfo } from "./types.js";

export interface ToolExecutorConfig {
  hooks: HookPipeline;
  tools: ToolRegistry;
  maxToolOutputBytes: number;
  toolTimeoutMs?: number;
  signal?: AbortSignal;
  onEvent: (e: LoopEvent) => LoopEvent;
}

const DEFAULT_TOOL_TIMEOUT_MS = 120_000; // 2 minutes

async function executeWithTimeout(
  tools: ToolRegistry,
  name: string,
  input: unknown,
  context: ToolContext,
  timeoutMs: number,
  signal?: AbortSignal,
): Promise<string> {
  const ac = new AbortController();
  const combinedSignal = ac.signal;

  const abortOnParent = signal
    ? () => ac.abort()
    : undefined;
  if (abortOnParent) signal!.addEventListener("abort", abortOnParent, { once: true });

  const timer = setTimeout(() => ac.abort(), timeoutMs);

  const contextWithSignal: ToolContext = { ...context, signal: combinedSignal };

  try {
    return await tools.execute(name, input, contextWithSignal);
  } catch (e) {
    if (combinedSignal.aborted && !signal?.aborted) {
      return `Error: tool "${name}" timed out after ${Math.round(timeoutMs / 1000)}s`;
    }
    throw e;
  } finally {
    clearTimeout(timer);
    if (abortOnParent) signal!.removeEventListener("abort", abortOnParent);
  }
}

function truncateOutput(output: string, maxBytes: number): string {
  const encoder = new TextEncoder();
  const bytes = encoder.encode(output);
  if (bytes.byteLength <= maxBytes) return output;
  const truncated = new TextDecoder().decode(bytes.slice(0, maxBytes));
  return truncated + `\n\n[truncated: output exceeded ${maxBytes} bytes]`;
}

function extractTransformed(result: { action: string; transformed?: unknown }): string | undefined {
  if (result.action === "continue" && "transformed" in result && typeof result.transformed === "string") {
    return result.transformed;
  }
  return undefined;
}

export async function* executeToolsSequential(
  cfg: ToolExecutorConfig,
  db: AgentDB,
  turn: number,
  toolCalls: ToolCallInfo[],
  toolContext: ToolContext,
  hookCtx: HookContext,
): AsyncGenerator<LoopEvent> {
  for (const toolCall of toolCalls) {
    if (cfg.signal?.aborted) {
      yield cfg.onEvent({ type: "error", error: new LoopError("Aborted", "aborted") });
      return;
    }
    const result = yield* executeSingleTool(cfg, db, turn, toolCall, toolContext, hookCtx);
    if (result === "pause") return;
  }
}

export async function* executeToolsParallel(
  cfg: ToolExecutorConfig,
  db: AgentDB,
  turn: number,
  toolCalls: ToolCallInfo[],
  toolContext: ToolContext,
  hookCtx: HookContext,
): AsyncGenerator<LoopEvent> {
  if (cfg.signal?.aborted) {
    yield cfg.onEvent({ type: "error", error: new LoopError("Aborted", "aborted") });
    return;
  }

  const rejectedIds = new Set<string>();
  for (const toolCall of toolCalls) {
    const preToolResult = await cfg.hooks.runPreTool(hookCtx, toolCall.name, toolCall.input);
    if (preToolResult.action === "pause") {
      yield cfg.onEvent({ type: "paused", reason: preToolResult.reason });
      appendEvent(db, { type: "turn.paused", payload: { reason: preToolResult.reason }, turn });
      return;
    }
    if (preToolResult.action === "reject") {
      const errorMsg = `Tool ${toolCall.name} rejected: ${preToolResult.reason}`;
      yield cfg.onEvent({ type: "tool.error", name: toolCall.name, error: errorMsg });
      appendMessage(db, { turn, role: "tool", toolCallId: toolCall.id, content: errorMsg });
      appendEvent(db, { type: "tool.error", payload: { tool: toolCall.name, error: errorMsg }, turn });
      rejectedIds.add(toolCall.id);
    }
  }

  const executableCalls = toolCalls.filter((tc) => !rejectedIds.has(tc.id));
  if (executableCalls.length === 0) return;

  for (const tc of executableCalls) {
    yield cfg.onEvent({ type: "tool.calling", name: tc.name, input: tc.input });
    appendEvent(db, { type: "tool.called", payload: { tool: tc.name, input: tc.input }, turn });
  }

  const timeoutMs = cfg.toolTimeoutMs ?? DEFAULT_TOOL_TIMEOUT_MS;
  const results = await Promise.allSettled(
    executableCalls.map(async (toolCall) => {
      const startMs = Date.now();
      const output = await executeWithTimeout(cfg.tools, toolCall.name, toolCall.input, toolContext, timeoutMs, cfg.signal);
      const durationMs = Date.now() - startMs;
      return { toolCall, output, durationMs };
    }),
  );

  for (let i = 0; i < results.length; i++) {
    const result = results[i];
    const toolCall = executableCalls[i];
    if (result.status === "rejected") {
      const err = result.reason instanceof Error ? result.reason.message : String(result.reason);
      yield cfg.onEvent({ type: "tool.error", name: toolCall.name, error: err });
      appendMessage(db, { turn, role: "tool", toolCallId: toolCall.id, content: err });
      appendEvent(db, { type: "tool.error", payload: { tool: toolCall.name, error: err }, turn });
      continue;
    }

    const { output, durationMs } = result.value;
    let finalOutput = truncateOutput(output, cfg.maxToolOutputBytes);

    const postToolResult = await cfg.hooks.runPostTool(hookCtx, toolCall.name, finalOutput);
    if (postToolResult.action === "reject") {
      yield cfg.onEvent({ type: "tool.error", name: toolCall.name, error: postToolResult.reason });
      appendMessage(db, { turn, role: "tool", toolCallId: toolCall.id, content: postToolResult.reason });
      appendEvent(db, { type: "tool.error", payload: { tool: toolCall.name, error: postToolResult.reason }, turn });
      continue;
    }
    if (postToolResult.action === "pause") {
      yield cfg.onEvent({ type: "paused", reason: postToolResult.reason });
      appendEvent(db, { type: "turn.paused", payload: { reason: postToolResult.reason }, turn });
      return;
    }

    const transformed = extractTransformed(postToolResult);
    if (transformed !== undefined) finalOutput = transformed;

    yield cfg.onEvent({ type: "tool.complete", name: toolCall.name, output: finalOutput, durationMs });
    appendMessage(db, { turn, role: "tool", toolCallId: toolCall.id, content: finalOutput });
    appendEvent(db, { type: "tool.complete", payload: { tool: toolCall.name, durationMs }, turn });
  }
}

async function* executeSingleTool(
  cfg: ToolExecutorConfig,
  db: AgentDB,
  turn: number,
  toolCall: ToolCallInfo,
  toolContext: ToolContext,
  hookCtx: HookContext,
): AsyncGenerator<LoopEvent, "pause" | "continue"> {
  const preToolResult = await cfg.hooks.runPreTool(hookCtx, toolCall.name, toolCall.input);
  if (preToolResult.action === "pause") {
    yield cfg.onEvent({ type: "paused", reason: preToolResult.reason });
    appendEvent(db, { type: "turn.paused", payload: { reason: preToolResult.reason }, turn });
    return "pause";
  }
  if (preToolResult.action === "reject") {
    const errorMsg = `Tool ${toolCall.name} rejected: ${preToolResult.reason}`;
    yield cfg.onEvent({ type: "tool.error", name: toolCall.name, error: errorMsg });
    appendMessage(db, { turn, role: "tool", toolCallId: toolCall.id, content: errorMsg });
    appendEvent(db, { type: "tool.error", payload: { tool: toolCall.name, error: errorMsg }, turn });
    return "continue";
  }

  yield cfg.onEvent({ type: "tool.calling", name: toolCall.name, input: toolCall.input });
  appendEvent(db, { type: "tool.called", payload: { tool: toolCall.name, input: toolCall.input }, turn });

  const timeoutMs = cfg.toolTimeoutMs ?? DEFAULT_TOOL_TIMEOUT_MS;
  const startMs = Date.now();
  const output = await executeWithTimeout(cfg.tools, toolCall.name, toolCall.input, toolContext, timeoutMs, cfg.signal);
  const durationMs = Date.now() - startMs;

  let finalOutput = truncateOutput(output, cfg.maxToolOutputBytes);

  const postToolResult = await cfg.hooks.runPostTool(hookCtx, toolCall.name, finalOutput);
  if (postToolResult.action === "reject") {
    yield cfg.onEvent({ type: "tool.error", name: toolCall.name, error: postToolResult.reason });
    appendMessage(db, { turn, role: "tool", toolCallId: toolCall.id, content: postToolResult.reason });
    appendEvent(db, { type: "tool.error", payload: { tool: toolCall.name, error: postToolResult.reason }, turn });
    return "continue";
  }
  if (postToolResult.action === "pause") {
    yield cfg.onEvent({ type: "paused", reason: postToolResult.reason });
    appendEvent(db, { type: "turn.paused", payload: { reason: postToolResult.reason }, turn });
    return "pause";
  }

  const transformed = extractTransformed(postToolResult);
  if (transformed !== undefined) finalOutput = transformed;

  yield cfg.onEvent({ type: "tool.complete", name: toolCall.name, output: finalOutput, durationMs });
  appendMessage(db, { turn, role: "tool", toolCallId: toolCall.id, content: finalOutput });
  appendEvent(db, { type: "tool.complete", payload: { tool: toolCall.name, durationMs }, turn });
  return "continue";
}
