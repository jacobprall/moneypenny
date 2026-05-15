import type { Database } from "bun:sqlite";
import type { OperationContext } from "./op-context.js";
import { append } from "./gov-events.js";
import { evaluatePolicy, type PolicyDecision } from "./policy.js";
import { runHooks, getPrePhases, getPostPhases } from "./hooks.js";

export interface Operation<TInput = unknown, TOutput = unknown> {
  name: string;
  execute(ctx: OperationContext, input: TInput): Promise<TOutput>;
}

export interface OperationRegistryOptions {
  denyByDefault?: boolean;
}

export class OperationRegistry {
  private readonly ops = new Map<string, Operation>();
  private readonly denyByDefault: boolean;

  constructor(options?: OperationRegistryOptions) {
    this.denyByDefault = options?.denyByDefault ?? false;
  }

  register<TInput = unknown, TOutput = unknown>(op: Operation<TInput, TOutput>): void {
    if (this.ops.has(op.name)) {
      throw new Error(`Operation already registered: ${op.name}`);
    }
    this.ops.set(op.name, op as Operation);
  }

  get(name: string): Operation | undefined {
    return this.ops.get(name);
  }

  list(): string[] {
    return Array.from(this.ops.keys());
  }

  async execute<TInput = unknown, TOutput = unknown>(
    name: string,
    input: TInput,
    options: ExecuteOptions
  ): Promise<TOutput> {
    return executeOp(this.ops, this.denyByDefault, name, input, options);
  }
}

/** Default global registry — preserved for backward compatibility. */
const defaultRegistry = new OperationRegistry();

export function register<TInput = unknown, TOutput = unknown>(
  op: Operation<TInput, TOutput>
): void {
  defaultRegistry.register(op);
}

export function get(name: string): Operation | undefined {
  return defaultRegistry.get(name);
}

export function list(): string[] {
  return defaultRegistry.list();
}

import type { AgentDB } from "@moneypenny/db";

export interface ExecuteOptions {
  db: AgentDB;
  actor: string;
  sessionId?: string;
  denyByDefault?: boolean;
  resource?: string;
}

function runPreHooks(
  db: AgentDB,
  name: string,
  actor: string,
  sessionId: string | undefined,
  input: unknown
): unknown {
  let currentInput = input;
  for (const phase of getPrePhases()) {
    const result = runHooks(db, phase, name, actor, sessionId, currentInput);
    if (result.aborted) {
      throw new Error(result.reason ?? "Pre-hook aborted");
    }
    currentInput = result.input;
  }
  return currentInput;
}

function decidePolicy(
  db: AgentDB,
  actor: string,
  resource: string,
  currentInput: unknown,
  denyByDefault: boolean,
): PolicyDecision {
  const decision = evaluatePolicy(db, {
    actor,
    toolName: resource,
    path: resource,
    args: currentInput,
  });
  if (decision.effect === "allow" && decision.matchedPolicy === null && denyByDefault) {
    return {
      effect: "deny",
      matchedPolicy: null,
      reason: "No matching policy; deny by default",
    };
  }
  return decision;
}

function runPostHooks(
  db: AgentDB,
  name: string,
  actor: string,
  sessionId: string | undefined,
  input: unknown,
  output: unknown
): { output: unknown; error?: string } {
  let currentOutput = output;
  let error: string | undefined;
  for (const phase of getPostPhases()) {
    const result = runHooks(db, phase, name, actor, sessionId, input, currentOutput);
    if (result.aborted) {
      error = result.reason ?? "Post-hook aborted";
      break;
    }
    if (result.output !== undefined) currentOutput = result.output;
  }
  return { output: currentOutput, error };
}

function appendEvent(
  db: AgentDB,
  name: string,
  actor: string,
  sessionId: string | undefined,
  input: unknown,
  output: unknown,
  error: string | undefined,
  durationMs: number,
  decision: PolicyDecision
): void {
  const eventInput = {
    ...(typeof input === "object" && input !== null ? (input as object) : { _: input }),
    _policy:
      decision.effect === "audit" || decision.effect === "confirm"
        ? { effect: decision.effect, reason: decision.reason }
        : undefined,
  };
  append(db, {
    id: crypto.randomUUID(),
    operation: name,
    actor,
    sessionId,
    input: eventInput,
    output,
    error,
    durationMs,
    createdAt: Date.now(),
  });
}

async function executeOp<TInput = unknown, TOutput = unknown>(
  ops: Map<string, Operation>,
  registryDenyByDefault: boolean,
  name: string,
  input: TInput,
  options: ExecuteOptions,
): Promise<TOutput> {
  const op = ops.get(name);
  if (!op) throw new Error(`Unknown operation: ${name}`);

  const ctx: OperationContext = {
    db: options.db,
    actor: options.actor,
    sessionId: options.sessionId,
  };

  const denyByDefault = options.denyByDefault ?? registryDenyByDefault;
  const resource = options.resource ?? name;

  const currentInput = runPreHooks(
    options.db,
    name,
    options.actor,
    options.sessionId,
    input
  ) as TInput;

  const decision = decidePolicy(options.db, options.actor, resource, currentInput, denyByDefault);

  if (decision.effect === "deny") {
    const error = `Policy denied: ${decision.reason}`;
    appendEvent(
      options.db,
      name,
      options.actor,
      options.sessionId,
      currentInput,
      undefined,
      error,
      0,
      decision
    );
    throw new Error(error);
  }

  const start = Date.now();
  let output: TOutput;
  try {
    output = await op.execute(ctx, currentInput) as TOutput;
  } catch (e) {
    const error = e instanceof Error ? e.message : String(e);
    appendEvent(
      options.db,
      name,
      options.actor,
      options.sessionId,
      currentInput,
      undefined,
      error,
      Date.now() - start,
      decision
    );
    throw e;
  }

  const { output: finalOutput, error } = runPostHooks(
    options.db,
    name,
    options.actor,
    options.sessionId,
    currentInput,
    output
  );

  appendEvent(
    options.db,
    name,
    options.actor,
    options.sessionId,
    currentInput,
    finalOutput,
    error,
    Date.now() - start,
    decision
  );

  return finalOutput as TOutput;
}

/**
 * Execute an operation: pre-hooks → policy → run → post-hooks → append event.
 * Returns the operation output. Throws on error or policy deny.
 */
export async function execute<TInput = unknown, TOutput = unknown>(
  name: string,
  input: TInput,
  options: ExecuteOptions
): Promise<TOutput> {
  return defaultRegistry.execute(name, input, options);
}
