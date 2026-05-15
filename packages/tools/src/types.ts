import type { AnthropicToolDef } from "@moneypenny/ctx";
import { z } from "zod";
import type { ChildLoopFactory } from "./tools/delegate.js";
import type { ToolServices } from "./services.js";

export type { AnthropicToolDef };

export interface ToolDefinition {
  name: string;
  description: string;
  inputSchema: z.ZodType;
  execute: (input: unknown, context: ToolContext) => Promise<string>;
}

export interface ToolContext {
  services: ToolServices;
  repoPath: string;
  workingDir: string;
  signal?: AbortSignal;
  childLoopFactory?: ChildLoopFactory;
}

export interface ToolRegistry {
  register(tool: ToolDefinition): void;
  get(name: string): ToolDefinition | undefined;
  list(): ToolDefinition[];
  listForLLM(): AnthropicToolDef[];
  execute(name: string, input: unknown, context: ToolContext): Promise<string>;
}
