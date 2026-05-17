import type { HookDefinition } from "./hooks.js";

const CREDENTIAL_PATTERNS = [
  /(?:api[_-]?key|secret|token|password|credential|auth)['":\s]*[=:]\s*['"][^'"]{8,}['"]/gi,
  /(?:sk-|pk_|rk_|ghp_|gho_|github_pat_|xoxb-|xoxp-|Bearer\s+)\S{10,}/g,
  /(?:AKIA|ASIA)[A-Z0-9]{16}/g,
];

export const credentialRedactor: HookDefinition = {
  name: "credential-redactor",
  phase: "pre-llm",
  priority: 1,
  fn: async (ctx) => {
    if (!ctx.messages) return;

    const redacted = ctx.messages.map((m) => {
      let content = m.content;
      for (const pattern of CREDENTIAL_PATTERNS) {
        content = content.replace(pattern, "[REDACTED]");
      }
      return { ...m, content };
    });

    return { ...ctx, messages: redacted };
  },
};

export const operationLogger: HookDefinition = {
  name: "operation-logger",
  phase: "post-tool",
  priority: 50,
  fn: async (ctx) => {
    const duration = (ctx as any).durationMs;
    if (ctx.toolName && duration != null) {
      console.error(
        `  [op] ${ctx.toolName}: ${duration.toFixed(0)}ms ${(ctx as any).error ? "ERR" : "OK"}`,
      );
    }
  },
};

export const budgetEnforcer: HookDefinition = {
  name: "budget-enforcer",
  phase: "pre-llm",
  priority: 5,
  fn: async (ctx) => {
    if (ctx.costUsd != null && ctx.costUsd > 0) {
      const total = (ctx as any).dailyTotal ?? 0;
      const limit = (ctx as any).dailyLimit ?? 10;
      if (total >= limit) {
        throw new Error(`Daily budget exceeded: $${total.toFixed(2)}/$${limit}`);
      }
    }
  },
};
