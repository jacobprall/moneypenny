import { Command } from "commander";
import * as path from "node:path";
import { closeAgentDB, closeWorkspaceDB, getEvents } from "@moneypenny/db";
import { openSession, openWorkspace } from "../session";
import { printError } from "../display";

export const eventsCommand = new Command("events").description("Inspect local event log");

eventsCommand
  .option("--repo <path>", "Repository path", process.cwd())
  .option("--session <id>", "Session / agent DB", "default")
  .option("--limit <n>", "Max events", "100")
  .option("--type <name>", "Filter by event type")
  .action((opts: { repo: string; session: string; limit: string; type?: string }) => {
    const repoPath = path.resolve(opts.repo);
    const workspace = openWorkspace(repoPath);
    const db = openSession(repoPath, { session: opts.session, workspace });
    try {
      const limit = parseInt(opts.limit, 10);
      const ev = getEvents(db, {
        limit: Number.isFinite(limit) ? limit : 100,
        type: opts.type,
      });
      console.log(JSON.stringify(ev, null, 2));
    } catch (e) {
      printError(e instanceof Error ? e.message : String(e));
      process.exitCode = 1;
    } finally {
      try { closeAgentDB(db); } catch { /* best effort */ }
      try { closeWorkspaceDB(workspace); } catch { /* best effort */ }
    }
  });
