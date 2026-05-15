import type { ChildLoopFactory, ChildLoopParams, ChildLoopResult, ToolRegistry } from "@moneypenny/tools";
import { createToolRegistry } from "@moneypenny/tools";
import { getConversation, type AgentDB } from "@moneypenny/db";
import {
  definePrompt,
  createHookPipeline,
  costGuard,
  credentialRedactor,
  toolGovernance,
} from "@moneypenny/ctx";
import { createAgentLoop } from "./loop.js";
import type { ProviderName, LLMProvider } from "./provider.js";

export interface CreateChildLoopFactoryConfig {
  /** Shared agent DB for the child loop (conversation, events, compaction). */
  db: AgentDB;
  model: string;
  apiKey: string;
  /** LLM provider name or pre-built instance. Inherited from parent loop. */
  provider?: ProviderName | LLMProvider;
  /** The parent's full tool registry. Child registries are filtered subsets of this. */
  parentRegistry: ToolRegistry;
}

/**
 * Creates a ChildLoopFactory that the delegate tool uses to spawn scoped child agent loops.
 * Lives in agent-loop (not agent-tools) to avoid circular deps.
 */
export function createChildLoopFactory(config: CreateChildLoopFactoryConfig): ChildLoopFactory {
  return {
    async run(params: ChildLoopParams): Promise<ChildLoopResult> {
      const childRegistry = createToolRegistry();
      const parentTools = config.parentRegistry.list();
      const allowed = new Set(params.allowedTools);

      for (const tool of parentTools) {
        if (tool.name === "delegate") continue;
        if (allowed.has(tool.name)) {
          childRegistry.register(tool);
        }
      }

      const childToolDefs = childRegistry.listForLLM();

      const childPrompt = definePrompt({
        sections: [
          {
            name: "skill-instructions",
            placement: "static",
            resolve: () => params.skillInstructions,
          },
          {
            name: "subagent-context",
            placement: "static",
            resolve: () => [
              "You are a focused subagent within the moneypenny system.",
              "Complete the task you are given thoroughly, then provide a clear summary of your findings or actions.",
              "You operate against the same codebase as the parent agent.",
            ].join("\n"),
          },
        ],
        conversationResolver: (db) => getConversation(db, { maxTokens: 4096 }),
        tools: childToolDefs,
      });

      const childHooks = createHookPipeline([
        ...(params.maxCostUsd != null ? [costGuard({ maxCostPerSession: params.maxCostUsd })] : []),
        credentialRedactor(),
        toolGovernance({ allowedTools: params.allowedTools }),
      ]);

      const childLoop = await createAgentLoop({
        model: config.model,
        apiKey: config.apiKey,
        provider: config.provider,
        tools: childRegistry,
        hooks: childHooks,
        ctx: childPrompt,
        repoPath: params.repoPath,
        workingDir: params.workingDir,
        maxIterations: params.maxIterations,
        maxCostPerSession: params.maxCostUsd,
        signal: params.signal,
      });

      let finalContent = "";
      let totalCost = 0;
      let iterations = 0;

      for await (const event of childLoop.run(config.db, params.task)) {
        if (event.type === "turn.complete") {
          totalCost += event.cost.costUsd;
          iterations++;
        }
        if (event.type === "llm.complete" && event.message.content) {
          finalContent = event.message.content;
        }
        if (event.type === "error") {
          throw event.error;
        }
      }

      return {
        content: finalContent || "(subagent produced no output)",
        costUsd: totalCost,
        iterations,
      };
    },
  };
}

