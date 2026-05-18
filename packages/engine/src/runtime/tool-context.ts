import { AsyncLocalStorage } from "node:async_hooks";

interface ToolCallContext {
  sessionId: string;
  runId: string;
}

const store = new AsyncLocalStorage<ToolCallContext>();

export function runInToolContext<T>(ctx: ToolCallContext, fn: () => T): T {
  return store.run(ctx, fn);
}

export function getToolCallingSession(): string | undefined {
  return store.getStore()?.sessionId;
}

export function getToolCallingRunId(): string | undefined {
  return store.getStore()?.runId;
}
