export { createAgentLoop } from "./loop.js";
export { createToolServices } from "@moneypenny/tools";
export { createChildLoopFactory } from "./child-loop.js";
export type { CreateChildLoopFactoryConfig } from "./child-loop.js";
export { createProvider, inferProvider } from "./provider.js";
export type { LLMProvider, ProviderName, CompletionParams, StreamEvent } from "./provider.js";
export type {
  AgentLoop,
  AssistantMessage,
  CostInfo,
  LoopConfig,
  LoopEvent,
  LoopErrorCode,
  ToolCallInfo,
  TokenUsage,
} from "./types.js";
export {
  LoopError,
  CostLimitError,
  HookRejectionError,
  ToolExecutionError,
  DEFAULT_MAX_ITERATIONS,
  DEFAULT_MAX_TOKENS,
  DEFAULT_MAX_TOOL_OUTPUT_BYTES,
} from "./types.js";
export { summariseSession } from "./summarise.js";
export type { SummariseConfig, MessagePair } from "./summarise.js";
export { runAutoLabel } from "./auto-label.js";
export type { AutoLabelConfig } from "./auto-label.js";
