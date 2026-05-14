import { Command } from "commander";
import * as path from "node:path";
import { closeAgentDB, closeWorkspaceDB } from "@moneypenny/db";
import { listAgentRows, runAgent } from "@moneypenny/agents";
import { openSession, openWorkspace } from "../session";
import { printError } from "../display";

export const agentsCliCommand = new Command("agents").description("Inspect and run scheduled agents");

agentsCliCommand
  .command("list")
  .description("List agents loaded in the local database")
  .option("--repo <path>", "Repository path", process.cwd())
  .option("--session <id>", "Session / agent DB", "default")
  .action((opts: { repo: string; session: string }) => {
    const repoPath = path.resolve(opts.repo);
    const workspace = openWorkspace(repoPath);
    const db = openSession(repoPath, { session: opts.session, workspace });
    try {
      const rows = listAgentRows(db.db).filter((r) => r.status !== "deleted");
      console.log(JSON.stringify(rows, null, 2));
    } finally {
      try { closeAgentDB(db); } catch { /* best effort */ }
      try { closeWorkspaceDB(workspace); } catch { /* best effort */ }
    }
  });

agentsCliCommand
  .command("run")
  .description("Run an agent by id")
  .argument("<id>", "Agent id")
  .option("--repo <path>", "Repository path", process.cwd())
  .option("--session <id>", "Session / agent DB", "default")
  .option("--model <model>", "Model override")
  .action(async (id: string, opts: { repo: string; session: string; model?: string }) => {
    const repoPath = path.resolve(opts.repo);
    const workspace = openWorkspace(repoPath);
    const db = openSession(repoPath, { session: opts.session, workspace });
    try {
      const apiKey = process.env.ANTHROPIC_API_KEY;
      if (!apiKey) {
        printError("ANTHROPIC_API_KEY is required");
        process.exitCode = 1;
        return;
      }
      const out = await runAgent({
        agentDb: db,
        agentId: id,
        apiKey,
        model: opts.model,
      });
      console.log(JSON.stringify(out, null, 2));
    } catch (e) {
      printError(e instanceof Error ? e.message : String(e));
      process.exitCode = 1;
    } finally {
      try { closeAgentDB(db); } catch { /* best effort */ }
      try { closeWorkspaceDB(workspace); } catch { /* best effort */ }
    }
  });

agentsCliCommand
  .command("logs")
  .description("Recent job runs (best-effort)")
  .option("--repo <path>", "Repository path", process.cwd())
  .option("--session <id>", "Session / agent DB", "default")
  .option("--agent <id>", "Filter by agent id")
  .option("--limit <n>", "Max rows", "30")
  .action((opts: { repo: string; session: string; agent?: string; limit: string }) => {
    const repoPath = path.resolve(opts.repo);
    const workspace = openWorkspace(repoPath);
    const db = openSession(repoPath, { session: opts.session, workspace });
    try {
      const lim = Math.min(500, Math.max(1, parseInt(opts.limit, 10) || 30));
      const rows = db.db
        .query(
          `SELECT r.id, r.job_id as jobId, r.started_at as startedAt, r.ended_at as endedAt,
                  r.status, r.error, j.payload
           FROM job_runs r
           LEFT JOIN jobs j ON j.id = r.job_id
           ORDER BY r.started_at DESC
           LIMIT ?`,
        )
        .all(lim) as Array<{
        id: string;
        jobId: string;
        startedAt: number;
        endedAt: number | null;
        status: string;
        error: string | null;
        payload: string | null;
      }>;

      const filtered = rows.filter((r) => {
        if (!opts.agent) return true;
        try {
          const p = r.payload ? JSON.parse(r.payload) : {};
          return (p as { agent_id?: string }).agent_id === opts.agent;
        } catch {
          return false;
        }
      });

      console.log(JSON.stringify(filtered, null, 2));
    } finally {
      try { closeAgentDB(db); } catch { /* best effort */ }
      try { closeWorkspaceDB(workspace); } catch { /* best effort */ }
    }
  });
