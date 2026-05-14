import { sqlError } from "./errors";
import type { AgentDB } from "./types";
import { generateUUIDv7 } from "./uuid";

export type PolicyEffect = "allow" | "deny" | "audit" | "confirm";

const ACTOR_KEY = "__mp_actor";

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
  createdAt: number;
  updatedAt: number;
}

export interface PolicyDecision {
  effect: PolicyEffect;
  matchedPolicy: Policy | null;
  reason: string;
}

const selectFields = `id, name, effect, priority,
  tool_pattern AS toolPattern,
  path_pattern AS pathPattern,
  cost_condition AS costCondition,
  args_pattern AS argsPattern,
  actor_pattern AS actorPattern,
  message, enabled, created_at AS createdAt, updated_at AS updatedAt`;

function rowToPolicy(r: Record<string, unknown>): Policy {
  return {
    id: r.id as string,
    name: r.name as string,
    effect: r.effect as PolicyEffect,
    priority: Number(r.priority),
    toolPattern: (r.toolPattern as string | null) ?? null,
    pathPattern: (r.pathPattern as string | null) ?? null,
    costCondition: (r.costCondition as string | null) ?? null,
    argsPattern: (r.argsPattern as string | null) ?? null,
    actorPattern: (r.actorPattern as string | null) ?? null,
    message: (r.message as string | null) ?? null,
    enabled: Number(r.enabled),
    createdAt: Number(r.createdAt),
    updatedAt: Number(r.updatedAt),
  };
}

export function listPolicies(db: AgentDB): Policy[] {
  try {
    const rows = db.db.prepare(`SELECT ${selectFields} FROM policies ORDER BY priority DESC, created_at ASC`).all() as Record<
      string,
      unknown
    >[];
    return rows.map(rowToPolicy);
  } catch (e) {
    throw sqlError("listPolicies", e);
  }
}

export function getPolicy(db: AgentDB, id: string): Policy | null {
  try {
    const row = db.db.prepare(`SELECT ${selectFields} FROM policies WHERE id = ?`).get(id) as Record<string, unknown> | undefined;
    return row ? rowToPolicy(row) : null;
  } catch (e) {
    throw sqlError("getPolicy", e);
  }
}

export type CreatePolicyInput = Omit<Policy, "id" | "createdAt" | "updatedAt" | "enabled"> & {
  id?: string;
  createdAt?: number;
  updatedAt?: number;
  enabled?: number;
};

export function createPolicy(db: AgentDB, input: CreatePolicyInput): Policy {
  const now = Date.now();
  const createdAt = input.createdAt ?? now;
  const updatedAt = input.updatedAt ?? now;
  const id = input.id ?? generateUUIDv7();
  try {
    db.db
      .prepare(
        `INSERT INTO policies (id, name, effect, priority, tool_pattern, path_pattern, cost_condition, args_pattern, actor_pattern, message, enabled, created_at, updated_at)
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)`,
      )
      .run(
        id,
        input.name,
        input.effect,
        input.priority ?? 0,
        input.toolPattern ?? null,
        input.pathPattern ?? null,
        input.costCondition ?? null,
        input.argsPattern ?? null,
        input.actorPattern ?? null,
        input.message ?? null,
        input.enabled ?? 1,
        createdAt,
        updatedAt,
      );
  } catch (e) {
    throw sqlError("createPolicy", e);
  }
  return {
    id,
    name: input.name,
    effect: input.effect,
    priority: input.priority ?? 0,
    toolPattern: input.toolPattern ?? null,
    pathPattern: input.pathPattern ?? null,
    costCondition: input.costCondition ?? null,
    argsPattern: input.argsPattern ?? null,
    actorPattern: input.actorPattern ?? null,
    message: input.message ?? null,
    enabled: input.enabled ?? 1,
    createdAt,
    updatedAt,
  };
}

export function updatePolicy(db: AgentDB, id: string, updates: Partial<Omit<Policy, "id" | "createdAt">>): Policy {
  const cur = getPolicy(db, id);
  if (!cur) throw new Error(`Policy not found: ${id}`);
  const next: Policy = {
    ...cur,
    ...updates,
    updatedAt: Date.now(),
  };
  try {
    db.db
      .prepare(
        `UPDATE policies SET name=?, effect=?, priority=?, tool_pattern=?, path_pattern=?, cost_condition=?, args_pattern=?, actor_pattern=?, message=?, enabled=?, updated_at=? WHERE id=?`,
      )
      .run(
        next.name,
        next.effect,
        next.priority,
        next.toolPattern,
        next.pathPattern,
        next.costCondition,
        next.argsPattern,
        next.actorPattern,
        next.message,
        next.enabled,
        next.updatedAt,
        id,
      );
  } catch (e) {
    throw sqlError("updatePolicy", e);
  }
  return next;
}

export function deletePolicy(db: AgentDB, id: string): void {
  try {
    db.db.prepare(`DELETE FROM policies WHERE id = ?`).run(id);
  } catch (e) {
    throw sqlError("deletePolicy", e);
  }
}
