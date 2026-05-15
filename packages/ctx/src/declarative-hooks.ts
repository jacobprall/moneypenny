import type { AgentDB } from "@moneypenny/db";
import { globMatch } from "@moneypenny/db/glob";
import type { Hook, HookContext, PostHookResult, PreHookResult } from "./builtin/types.js";

export interface DeclarativeHook {
  id: string;
  name: string;
  phase: "pre_tool" | "post_tool" | "pre_llm" | "post_llm";
  priority: number;
  condition: HookCondition;
  action: HookAction;
  enabled: boolean;
}

export type HookCondition =
  | { type: "tool_name"; pattern: string }
  | { type: "args_match"; jsonpath: string; value: string }
  | { type: "cost_exceeds"; usd: number }
  | { type: "session_turns_exceed"; count: number }
  | { type: "always" };

export type HookAction =
  | { type: "deny"; message: string }
  | { type: "audit"; message: string }
  | { type: "confirm"; message: string }
  | { type: "transform_args"; jsonpath: string; value: string }
  | { type: "inject_context"; content: string };

type PathSegment = string | number;

function parseSimpleJsonPath(path: string): PathSegment[] {
  let s = path.trim();
  if (s.startsWith("$")) {
    s = s.slice(1);
    if (s.startsWith(".")) s = s.slice(1);
  }
  if (!s) return [];
  const out: PathSegment[] = [];
  let pos = 0;
  while (pos < s.length) {
    if (s[pos] === ".") {
      pos++;
      continue;
    }
    if (s[pos] === "[") {
      const close = s.indexOf("]", pos);
      if (close === -1) return out;
      const n = Number(s.slice(pos + 1, close));
      if (!Number.isFinite(n)) return out;
      out.push(n);
      pos = close + 1;
      continue;
    }
    const start = pos;
    while (pos < s.length && !" .[]".includes(s[pos]!)) pos++;
    const key = s.slice(start, pos);
    if (key.length > 0) out.push(key);
  }
  return out;
}

function jsonPathGet(root: unknown, segments: PathSegment[]): unknown {
  let cur: unknown = root;
  for (const seg of segments) {
    if (cur === null || cur === undefined) return undefined;
    if (typeof seg === "number") {
      if (!Array.isArray(cur)) return undefined;
      cur = cur[seg];
    } else {
      if (typeof cur !== "object") return undefined;
      cur = (cur as Record<string, unknown>)[seg];
    }
  }
  return cur;
}

function cloneForMutation(input: unknown): unknown {
  if (input === null || typeof input !== "object") return input;
  try {
    return JSON.parse(JSON.stringify(input)) as unknown;
  } catch {
    return input;
  }
}

function jsonPathSet(root: unknown, segments: PathSegment[], value: unknown): unknown {
  if (segments.length === 0) return value;
  const base = cloneForMutation(root);
  if (typeof base !== "object" || base === null) return base;

  let cur: unknown = base;
  for (let i = 0; i < segments.length - 1; i++) {
    const seg = segments[i]!;
    const next = segments[i + 1]!;
    if (typeof seg === "number") {
      if (!Array.isArray(cur)) return base;
      const arr = cur as unknown[];
      while (arr.length <= seg) arr.push(null);
      if (arr[seg] === null || typeof arr[seg] !== "object" || Array.isArray(arr[seg])) {
        arr[seg] = typeof next === "number" ? [] : {};
      }
      cur = arr[seg];
    } else {
      const o = cur as Record<string, unknown>;
      let child = o[seg];
      if (child === null || typeof child !== "object" || Array.isArray(child)) {
        o[seg] = typeof next === "number" ? [] : {};
      }
      cur = o[seg];
    }
  }

  const last = segments[segments.length - 1]!;
  if (typeof last === "number") {
    if (!Array.isArray(cur)) return base;
    const arr = cur as unknown[];
    while (arr.length <= last) arr.push(null);
    arr[last] = value;
  } else {
    (cur as Record<string, unknown>)[last] = value;
  }
  return base;
}

