import type { AgentDB } from "@moneypenny/db";

import type { Job } from "../jobs-repo.js";

export type JobOperation =
  | "agents.run"
  | "pipeline.run"
  | "index.run"
  | "sync.run"
  | "custom.run";

export interface JobExecutor {
  readonly operation: JobOperation;
  execute(job: Job, runId: string, context: ExecutorContext): Promise<string>;
}

export interface ExecutorContext {
  agentDb: AgentDB;
  getApiKey: () => string | undefined;
  repoPath?: string;
}
