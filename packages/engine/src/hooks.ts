import type { Database } from "bun:sqlite";

export type HookPhase =
  | "pre-llm"
  | "post-llm"
  | "pre-tool"
  | "post-tool"
  | "pre-turn"
  | "post-turn";

export interface HookContext {
  phase: HookPhase;
  agentName: string;
  sessionId: string;
  model?: string;
  toolName?: string;
  input?: unknown;
  output?: unknown;
  messages?: Array<{ role: string; content: string }>;
  usage?: { promptTokens: number; completionTokens: number };
  costUsd?: number;
}

export type HookFn = (ctx: HookContext) => Promise<HookContext | void>;

export interface HookDefinition {
  name: string;
  phase: HookPhase;
  fn: HookFn;
  priority?: number;
}

export class HookPipeline {
  private hooks: Map<HookPhase, HookDefinition[]> = new Map();

  register(hook: HookDefinition): void {
    const existing = this.hooks.get(hook.phase) ?? [];
    existing.push(hook);
    existing.sort((a, b) => (a.priority ?? 100) - (b.priority ?? 100));
    this.hooks.set(hook.phase, existing);
  }

  async run(ctx: HookContext): Promise<HookContext> {
    const hooks = this.hooks.get(ctx.phase) ?? [];
    let current = ctx;
    for (const hook of hooks) {
      try {
        const result = await hook.fn(current);
        if (result) current = result;
      } catch (err) {
        console.error(
          `Hook '${hook.name}' (${hook.phase}) failed:`,
          err instanceof Error ? err.message : err,
        );
      }
    }
    return current;
  }

  list(): Array<{ name: string; phase: HookPhase; priority: number }> {
    const result: Array<{ name: string; phase: HookPhase; priority: number }> = [];
    for (const [phase, hooks] of this.hooks) {
      for (const h of hooks) {
        result.push({ name: h.name, phase, priority: h.priority ?? 100 });
      }
    }
    return result;
  }
}

export function createDefaultHooks(db: Database): HookPipeline {
  const pipeline = new HookPipeline();

  pipeline.register({
    name: "cost-logger",
    phase: "post-llm",
    priority: 10,
    fn: async (ctx) => {
      if (ctx.costUsd != null && ctx.costUsd > 0) {
        db.query(
          `INSERT INTO events (type, agent_name, session_id, detail, created_at)
           VALUES ('hook.cost', ?, ?, json_object('cost_usd', ?, 'model', ?), unixepoch())`,
        ).run(
          ctx.agentName,
          ctx.sessionId,
          ctx.costUsd,
          ctx.model ?? "unknown",
        );
      }
    },
  });

  pipeline.register({
    name: "tool-logger",
    phase: "post-tool",
    priority: 10,
    fn: async (ctx) => {
      db.query(
        `INSERT INTO events (type, agent_name, session_id, detail, created_at)
         VALUES ('hook.tool', ?, ?, json_object('tool', ?, 'success', ?), unixepoch())`,
      ).run(
        ctx.agentName,
        ctx.sessionId,
        ctx.toolName ?? "unknown",
        ctx.output ? 1 : 0,
      );
    },
  });

  return pipeline;
}
