import type { Database } from "bun:sqlite";

export type PolicyEffect = "allow" | "deny" | "audit" | "confirm";

export interface Policy {
  id: string;
  name: string;
  effect: PolicyEffect;
  priority: number;
  toolPattern: string | null;
  pathPattern: string | null;
  costCondition: string | null;
  argsPattern: string | null;
  actorPattern: string | null;
  message: string | null;
  enabled: number;
}

export interface PolicyDecision {
  effect: PolicyEffect;
  matchedPolicy: Policy | null;
  reason: string;
}

const globCache = new Map<string, RegExp>();
const MAX_GLOB_CACHE = 512;

function getGlobRegex(pattern: string): RegExp {
  let re = globCache.get(pattern);
  if (re) return re;
  if (globCache.size >= MAX_GLOB_CACHE) {
    const firstKey = globCache.keys().next().value;
    if (firstKey !== undefined) globCache.delete(firstKey);
  }
  const escaped = pattern.replace(/[.+^${}()|[\]\\]/g, "\\$&").replace(/\*/g, ".*");
  re = new RegExp(`^${escaped}$`);
  globCache.set(pattern, re);
  return re;
}

function matchesGlob(pattern: string | null, value: string): boolean {
  if (!pattern) return true;
  if (!pattern.includes("*")) return pattern === value;
  return getGlobRegex(pattern).test(value);
}

function matchesCostCondition(condition: string | null, context: { sessionCost?: number; turnCost?: number }): boolean {
  if (!condition) return true;
  const match = condition.match(/^(session_cost|turn_cost)\s*(>|>=|<|<=|==)\s*(\d+\.?\d*)$/);
  if (!match) return false;
  const [, field, op, threshold] = match;
  const value = field === "session_cost" ? (context.sessionCost ?? 0) : (context.turnCost ?? 0);
  const t = parseFloat(threshold!);
  switch (op) {
    case ">": return value > t;
    case ">=": return value >= t;
    case "<": return value < t;
    case "<=": return value <= t;
    case "==": return Math.abs(value - t) < 0.001;
    default: return false;
  }
}

const argsRegexCache = new Map<string, RegExp | null>();
const MAX_ARGS_CACHE = 256;

function getArgsRegex(pattern: string): RegExp | null {
  if (argsRegexCache.has(pattern)) return argsRegexCache.get(pattern)!;
  if (argsRegexCache.size >= MAX_ARGS_CACHE) {
    const firstKey = argsRegexCache.keys().next().value;
    if (firstKey !== undefined) argsRegexCache.delete(firstKey);
  }
  try {
    const re = new RegExp(pattern);
    argsRegexCache.set(pattern, re);
    return re;
  } catch {
    argsRegexCache.set(pattern, null);
    return null;
  }
}

function matchesArgsPattern(pattern: string | null, args: unknown): boolean {
  if (!pattern) return true;
  const re = getArgsRegex(pattern);
  if (!re) return false;
  const serialized = typeof args === "string" ? args : JSON.stringify(args);
  return re.test(serialized);
}

export interface EvaluateContext {
  actor?: string;
  toolName?: string;
  path?: string;
  args?: unknown;
  sessionCost?: number;
  turnCost?: number;
}

export function evaluatePolicy(db: Database, context: EvaluateContext): PolicyDecision {
  const rows = db
    .query(
      `SELECT id, name, effect, priority, tool_pattern as toolPattern, path_pattern as pathPattern,
              cost_condition as costCondition, args_pattern as argsPattern, actor_pattern as actorPattern,
              message, enabled
       FROM policies WHERE enabled = 1 ORDER BY priority DESC`
    )
    .all() as Policy[];

  for (const p of rows) {
    const toolMatch = matchesGlob(p.toolPattern, context.toolName ?? "");
    const pathMatch = matchesGlob(p.pathPattern, context.path ?? "");
    const costMatch = matchesCostCondition(p.costCondition, context);
    const argsMatch = matchesArgsPattern(p.argsPattern, context.args);
    const actorMatch = matchesGlob(p.actorPattern, context.actor ?? "");

    if (toolMatch && pathMatch && costMatch && argsMatch && actorMatch) {
      return {
        effect: p.effect as PolicyEffect,
        matchedPolicy: p,
        reason: p.message ?? `Matched policy: ${p.name}`,
      };
    }
  }

  return { effect: "allow", matchedPolicy: null, reason: "No matching policy; allow by default" };
}
