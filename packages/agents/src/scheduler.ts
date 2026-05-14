import cronParser from "cron-parser";
import type { AgentDB } from "@mp/db";
import * as jobsRepo from "./jobs-repo.js";
import * as repo from "./repository.js";
import { AGENT_RUN_OPERATION } from "./operations.js";
import { runAgent } from "./runner.js";

const ACTOR = "scheduler";

export function startScheduler(
  agent: AgentDB,
  getApiKey: () => string | undefined,
  intervalMs = 60_000,
): () => void {
  const tick = async () => {
    const now = Date.now();
    const due = jobsRepo.findDue(agent.db, now);
    for (const job of due) {
      let runId = "";
      try {
        runId = crypto.randomUUID();
        const startedAt = Date.now();
        jobsRepo.insertRun(agent.db, {
          id: runId,
          jobId: job.id,
          startedAt,
          endedAt: null,
          status: "running",
          result: null,
          error: null,
          retryCount: 0,
          createdAt: startedAt,
        });

        if (job.operation === AGENT_RUN_OPERATION) {
          const payload = job.payload ? (JSON.parse(job.payload) as { agent_id?: string }) : {};
          const agentId = payload.agent_id;
          if (!agentId) {
            throw new Error("agents.run job missing agent_id in payload");
          }
          const apiKey = getApiKey();
          if (!apiKey) {
            throw new Error("ANTHROPIC_API_KEY (or configured key) required for scheduled agents");
          }
          const result = await Promise.race([
            runAgent({ agentDb: agent, agentId, apiKey }),
            new Promise<never>((_, reject) => setTimeout(() => reject(new Error("Timeout")), job.timeoutMs)),
          ]);

          const endedAt = Date.now();
          jobsRepo.updateRun(agent.db, runId, {
            endedAt,
            status: "completed",
            result: JSON.stringify(result),
          });
          jobsRepo.updateLastRun(agent.db, job.id, endedAt);
          const next = cronParser.parse(job.schedule, { tz: undefined }).next().toDate().getTime();
          jobsRepo.updateNextRun(agent.db, job.id, next);
        } else {
          throw new Error(`Unsupported job operation: ${job.operation}`);
        }
      } catch (err) {
        const endedAt = Date.now();
        const errorMsg = err instanceof Error ? err.message : String(err);
        if (runId) {
          jobsRepo.updateRun(agent.db, runId, {
            endedAt,
            status: "failed",
            error: errorMsg,
          });
        }
        jobsRepo.updateLastRun(agent.db, job.id, endedAt);
        try {
          const next = cronParser.parse(job.schedule, { tz: undefined }).next().toDate().getTime();
          jobsRepo.updateNextRun(agent.db, job.id, next);
        } catch {
          /* */
        }
      }
    }
  };

  const id = setInterval(tick, intervalMs);
  void tick();

  return () => clearInterval(id);
}
