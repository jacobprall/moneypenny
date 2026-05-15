import type { Job } from "../jobs-repo.js";
import { CUSTOM_RUN_OPERATION } from "../operations.js";
import type { ExecutorContext, JobExecutor, JobOperation } from "./types.js";

export type CustomJobHandler = (
  params: Record<string, unknown>,
  context: ExecutorContext,
) => Promise<string>;

const handlers = new Map<string, CustomJobHandler>();

export function registerCustomJobHandler(name: string, handler: CustomJobHandler): void {
  handlers.set(name, handler);
}

export function getCustomJobHandlers(): ReadonlyMap<string, CustomJobHandler> {
  return handlers;
}

export class CustomExecutor implements JobExecutor {
  readonly operation: JobOperation = CUSTOM_RUN_OPERATION;

  async execute(job: Job, _runId: string, context: ExecutorContext): Promise<string> {
    const payload = job.payload
      ? (JSON.parse(job.payload) as { handler?: string; params?: Record<string, unknown> })
      : {};
    const name = payload.handler;
    if (!name || typeof name !== "string") {
      throw new Error("custom.run job missing handler name in payload");
    }
    const fn = handlers.get(name);
    if (!fn) {
      throw new Error(`custom.run: no handler registered for "${name}"`);
    }
    const params = payload.params && typeof payload.params === "object" ? payload.params : {};
    return fn(params, context);
  }
}
