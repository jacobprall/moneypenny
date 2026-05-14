import type {
  Hook,
  HookContext,
  PostHookResult,
  PreHookResult,
  RedactorConfig,
} from "./types.js";

const DEFAULT_PATTERNS: RegExp[] = [
  /sk-[a-zA-Z0-9]{20,}/g,
  /ghp_[a-zA-Z0-9]{36}/g,
  /ghs_[a-zA-Z0-9]{36}/g,
  /github_pat_[a-zA-Z0-9_]{82}/g,
  /glpat-[a-zA-Z0-9\-_]{20,}/g,
  /-----BEGIN[A-Z ]*PRIVATE KEY-----[\s\S]*?-----END[A-Z ]*PRIVATE KEY-----/g,
  /xox[bpors]-[a-zA-Z0-9\-]{10,}/g,
  /AKIA[0-9A-Z]{16}/g,
  /npm_[a-zA-Z0-9]{36}/g,
  /(?:postgres|postgresql|mysql|mongodb|mongodb\+srv|redis|amqp):\/\/[^\s:]+:[^\s@]+@[^\s"']+/g,
  /Bearer\s+[A-Za-z0-9\-._~+/]+=*/g,
  /AIza[0-9A-Za-z\-_]{35}/g,
  /sk-ant-[a-zA-Z0-9\-_]{20,}/g,
  /goog_[a-zA-Z0-9\-_]{30,}/g,
  /SG\.[a-zA-Z0-9\-_]{22}\.[a-zA-Z0-9\-_]{43}/g,
];

function compilePatterns(patterns: RegExp[]): RegExp[] {
  return patterns.map((p) => new RegExp(p.source, p.flags));
}

function applyPatterns(
  text: string,
  compiled: RegExp[],
  replacement: string,
): string {
  let out = text;
  for (const re of compiled) {
    out = out.replace(re, replacement);
  }
  return out;
}

/**
 * Mutates `obj` in-place, replacing string values that match credential
 * patterns. Returns true if any value was changed. The in-place mutation
 * is intentional: preTool hooks receive the input by reference before the
 * tool executes, so modifications propagate without a return channel.
 */
function redactObject(
  obj: unknown,
  compiled: RegExp[],
  replacement: string,
): boolean {
  if (typeof obj !== "object" || obj === null) return false;
  const record = obj as Record<string, unknown>;
  let changed = false;
  for (const key of Object.keys(record)) {
    const val = record[key];
    if (typeof val === "string") {
      const redacted = applyPatterns(val, compiled, replacement);
      if (redacted !== val) {
        record[key] = redacted;
        changed = true;
      }
    } else if (typeof val === "object" && val !== null) {
      if (redactObject(val, compiled, replacement)) changed = true;
    }
  }
  return changed;
}

export function credentialRedactor(config?: RedactorConfig): Hook {
  const compiled = compilePatterns(config?.patterns ?? DEFAULT_PATTERNS);
  const replacement = config?.replacement ?? "[REDACTED]";

  function redact(text: string): PostHookResult {
    const redacted = applyPatterns(text, compiled, replacement);
    return redacted === text
      ? { action: "continue" }
      : { action: "continue", transformed: redacted };
  }

  return {
    name: "credential-redactor",
    async preLLM(_ctx: HookContext): Promise<PreHookResult> {
      return { action: "continue" };
    },
    async postLLM(
      _ctx: HookContext,
      responseText: string,
    ): Promise<PostHookResult> {
      return redact(responseText);
    },
    async preTool(
      _ctx: HookContext,
      _toolName: string,
      input: unknown,
    ): Promise<PreHookResult> {
      redactObject(input, compiled, replacement);
      return { action: "continue" };
    },
    async postTool(
      _ctx: HookContext,
      _toolName: string,
      output: string,
    ): Promise<PostHookResult> {
      return redact(output);
    },
  };
}
