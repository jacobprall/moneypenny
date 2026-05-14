import type { AgentDB } from "@swe/db";
import { createAgentLoop, type LoopEvent } from "@swe/loop";
import { definePrompt, createHookPipeline, costGuard, credentialRedactor, toolGovernance, dbPolicyHook } from "@swe/ctx";
import { createToolRegistry, registerBuiltinTools } from "@swe/tools";
import * as repo from "./repository.js";
import { frontmatterSchema, type AgentFrontmatter } from "./schema.js";

function registerBuiltinToolsFiltered(registry: ReturnType<typeof createToolRegistry>, allow?: string[]): void {
  const tmp = createToolRegistry();
  registerBuiltinTools(tmp);
  for (const t of tmp.list()) {
    if (allow != null && allow.length > 0 && !allow.includes(t.name)) continue;
    registry.register(t);
  }
}

export interface RunAgentOptions {
  agentDb: AgentDB;
  agentId: string;
  apiKey: string;
  model?: string;
}

export async function runAgent(options: RunAgentOptions): Promise<{ events: LoopEvent[] }> {
  const { agentDb, agentId, apiKey } = options;
  const row = repo.getById(agentDb.db, agentId);
  if (!row || row.status === "deleted") {
    throw new Error(`Agent not found: ${agentId}`);
  }
  if (!row.enabled) {
    throw new Error(`Agent disabled: ${agentId}`);
  }

  let config: AgentFrontmatter;
  try {
    const raw = JSON.parse(row.configJson) as unknown;
    config = frontmatterSchema.parse(raw);
  } catch (e) {
    throw new Error(`Invalid agent config for ${agentId}: ${e instanceof Error ? e.message : String(e)}`);
  }

  const model = options.model ?? config.model ?? "claude-sonnet-4-20250514";
  const agentPrompt = row.prompt;
  const repoPath = agentDb.repoPath;

  const registry = createToolRegistry();
  registerBuiltinToolsFiltered(registry, config.tools?.length ? config.tools : undefined);

  const agentCtx = definePrompt({
    sections: [
      {
        name: "agent-instructions",
        placement: "static",
        resolve: (_db, _ctx) => agentPrompt,
      },
    ],
    tools: registry.listForLLM(),
  });

  const maxCostSession = config.max_cost_per_session ?? 5.0;
  const maxCostTurn = config.max_cost_per_turn;
  const hooks = createHookPipeline([
    costGuard({ maxCostPerSession: maxCostSession, maxCostPerTurn: maxCostTurn }),
    credentialRedactor(),
    dbPolicyHook({ db: () => agentDb.db }),
    ...(config.permissions?.deny?.length
      ? [toolGovernance({ deniedTools: config.permissions.deny })]
      : []),
  ]);

  const loop = await createAgentLoop({
    model,
    apiKey,
    tools: registry,
    hooks,
    ctx: agentCtx,
    repoPath,
    maxIterations: config.max_turns,
  });

  const events: LoopEvent[] = [];
  const userTurn = "Proceed with the above instructions.";
  for await (const event of loop.run(agentDb, userTurn)) {
    events.push(event);
  }
  return { events };
}
