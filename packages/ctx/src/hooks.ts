/**
 * @deprecated This module contained legacy `Function()` constructor hook execution.
 * Schema v10 removed the `script` and `match_pattern` columns.
 * Use declarative hooks (`loadDeclarativeHooks` / `createHookPipelineWithDeclarative`) instead.
 *
 * All exports are retained as no-ops for backward compatibility.
 */

import type { AgentDB } from "@moneypenny/db";

/** @deprecated Use HookContext from builtin/types.ts */
export interface HookContext {
  operation: string;
  actor: string;
  sessionId?: string;
  phase: string;
  input: unknown;
  output?: unknown;
}

/** @deprecated */
export type HookAction = "continue" | "abort" | "mutate";

/** @deprecated */
export interface HookResult {
  action: HookAction;
  input?: unknown;
  output?: unknown;
  reason?: string;
}

/** @deprecated No-op. Use `createHookPipelineWithDeclarative` instead. */
export function runHooks(
  _db: AgentDB,
  _phase: string,
  _operation: string,
  _actor: string,
  _sessionId: string | undefined,
  input: unknown,
  output?: unknown,
): { input: unknown; output?: unknown; aborted: boolean; reason?: string } {
  return { input, output, aborted: false };
}

/** @deprecated Use HookPipeline phases: 'pre_tool', 'pre_llm'. */
export function getPrePhases(): string[] {
  return ["pre_tool", "pre_llm"];
}

/** @deprecated Use HookPipeline phases: 'post_tool', 'post_llm'. */
export function getPostPhases(): string[] {
  return ["post_tool", "post_llm"];
}
