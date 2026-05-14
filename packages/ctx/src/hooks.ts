/**
 * Hook execution — run pre/post hooks from DB.
 * Hooks sync via CRDT; phases: pre:validation, pre:injection, post:transform, etc.
 */

import type { Database } from "bun:sqlite";
import { BoundedMap } from "./bounded-map.js";
import { compileUserRegex } from "./safe-regex.js";

export interface HookContext {
  operation: string;
  actor: string;
  sessionId?: string;
  phase: string;
  input: unknown;
  output?: unknown;
}

export type HookAction = "continue" | "abort" | "mutate";

export interface HookResult {
  action: HookAction;
  input?: unknown;
  output?: unknown;
  reason?: string;
}

interface HookRow {
  id: string;
  name: string;
  phase: string;
  matchPattern: string;
  priority: number;
  script: string;
}

const hookMatchCache = new BoundedMap<string, RegExp | null>(256);

function matchesOperation(pattern: string, operation: string): boolean {
  let re = hookMatchCache.get(pattern);
  if (re === undefined) {
    re = compileUserRegex(pattern);
    hookMatchCache.set(pattern, re);
  }
  return re ? re.test(operation) : false;
}

const MAX_HOOK_SCRIPT_LENGTH = 10_000;

/**
 * Globals explicitly shadowed as `undefined` inside the Function constructor
 * to block access to runtime APIs. This is defence-in-depth — not a full sandbox.
 */
const SHADOWED_GLOBALS = [
  "process", "require", "import", "eval", "Function",
  "Bun", "Deno", "globalThis", "window", "self",
  "Proxy", "Reflect",
  "fetch", "XMLHttpRequest", "WebSocket",
  "Worker", "SharedWorker", "importScripts",
  "setTimeout", "setInterval", "setImmediate", "queueMicrotask",
] as const;

const BLOCKED_PATTERNS = [
  /\bconstructor\b/,
  /\b__proto__\b/,
  /\bprototype\b/,
  /\bgetOwnPropertyDescriptor\b/,
  /\bdefineProperty\b/,
  /\bprocess\b/,
  /\brequire\b/,
  /\bimport\b/,
  /\beval\b/,
  /\bFunction\b/,
  /\bBun\b/,
  /\bDeno\b/,
  /\bglobalThis\b/,
  /\bProxy\b/,
  /\bReflect\b/,
];

function validateHookScript(script: string): void {
  if (script.length > MAX_HOOK_SCRIPT_LENGTH) {
    throw new Error(`Hook script exceeds maximum length of ${MAX_HOOK_SCRIPT_LENGTH}`);
  }
  for (const pattern of BLOCKED_PATTERNS) {
    if (pattern.test(script)) {
      throw new Error(`Hook script contains blocked keyword: ${pattern.source}`);
    }
  }
}

const UNDEFINED_ARGS = SHADOWED_GLOBALS.map(() => undefined);

function runHookScript(script: string, ctx: HookContext): HookResult {
  validateHookScript(script);

  const frozenCtx = Object.freeze({ ...ctx });

  const fn = new Function(
    ...SHADOWED_GLOBALS,
    "ctx",
    `"use strict";
    const { operation, actor, sessionId, phase, input, output } = ctx;
    return (function() {
      ${script}
    })();
  `
  );
  const result = fn(...UNDEFINED_ARGS, frozenCtx);
  if (result && typeof result === "object" && "action" in result) {
    const action = result.action;
    if (action !== "continue" && action !== "abort" && action !== "mutate") {
      return { action: "continue" };
    }
    return result as HookResult;
  }
  return { action: "continue" };
}

export function runHooks(
  db: Database,
  phase: string,
  operation: string,
  actor: string,
  sessionId: string | undefined,
  input: unknown,
  output?: unknown
): { input: unknown; output?: unknown; aborted: boolean; reason?: string } {
  const rows = db
    .query(
      `SELECT id, name, phase, match_pattern as matchPattern, priority, script
       FROM hooks WHERE enabled = 1 AND phase = ? ORDER BY priority DESC`
    )
    .all(phase) as HookRow[];

  let currentInput = input;
  let currentOutput = output;
  let aborted = false;
  let abortReason: string | undefined;

  for (const row of rows) {
    if (!matchesOperation(row.matchPattern, operation)) continue;

    const ctx: HookContext = {
      operation,
      actor,
      sessionId,
      phase,
      input: currentInput,
      output: currentOutput,
    };

    try {
      const result = runHookScript(row.script, ctx);
      if (result.action === "abort") {
        aborted = true;
        abortReason = result.reason ?? "Hook aborted";
        break;
      }
      if (result.action === "mutate") {
        if (result.input !== undefined) currentInput = result.input;
        if (result.output !== undefined) currentOutput = result.output;
      }
    } catch (e) {
      aborted = true;
      abortReason = e instanceof Error ? e.message : String(e);
      break;
    }
  }

  return {
    input: currentInput,
    output: currentOutput,
    aborted,
    reason: abortReason,
  };
}

export function getPrePhases(): string[] {
  return ["pre:validation", "pre:injection"];
}

export function getPostPhases(): string[] {
  return ["post:transform"];
}
