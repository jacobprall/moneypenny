import cronParser from "cron-parser";
import type { AgentDB } from "@moneypenny/db";
import * as jobsRepo from "./jobs-repo.js";
import type { Job } from "./jobs-repo.js";
import { createExecutorRegistry } from "./executors/index.js";
import type { ExecutorContext } from "./executors/types.js";

async function runDueJob(
  agent: AgentDB,
  job: Job,
  getApiKey: () => string | undefined,
  executors: ReturnType<typeof createExecutorRegistry>,
): Promise<void> {
  let runId = "";
  try {
    runId = crypto.randomUUID();
    const startedAt = Date.now();
    agent.writer.exclusive((db) => {
      jobsRepo.insertRun(db, {
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
    });

    const executor = executors.get(job.operation);
    if (!executor) {
      throw new Error(`Unsupported job operation: ${job.operation}`);
    }
    const context: ExecutorContext = {
      agentDb: agent,
      getApiKey,
      repoPath: agent.repoPath,
    };
    const result = await Promise.race([
      executor.execute(job, runId, context),
      new Promise<never>((_, reject) => setTimeout(() => reject(new Error("Timeout")), job.timeoutMs)),
    ]);

    const endedAt = Date.now();
    agent.writer.exclusive((db) => {
      jobsRepo.updateRun(db, runId, {
        endedAt,
        status: "completed",
        result,
      });
      jobsRepo.updateLastRun(db, job.id, endedAt);
      const next = cronParser.parse(job.schedule, { tz: undefined }).next().toDate().getTime();
      jobsRepo.updateNextRun(db, job.id, next);
    });
  } catch (err) {
    const endedAt = Date.now();
    const errorMsg = err instanceof Error ? err.message : String(err);
    if (runId) {
      agent.writer.exclusive((db) => {
        jobsRepo.updateRun(db, runId, {
          endedAt,
          status: "failed",
          error: errorMsg,
        });
      });
    }
    agent.writer.exclusive((db) => {
      jobsRepo.updateLastRun(db, job.id, endedAt);
      try {
        const next = cronParser.parse(job.schedule, { tz: undefined }).next().toDate().getTime();
        jobsRepo.updateNextRun(db, job.id, next);
      } catch {
        /* */
      }
    });
  }
}

export function startScheduler(
  agent: AgentDB,
  getApiKey: () => string | undefined,
  intervalMs = 60_000,
): () => void {
  const executors = createExecutorRegistry();
  const tick = async () => {
    const now = Date.now();
    const due = agent.reads.read((raw) => jobsRepo.findDue(raw, now));
    for (const job of due) {
      await runDueJob(agent, job, getApiKey, executors);
    }
  };

  const id = setInterval(tick, intervalMs);
  void tick();

  return () => clearInterval(id);
}

export async function triggerJobById(
  agent: AgentDB,
  jobId: string,
  getApiKey: () => string | undefined,
): Promise<void> {
  const job = agent.reads.read((raw) => jobsRepo.getById(raw, jobId));
  if (!job) {
    throw new Error("Job not found");
  }
  const executors = createExecutorRegistry();
  await runDueJob(agent, job, getApiKey, executors);
}
