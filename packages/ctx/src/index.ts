// Context assembly
export { definePrompt } from "./assemble.js";
export type {
  MaybePromise,
  ContentBlock,
  AnthropicSystemBlock,
  Section,
  AssemblyContext,
  AnthropicMessage,
  AnthropicContentBlock,
  AnthropicToolDef,
  ConversationResolver,
  PromptConfig,
  SectionWithPriority,
  AssembleResult,
  Prompt,
} from "./types.js";
export { formatConversation, normalizeToBlocks } from "./format.js";

// Governance pipeline
export { register, get, list, execute, type Operation, type ExecuteOptions } from "./operations.js";
export { evaluatePolicy, type Policy, type PolicyDecision, type PolicyEffect, type EvaluateContext } from "./policy.js";
export { runHooks, getPrePhases, getPostPhases, type HookContext as DbHookContext, type HookResult as DbHookResult } from "./hooks.js";
export { append as appendGovEvent, query as queryGovEvents, type NewEvent as GovNewEvent, type Event as GovEvent } from "./gov-events.js";
export type { OperationContext } from "./op-context.js";

// Built-in hook pipeline
export type { Hook, HookPipeline, HookContext, PreHookResult, PostHookResult, CostGuardConfig, RedactorConfig, GovernanceConfig, ConfirmationConfig } from "./builtin/types.js";
export { createHookPipeline, type PipelineOptions } from "./builtin/pipeline.js";
export { costGuard } from "./builtin/cost-guard.js";
export { credentialRedactor } from "./builtin/credential-redactor.js";
export { toolGovernance } from "./builtin/tool-governance.js";
export { confirmationGate } from "./builtin/confirmation-gate.js";
export { dbPolicyHook, type DbPolicyHookConfig } from "./builtin/db-policy-hook.js";
