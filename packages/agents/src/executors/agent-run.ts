import type { Job } from "../jobs-repo.js";
import { AGENT_RUN_OPERATION } from "../operations.js";
import { runAgent } from "../runner.js";
import type { ExecutorContext, JobExecutor, JobOperation } from "./types.js";

export class AgentRunExecutor implements JobExecutor {
  readonly operation: JobOperation = AGENT_RUN_OPERATION;

  async execute(job: Job, _runId: string, context: ExecutorContext): Promise<string> {
    const payload = job.payload ? (JSON.parse(job.payload) as { agent_id?: string }) : {};
    const agentId = payload.agent_id;
    if (!agentId) {
      throw new Error("agents.run job missing agent_id in payload");
    }
    const apiKey = context.getApiKey();
    if (!apiKey) {
      throw new Error("ANTHROPIC_API_KEY (or configured key) required for scheduled agents");
    }
    const result = await runAgent({ agentDb: context.agentDb, agentId, apiKey });
    return JSON.stringify(result);
  }
}
