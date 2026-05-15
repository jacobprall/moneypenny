import type { AgentDB } from "@moneypenny/db";
import { declarativeHookToHook, loadDeclarativeHooks } from "../declarative-hooks.js";
import type {
  Hook,
  HookContext,
  HookPipeline,
  PostHookResult,
  PreHookResult,
} from "./types.js";

export interface PipelineOptions {
  /** Max ms a single hook invocation may take before being treated as a rejection. Default: 30000. */
  hookTimeoutMs?: number;
}

const DEFAULT_HOOK_TIMEOUT_MS = 30_000;

function isShortCircuit(result: PreHookResult | PostHookResult): boolean {
  return result.action === "reject" || result.action === "pause";
}

function withTimeout<T>(promise: Promise<T>, ms: number, hookName: string): Promise<T> {
  if (ms <= 0) return promise;
  return new Promise<T>((resolve, reject) => {
    const timer = setTimeout(
      () => reject(new Error(`Hook "${hookName}" timed out after ${ms}ms`)),
      ms,
    );
    promise.then(
      (v) => { clearTimeout(timer); resolve(v); },
      (e) => { clearTimeout(timer); reject(e); },
    );
  });
}

async function runPreLLMPhase(
  hooks: Hook[],
  invoke: (hook: Hook) => Promise<PreHookResult> | undefined,
  timeoutMs: number,
): Promise<PreHookResult> {
  const injections: string[] = [];
  for (const hook of hooks) {
    const promise = invoke(hook);
    if (!promise) continue;
    let result: PreHookResult;
    try {
      result = await withTimeout(promise, timeoutMs, hook.name);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      return { action: "reject", reason: `Hook "${hook.name}" failed: ${msg}` };
    }
    if (isShortCircuit(result)) return result;
    if (result.action === "continue" && result.injectedContext?.trim()) {
      injections.push(result.injectedContext.trim());
    }
  }
  return injections.length > 0
    ? { action: "continue", injectedContext: injections.join("\n\n") }
    : { action: "continue" };
}

async function runPreToolPhase(
  hooks: Hook[],
  invoke: (hook: Hook, input: unknown) => Promise<PreHookResult> | undefined,
  initialInput: unknown,
  timeoutMs: number,
): Promise<PreHookResult> {
  let currentInput = initialInput;
  let replaced = false;
  for (const hook of hooks) {
    const promise = invoke(hook, currentInput);
    if (!promise) continue;
    let result: PreHookResult;
    try {
      result = await withTimeout(promise, timeoutMs, hook.name);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      return { action: "reject", reason: `Hook "${hook.name}" failed: ${msg}` };
    }
    if (isShortCircuit(result)) return result;
    if (result.action === "continue" && "input" in result && result.input !== undefined) {
      currentInput = result.input;
      replaced = true;
    }
  }
  return replaced ? { action: "continue", input: currentInput } : { action: "continue" };
}

async function runTransformPhase(
  hooks: Hook[],
  invoke: (hook: Hook, text: string) => Promise<PostHookResult> | undefined,
  text: string,
  timeoutMs: number,
): Promise<PostHookResult> {
  let current = text;
  let mutated = false;
  for (const hook of hooks) {
    const promise = invoke(hook, current);
    if (!promise) continue;
    let result: PostHookResult;
    try {
      result = await withTimeout(promise, timeoutMs, hook.name);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      return { action: "reject", reason: `Hook "${hook.name}" failed: ${msg}` };
    }
    if (isShortCircuit(result)) return result;
    if (result.action === "continue" && "transformed" in result) {
      current = result.transformed;
      mutated = true;
    }
  }
  return mutated
    ? { action: "continue", transformed: current }
    : { action: "continue" };
}

export function createHookPipeline(hooks: Hook[], options?: PipelineOptions): HookPipeline {
  const timeoutMs = options?.hookTimeoutMs ?? DEFAULT_HOOK_TIMEOUT_MS;

  return {
    runPreLLM(context: HookContext) {
      return runPreLLMPhase(hooks, (h) => h.preLLM?.(context), timeoutMs);
    },
    runPostLLM(context: HookContext, responseText: string) {
      return runTransformPhase(
        hooks,
        (h, text) => h.postLLM?.(context, text),
        responseText,
        timeoutMs,
      );
    },
    runPreTool(context: HookContext, toolName: string, input: unknown) {
      return runPreToolPhase(
        hooks,
        (h, inp) => h.preTool?.(context, toolName, inp),
        input,
        timeoutMs,
      );
    },
    runPostTool(context: HookContext, toolName: string, output: string) {
      return runTransformPhase(
        hooks,
        (h, text) => h.postTool?.(context, toolName, text),
        output,
        timeoutMs,
      );
    },
  };
}

export function createHookPipelineWithDeclarative(
  codeHooks: Hook[],
  db: AgentDB,
  options?: PipelineOptions,
): HookPipeline {
  const declarativeHooks = loadDeclarativeHooks(db);
  const converted = declarativeHooks.map(declarativeHookToHook);
  const allHooks = [...codeHooks, ...converted].sort(
    (a, b) => (b.priority ?? 0) - (a.priority ?? 0),
  );
  return createHookPipeline(allHooks, options);
}
