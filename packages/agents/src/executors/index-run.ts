import type { Job } from "../jobs-repo.js";
import { INDEX_RUN_OPERATION } from "../operations.js";
import type { ExecutorContext, JobExecutor, JobOperation } from "./types.js";

export class IndexExecutor implements JobExecutor {
  readonly operation: JobOperation = INDEX_RUN_OPERATION;

  async execute(job: Job, _runId: string, _context: ExecutorContext): Promise<string> {
    const payload = job.payload
      ? (JSON.parse(job.payload) as { scope?: string })
      : {};
    const scope = payload.scope ?? "incremental";
    return `Index run (placeholder): would re-index codebase with scope=${String(scope)} (full | incremental | stale).`;
  }
}
