import type { Database } from "bun:sqlite";
import { runAgentOnce, recordTurn } from "./agent.js";

export interface PoolConfig {
  db: Database;
  defaultModel: string;
  maxConcurrent?: number;
}

interface AgentJob {
  id: string;
  agentName: string;
  model: string;
  task: string;
  sessionId: string;
}

interface AgentResult {
  jobId: string;
  sessionId: string;
  agentName: string;
  response: string;
  costUsd: number;
  durationMs: number;
  error?: string;
}

export class AgentPool {
  private db: Database;
  private defaultModel: string;
  private maxConcurrent: number;
  private running = 0;
  private queue: AgentJob[] = [];
  private results = new Map<string, AgentResult>();

  constructor(config: PoolConfig) {
    this.db = config.db;
    this.defaultModel = config.defaultModel;
    this.maxConcurrent = config.maxConcurrent ?? 3;
  }

  async submit(
    agentName: string,
    task: string,
    model?: string,
  ): Promise<string> {
    const sessionId = crypto.randomUUID();
    const jobId = crypto.randomUUID();

    const agentDef = this.db
      .query<{ model: string | null }, [string]>(
        "SELECT model FROM agent_defs WHERE name = ?",
      )
      .get(agentName);

    const job: AgentJob = {
      id: jobId,
      agentName,
      model: model ?? agentDef?.model ?? this.defaultModel,
      task,
      sessionId,
    };

    this.db.query(
      `INSERT INTO sessions (id, agent_name, created_at, last_active_at, is_active)
       VALUES (?, ?, unixepoch(), unixepoch(), 1)`,
    ).run(sessionId, agentName);

    this.queue.push(job);
    this.drain();
    return jobId;
  }

  private async drain(): Promise<void> {
    while (this.queue.length > 0 && this.running < this.maxConcurrent) {
      const job = this.queue.shift()!;
      this.running++;
      this.runJob(job).finally(() => {
        this.running--;
        this.drain();
      });
    }
  }

  private async runJob(job: AgentJob): Promise<void> {
    const start = performance.now();
    const messages: Array<{ role: "user" | "assistant"; content: string }> = [
      { role: "user", content: job.task },
    ];

    recordTurn(this.db, job.sessionId, "user", job.task);

    try {
      const result = await runAgentOnce(
        {
          db: this.db,
          model: job.model,
          agentName: job.agentName,
          maxSteps: 10,
        },
        messages,
      );

      const text = result.text;
      const usage = result.usage;

      recordTurn(this.db, job.sessionId, "assistant", text, job.model, {
        promptTokens: usage.promptTokens,
        completionTokens: usage.completionTokens,
      });

      this.db.query(
        "UPDATE sessions SET is_active = 0, last_active_at = unixepoch() WHERE id = ?",
      ).run(job.sessionId);

      const costRow = this.db
        .query<{ cost: number }, [string]>(
          "SELECT COALESCE(SUM(cost_usd), 0) as cost FROM messages WHERE session_id = ?",
        )
        .get(job.sessionId);

      this.results.set(job.id, {
        jobId: job.id,
        sessionId: job.sessionId,
        agentName: job.agentName,
        response: text,
        costUsd: costRow?.cost ?? 0,
        durationMs: performance.now() - start,
      });

      this.db.query(
        `INSERT INTO events (type, agent_name, session_id, detail, created_at)
         VALUES ('pool.complete', ?, ?, json_object('job_id', ?, 'cost_usd', ?), unixepoch())`,
      ).run(job.agentName, job.sessionId, job.id, costRow?.cost ?? 0);
    } catch (err) {
      this.db.query(
        "UPDATE sessions SET is_active = 0, last_active_at = unixepoch() WHERE id = ?",
      ).run(job.sessionId);

      this.results.set(job.id, {
        jobId: job.id,
        sessionId: job.sessionId,
        agentName: job.agentName,
        response: "",
        costUsd: 0,
        durationMs: performance.now() - start,
        error: err instanceof Error ? err.message : String(err),
      });
    }
  }

  getResult(jobId: string): AgentResult | undefined {
    return this.results.get(jobId);
  }

  status(): {
    running: number;
    queued: number;
    completed: number;
  } {
    return {
      running: this.running,
      queued: this.queue.length,
      completed: this.results.size,
    };
  }

  async runTriggeredAgents(trigger: string, context?: string): Promise<string[]> {
    const agents = this.db
      .query<{ name: string; model: string | null }, [string]>(
        "SELECT name, model FROM agent_defs WHERE trigger_on = ?",
      )
      .all(trigger);

    const jobIds: string[] = [];
    for (const agent of agents) {
      const task = context ?? `Triggered by: ${trigger}`;
      const jobId = await this.submit(agent.name, task, agent.model ?? undefined);
      jobIds.push(jobId);
    }
    return jobIds;
  }

  async processScheduledJobs(): Promise<number> {
    const jobs = this.db
      .query<
        { id: string; agent_name: string; action: string | null },
        []
      >(
        `SELECT j.id, j.agent_name, j.action FROM jobs j
         WHERE j.enabled = 1 AND j.schedule IS NOT NULL
         AND (j.last_run_at IS NULL
              OR j.last_run_at < unixepoch() - 3600)`,
      )
      .all();

    let count = 0;
    for (const job of jobs) {
      const task = job.action ?? "Run scheduled maintenance";
      await this.submit(job.agent_name, task);
      this.db.query("UPDATE jobs SET last_run_at = unixepoch() WHERE id = ?").run(
        job.id,
      );
      count++;
    }
    return count;
  }
}
