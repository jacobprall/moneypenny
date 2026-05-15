import type { Job } from "../jobs-repo.js";
import { SYNC_RUN_OPERATION } from "../operations.js";
import type { ExecutorContext, JobExecutor, JobOperation } from "./types.js";

export class SyncExecutor implements JobExecutor {
  readonly operation: JobOperation = SYNC_RUN_OPERATION;

  async execute(job: Job, _runId: string, _context: ExecutorContext): Promise<string> {
    const payload = job.payload
      ? (JSON.parse(job.payload) as { target?: string })
      : {};
    const target = payload.target ?? "cloud";
    return `Sync run (placeholder): would sync to target=${String(target)} (cloud | team).`;
  }
}
