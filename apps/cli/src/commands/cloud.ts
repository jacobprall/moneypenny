import { Command } from "commander";
import * as path from "node:path";
import { closeAgentDB, createAgentDB, closeWorkspaceDB, DEFAULT_BLUEPRINT } from "@swe/db";
import { getSyncConfig, runCloudSync, setSyncConfig, initSyncTables, hasCloudsync } from "@swe/cloud";
import { getDbPath, openWorkspace } from "../session";
import { printError } from "../display";

export const cloudCommand = new Command("cloud").description("Cloud sync (sqlite-sync)");

cloudCommand
  .command("init")
  .description("Initialize cloudsync tables and store server URL")
  .requiredOption("--url <url>", "Cloud sync server URL")
  .option("--repo <path>", "Repository path", process.cwd())
  .option("--session <id>", "Agent DB name", "default")
  .action((opts: { url: string; repo: string; session: string }) => {
    const repoPath = path.resolve(opts.repo);
    const workspace = openWorkspace(repoPath);
    const dbPath = getDbPath(repoPath, opts.session);
    const agentDb = createAgentDB(dbPath, { repoPath, workspace, blueprint: DEFAULT_BLUEPRINT });
    try {
      const n = initSyncTables(agentDb.db);
      setSyncConfig(agentDb.db, "cloud_url", opts.url);
      process.stdout.write(`Initialized ${String(n)} sync tables; cloud_url set.\n`);
    } catch (e) {
      printError(e instanceof Error ? e.message : String(e));
      process.exitCode = 1;
    } finally {
      try {
        closeAgentDB(agentDb);
        closeWorkspaceDB(workspace);
      } catch {
        /* */
      }
    }
  });

cloudCommand
  .command("login")
  .description("Placeholder for future auth flow")
  .action(() => {
    process.stdout.write("swe cloud login is not yet implemented for this MVP.\n");
  });

cloudCommand
  .command("sync")
  .description("Run a one-shot sync with the configured cloud URL")
  .option("--repo <path>", "Repository path", process.cwd())
  .option("--session <id>", "Agent DB name", "default")
  .action(async (opts: { repo: string; session: string }) => {
    const repoPath = path.resolve(opts.repo);
    const workspace = openWorkspace(repoPath);
    const dbPath = getDbPath(repoPath, opts.session);
    const agentDb = createAgentDB(dbPath, { repoPath, workspace, blueprint: DEFAULT_BLUEPRINT });
    try {
      if (!hasCloudsync(agentDb.db)) {
        printError("sqlite-sync extension not available on this database.");
        process.exitCode = 1;
        return;
      }
      const cfg = getSyncConfig(agentDb.db);
      if (!cfg.cloudUrl) {
        printError("No cloud_url configured. Run: swe cloud init --url <url>");
        process.exitCode = 1;
        return;
      }
      const result = await runCloudSync(agentDb.db, cfg.cloudUrl);
      console.log(JSON.stringify(result, null, 2));
    } catch (e) {
      printError(e instanceof Error ? e.message : String(e));
      process.exitCode = 1;
    } finally {
      try {
        closeAgentDB(agentDb);
        closeWorkspaceDB(workspace);
      } catch {
        /* */
      }
    }
  });