function coalesceJsonFragment(raw: string): unknown {
  try {
    return JSON.parse(raw) as unknown;
  } catch {
    return raw;
  }
}

function toolConditionInputForPhase(
  phase: DeclarativeHook["phase"],
  toolInput: unknown,
  toolOutput: string,
): unknown {
  if (phase === "post_tool") {
    try {
      return JSON.parse(toolOutput) as unknown;
    } catch {
      return toolOutput;
    }
  }
  return toolInput;
}

export function evaluateCondition(
  condition: HookCondition,
  context: HookContext,
  toolName?: string,
  toolInput?: unknown,
  phase?: DeclarativeHook["phase"],
  toolOutput?: string,
): boolean {
  switch (condition.type) {
    case "always":
      return true;
    case "tool_name": {
      if (toolName === undefined) return false;
      return globMatch(condition.pattern, toolName);
    }
    case "args_match": {
      const effectivePhase = phase ?? "pre_tool";
      const payload =
        effectivePhase === "post_tool" && toolOutput !== undefined
          ? toolConditionInputForPhase("post_tool", toolInput, toolOutput)
          : toolInput;
      const segments = parseSimpleJsonPath(condition.jsonpath);
      if (segments.length === 0) return false;
      const found = jsonPathGet(payload, segments);
      return String(found) === condition.value;
    }
    case "cost_exceeds":
      return context.sessionCostUsd >= condition.usd;
    case "session_turns_exceed":
      return context.turnNumber >= condition.count;
    default:
      return false;
  }
}

export function executeAction(action: HookAction): PreHookResult | PostHookResult {
  switch (action.type) {
    case "deny":
      return { action: "reject", reason: action.message };
    case "confirm":
      return { action: "pause", reason: action.message };
    case "audit":
      console.info(`[declarative-hook audit] ${action.message}`);
      return { action: "continue" };
    case "transform_args":
      return { action: "continue" };
    case "inject_context":
      return { action: "continue", injectedContext: action.content };
  }
}

function isDeclarativePhase(s: string): s is DeclarativeHook["phase"] {
  return s === "pre_tool" || s === "post_tool" || s === "pre_llm" || s === "post_llm";
}

function parseConditionJson(raw: string | null): HookCondition | null {
  if (!raw?.trim()) return null;
  let v: unknown;
  try {
    v = JSON.parse(raw) as unknown;
  } catch {
    return null;
  }
  if (!v || typeof v !== "object") return null;
  const rec = v as Record<string, unknown>;
  const t = rec.type;
  if (t === "always") return { type: "always" };
  if (t === "tool_name" && typeof rec.pattern === "string") {
    return { type: "tool_name", pattern: rec.pattern };
  }
  if (
    t === "args_match" &&
    typeof rec.jsonpath === "string" &&
    typeof rec.value === "string"
  ) {
    return {
      type: "args_match",
      jsonpath: rec.jsonpath,
      value: rec.value,
    };
  }
  if (t === "cost_exceeds" && typeof rec.usd === "number") {
    return { type: "cost_exceeds", usd: rec.usd };
  }
  if (t === "session_turns_exceed" && typeof rec.count === "number") {
    return { type: "session_turns_exceed", count: rec.count };
  }
  return null;
}

function parseActionJson(raw: string | null): HookAction | null {
  if (!raw?.trim()) return null;
  let v: unknown;
  try {
    v = JSON.parse(raw) as unknown;
  } catch {
    return null;
  }
  if (!v || typeof v !== "object") return null;
  const rec = v as Record<string, unknown>;
  const t = rec.type;
  if (t === "deny" && typeof rec.message === "string") {
    return { type: "deny", message: rec.message };
  }
  if (t === "audit" && typeof rec.message === "string") {
    return { type: "audit", message: rec.message };
  }
  if (t === "confirm" && typeof rec.message === "string") {
    return { type: "confirm", message: rec.message };
  }
  if (
    t === "transform_args" &&
    typeof rec.jsonpath === "string" &&
    typeof rec.value === "string"
  ) {
    return {
      type: "transform_args",
      jsonpath: rec.jsonpath,
      value: rec.value,
    };
  }
  if (t === "inject_context" && typeof rec.content === "string") {
    return { type: "inject_context", content: rec.content };
  }
  return null;
}

