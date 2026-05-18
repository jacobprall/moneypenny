import type { Database } from "bun:sqlite";
import type { ToolRegistry } from "../tools/registry.js";
import type { ToolContext } from "../tools/types.js";

export type DispatchToolCall = {
  id: string;
  name: string;
  args: unknown;
};

export type DispatchResult =
  | { ok: true; value: unknown }
  | { error: string; details?: unknown; message?: string };

type PolicyRow = {
  name: string;
  effect: string;
  conditions: string | null;
};

let cachedPolicies: PolicyRow[] | null = null;
let policyCacheTime = 0;
const POLICY_CACHE_TTL_MS = 30_000;

function loadActivePolicies(db: Database): PolicyRow[] {
  const now = Date.now();
  if (cachedPolicies && now - policyCacheTime < POLICY_CACHE_TTL_MS) {
    return cachedPolicies;
  }
  cachedPolicies = db
    .query<PolicyRow, []>(
      `SELECT name, effect, conditions FROM policies WHERE enabled = 1`,
    )
    .all();
  policyCacheTime = now;
  return cachedPolicies;
}

function checkPolicies(
  db: Database,
  toolName: string,
  toolCategory: string,
): PolicyRow | null {
  const policies = loadActivePolicies(db);
  for (const p of policies) {
    if (p.effect !== "deny" && p.effect !== "warn") continue;
    if (!p.conditions) continue;
    let cond: { tools?: string[]; categories?: string[]; paths?: string[] };
    try {
      cond = JSON.parse(p.conditions);
    } catch {
      continue;
    }
    if (cond.tools && cond.tools.includes(toolName)) return p;
    if (cond.categories && cond.categories.includes(toolCategory)) return p;
  }
  return null;
}

export function invalidatePolicyCache(): void {
  cachedPolicies = null;
  policyCacheTime = 0;
}

export async function dispatchTool(
  call: DispatchToolCall,
  ctx: ToolContext,
  registry: ToolRegistry,
): Promise<DispatchResult> {
  const tool = registry.get(call.name);
  if (!tool) return { error: "TOOL_NOT_FOUND" };

  const parsed = tool.inputSchema.safeParse(call.args);
  if (!parsed.success) return { error: "INVALID_ARGS", details: parsed.error };

  const denied = checkPolicies(ctx.readDb, call.name, tool.category);
  if (denied) {
    if (denied.effect === "deny") {
      ctx.events.emit({
        type: "policy.blocked",
        session_id: ctx.sessionId,
        run_id: ctx.runId,
        detail: {
          tool_call_id: call.id,
          tool: call.name,
          policy: denied.name,
        },
      });
      return {
        error: "POLICY_DENIED",
        message: `Policy "${denied.name}" denies tool "${call.name}"`,
      };
    }
    if (denied.effect === "warn") {
      ctx.events.emit({
        type: "policy.warned",
        session_id: ctx.sessionId,
        run_id: ctx.runId,
        detail: {
          tool_call_id: call.id,
          tool: call.name,
          policy: denied.name,
        },
      });
    }
  }

  ctx.events.emit({
    type: "tool.started",
    session_id: ctx.sessionId,
    run_id: ctx.runId,
    detail: { tool_call_id: call.id, name: call.name, args: parsed.data },
  });
  const start = performance.now();

  try {
    const value = await tool.execute(parsed.data, ctx);
    const ms = performance.now() - start;
    ctx.events.emit({
      type: "tool.completed",
      session_id: ctx.sessionId,
      run_id: ctx.runId,
      detail: {
        tool_call_id: call.id,
        duration_ms: ms,
        result_size: JSON.stringify(value).length,
      },
    });
    return { ok: true, value };
  } catch (err) {
    ctx.events.emit({
      type: "tool.failed",
      session_id: ctx.sessionId,
      run_id: ctx.runId,
      detail: { tool_call_id: call.id, error: String(err) },
    });
    return { error: "TOOL_ERROR", message: String(err) };
  }
}
