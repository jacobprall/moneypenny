import type { Job } from "../jobs-repo.js";
import { PIPELINE_RUN_OPERATION } from "../operations.js";
import { runAgent } from "../runner.js";
import type { ExecutorContext, JobExecutor, JobOperation } from "./types.js";

export interface PipelineStep {
  name: string;
  action: PipelineAction;
}

export type PipelineAction =
  | { type: "http_fetch"; url: string; method?: string; headers?: Record<string, string> }
  | { type: "transform"; script: string }
  | { type: "index_content"; table: string }
  | { type: "shell"; command: string; timeout?: number }
  | { type: "agent_run"; blueprint: string; input: string };

const INDEX_TABLES = new Set(["skills", "knowledge", "docs"]);

/** Same defence-in-depth shadow list as `@moneypenny/ctx` hook scripts. */
const SHADOWED_GLOBALS = [
  "process",
  "require",
  "import",
  "eval",
  "Function",
  "Bun",
  "Deno",
  "globalThis",
  "window",
  "self",
  "Proxy",
  "Reflect",
  "fetch",
  "XMLHttpRequest",
  "WebSocket",
  "Worker",
  "SharedWorker",
  "importScripts",
  "setTimeout",
  "setInterval",
  "setImmediate",
  "queueMicrotask",
] as const;

const BLOCKED_TRANSFORM = [
  /\bconstructor\b/,
  /\b__proto__\b/,
  /\bprototype\b/,
  /\bgetOwnPropertyDescriptor\b/,
  /\bdefineProperty\b/,
  /\bprocess\b/,
  /\brequire\b/,
  /\bimport\b/,
  /\beval\b/,
  /\bFunction\b/,
  /\bBun\b/,
  /\bDeno\b/,
  /\bglobalThis\b/,
  /\bProxy\b/,
  /\bReflect\b/,
] as const;

const MAX_TRANSFORM_LEN = 50_000;
const UNDEFINED_ARGS = SHADOWED_GLOBALS.map(() => undefined);

function validateTransformScript(script: string): void {
  if (script.length > MAX_TRANSFORM_LEN) {
    throw new Error(`transform script exceeds maximum length (${MAX_TRANSFORM_LEN})`);
  }
  for (const pattern of BLOCKED_TRANSFORM) {
    if (pattern.test(script)) {
      throw new Error(`transform script contains blocked pattern: ${pattern.source}`);
    }
  }
}

function runTransformScript(script: string, input: string): string {
  validateTransformScript(script);
  const fn = new Function(
    ...SHADOWED_GLOBALS,
    "input",
    `"use strict";
    return (function() {
      ${script}
    })();
  `,
  );
  const out = fn(...UNDEFINED_ARGS, input);
  if (out === undefined || out === null) return "";
  return typeof out === "string" ? out : JSON.stringify(out);
}

function normalizeHostname(hostname: string): string {
  return hostname.replace(/^\[|\]$/g, "").toLowerCase();
}

export function isBlockedHttpUrl(urlStr: string): boolean {
  let url: URL;
  try {
    url = new URL(urlStr);
  } catch {
    return true;
  }
  const proto = url.protocol.toLowerCase();
  if (proto !== "http:" && proto !== "https:") return true;
  const host = normalizeHostname(url.hostname);
  if (host === "localhost" || host === "0.0.0.0") return true;
  if (host === "::1" || host === "0:0:0:0:0:0:0:1") return true;
  if (host.startsWith("127.")) return true;
  if (host.startsWith("10.")) return true;
  if (host.startsWith("192.168.")) return true;
  const m = /^172\.(\d+)\./.exec(host);
  if (m) {
    const second = parseInt(m[1]!, 10);
    if (second >= 16 && second <= 31) return true;
  }
  return false;
}

function defaultShellTimeoutMs(jobTimeoutMs: number): number {
  return Math.min(30_000, jobTimeoutMs > 0 ? jobTimeoutMs : 30_000);
}

async function runShell(command: string, cwd: string | undefined, timeoutMs: number): Promise<string> {
  const proc = Bun.spawn(["/bin/sh", "-c", command], {
    cwd: cwd ?? process.cwd(),
    stdout: "pipe",
    stderr: "pipe",
  });
  const killTimer = setTimeout(() => {
    try {
      proc.kill();
    } catch {
      /* */
    }
  }, timeoutMs);
  try {
    const [outBuf, errBuf] = await Promise.all([new Response(proc.stdout).arrayBuffer(), new Response(proc.stderr).arrayBuffer()]);
    const code = await proc.exited;
    const stdout = new TextDecoder().decode(outBuf);
    const stderr = new TextDecoder().decode(errBuf);
    if (code !== 0) {
      throw new Error(`shell exited ${String(code)}: ${stderr || stdout || "(no output)"}`);
    }
    return stdout || stderr || "";
  } finally {
    clearTimeout(killTimer);
  }
}

function parsePipelinePayload(job: Job): PipelineStep[] {
  const raw = job.payload ? (JSON.parse(job.payload) as unknown) : {};
  if (!raw || typeof raw !== "object" || !("steps" in raw)) {
    throw new Error("pipeline.run job missing steps in payload");
  }
  const steps = (raw as { steps: unknown }).steps;
  if (!Array.isArray(steps) || steps.length === 0) {
    throw new Error("pipeline.run steps must be a non-empty array");
  }
  return steps as PipelineStep[];
}

export class PipelineExecutor implements JobExecutor {
  readonly operation: JobOperation = PIPELINE_RUN_OPERATION;

  async execute(job: Job, _runId: string, context: ExecutorContext): Promise<string> {
    const steps = parsePipelinePayload(job);
    let carry = "";

    for (const step of steps) {
      const action = step.action;
      if (!action || typeof action !== "object" || !("type" in action)) {
        throw new Error(`pipeline step "${step.name}" missing action`);
      }

      switch (action.type) {
        case "http_fetch": {
          if (isBlockedHttpUrl(action.url)) {
            throw new Error(`http_fetch blocked URL (localhost / private): ${action.url}`);
          }
          const method = (action.method ?? "GET").toUpperCase();
          const res = await fetch(action.url, {
            method,
            headers: action.headers,
          });
          carry = await res.text();
          break;
        }
        case "transform":
          carry = runTransformScript(action.script, carry);
          break;
        case "shell": {
          const t = action.timeout ?? defaultShellTimeoutMs(job.timeoutMs);
          carry = await runShell(action.command, context.repoPath, t);
          break;
        }
        case "agent_run": {
          const apiKey = context.getApiKey();
          if (!apiKey) {
            throw new Error("ANTHROPIC_API_KEY (or configured key) required for pipeline agent_run");
          }
          const agentId = action.blueprint;
          const input = action.input.replace(/\{\{input\}\}/g, carry);
          const result = await runAgent({
            agentDb: context.agentDb,
            agentId,
            apiKey,
            userTurn: input,
          });
          carry = JSON.stringify(result);
          break;
        }
        case "index_content": {
          if (!INDEX_TABLES.has(action.table)) {
            throw new Error(`index_content: table must be one of: ${[...INDEX_TABLES].join(", ")}`);
          }
          carry = `index_content placeholder for table=${action.table}; input length=${carry.length}`;
          break;
        }
        default:
          throw new Error(`unsupported pipeline action: ${String((action as { type: string }).type)}`);
      }
    }

    return carry;
  }
}