interface HooksTableRow {
  id: string;
  name: string;
  phase: string;
  priority: number;
  condition: string | null;
  action: string | null;
  enabled: number;
}

export function loadDeclarativeHooks(db: AgentDB): DeclarativeHook[] {
  const rows = db.db
    .query(
      `SELECT id, name, phase, priority, condition, action, enabled
       FROM hooks WHERE enabled = 1 ORDER BY priority DESC`,
    )
    .all() as HooksTableRow[];

  const out: DeclarativeHook[] = [];
  for (const row of rows) {
    if (!isDeclarativePhase(row.phase)) continue;
    const condition = parseConditionJson(row.condition);
    const action = parseActionJson(row.action);
    if (!condition || !action) continue;
    out.push({
      id: row.id,
      name: row.name,
      phase: row.phase,
      priority: row.priority ?? 0,
      condition,
      action,
      enabled: row.enabled === 1,
    });
  }
  return out;
}

function applyTransformArgs(input: unknown, action: HookAction & { type: "transform_args" }): unknown {
  const segments = parseSimpleJsonPath(action.jsonpath);
  if (segments.length === 0) return input;
  const coalesced = coalesceJsonFragment(action.value);
  return jsonPathSet(input, segments, coalesced);
}

export function declarativeHookToHook(decl: DeclarativeHook): Hook {
  const baseName = decl.name || `declarative:${decl.id}`;
  const priority = decl.priority;

  if (decl.phase === "pre_tool") {
    return {
      name: baseName,
      priority,
      async preTool(ctx, toolName, input) {
        if (!decl.enabled) return { action: "continue" };
        if (!evaluateCondition(decl.condition, ctx, toolName, input, "pre_tool")) {
          return { action: "continue" };
        }

        if (decl.action.type === "transform_args") {
          const nextInput = applyTransformArgs(input, decl.action);
          return { action: "continue", input: nextInput };
        }

        const result = executeAction(decl.action);
        if (result.action === "reject" || result.action === "pause") return result;
        if (result.action === "continue" && "injectedContext" in result) return result;
        return { action: "continue" };
      },
    };
  }

  if (decl.phase === "post_tool") {
    return {
      name: baseName,
      priority,
      async postTool(ctx, toolName, output) {
        if (!decl.enabled) return { action: "continue" };
        if (!evaluateCondition(decl.condition, ctx, toolName, undefined, "post_tool", output)) {
          return { action: "continue" };
        }

        if (decl.action.type === "inject_context") {
          return {
            action: "continue",
            transformed: `${output}\n\n${decl.action.content}`,
          };
        }

        const result = executeAction(decl.action);
        if (result.action === "reject" || result.action === "pause") return result;
        return { action: "continue" };
      },
    };
  }

  if (decl.phase === "pre_llm") {
    return {
      name: baseName,
      priority,
      async preLLM(ctx) {
        if (!decl.enabled) return { action: "continue" };
        if (!evaluateCondition(decl.condition, ctx)) return { action: "continue" };

        if (decl.action.type === "inject_context") {
          return { action: "continue", injectedContext: decl.action.content };
        }

        const result = executeAction(decl.action);
        if (result.action === "reject" || result.action === "pause") return result;
        return { action: "continue" };
      },
    };
  }

  return {
    name: baseName,
    priority,
    async postLLM(ctx, responseText) {
      if (!decl.enabled) return { action: "continue" };
      if (!evaluateCondition(decl.condition, ctx)) return { action: "continue" };

      if (decl.action.type === "inject_context") {
        return {
          action: "continue",
          transformed: `${responseText}\n\n${decl.action.content}`,
        };
      }

      const result = executeAction(decl.action);
      if (result.action === "reject" || result.action === "pause") return result;
      return { action: "continue" };
    },
  };
}
