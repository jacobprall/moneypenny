import { Command } from "commander";
import * as path from "node:path";
import { createAgentDB, closeWorkspaceDB, DEFAULT_BLUEPRINT } from "@mp/db";
import { createHttpApp } from "@mp/http";
import { scan, startScheduler } from "@mp/agents";
import { startBackgroundSync, initSyncTables } from "@mp/cloud";
import { getDbPath, getMoneypennyDir, openWorkspace } from "../session";

export const serveCommand = new Command("serve")
  .description("Run HTTP API + scheduler + background sync")
  .option("--repo <path>", "Repository path", process.cwd())
  .option("--port <n>", "HTTP port", "3123")
  .option("--session <id>", "Agent DB name", "default")
  .action(async (opts: { repo: string; port: string; session: string }) => {
    const repoPath = path.resolve(opts.repo);
    const workspace = openWorkspace(repoPath);
    const dbPath = getDbPath(repoPath, opts.session);
    const agentDb = createAgentDB(dbPath, {
      repoPath,
      workspace,
      blueprint: DEFAULT_BLUEPRINT,
    });
    const agentsDir = path.join(getMoneypennyDir(repoPath), "agents");
    scan({ db: agentDb.db, agentsDir });
    try {
      initSyncTables(agentDb.db);
    } catch {
      /* */
    }

    const stopSched = startScheduler(agentDb, () => process.env.ANTHROPIC_API_KEY, 60_000);
    const stopSync = startBackgroundSync(agentDb.db);

    const port = parseInt(opts.port, 10) || 3123;
    const app = createHttpApp({
      getDb: () => agentDb,
      getApiKey: () => process.env.ANTHROPIC_API_KEY,
      agentsDir,
      uiDistPath: path.join(repoPath, "ui", "dist"),
    });

    const server = Bun.serve({
      port,
      fetch: app.fetch,
    });

    process.stderr.write(`Moneypenny HTTP listening on http://localhost:${String(server.port)}\n`);

    const onStop = (): void => {
      stopSched();
      stopSync();
      try {
        closeWorkspaceDB(workspace);
      } catch {
        /* */
      }
      server.stop();
      process.exit(0);
    };

    process.on("SIGINT", onStop);
    process.on("SIGTERM", onStop);
  });
