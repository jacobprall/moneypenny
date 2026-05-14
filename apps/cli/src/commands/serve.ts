import { Command } from "commander";
import * as path from "node:path";
import { createAgentDB, closeAgentDB, closeWorkspaceDB, DEFAULT_BLUEPRINT } from "@swe/db";
import { createHttpApp } from "@swe/http";
import { scan, startScheduler } from "@swe/agents";
import { startBackgroundSync, initSyncTables } from "@swe/cloud";
import { getDbPath, getSweDir, openWorkspace } from "../session";

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
    const agentsDir = path.join(getSweDir(repoPath), "agents");
    scan({ db: agentDb.db, agentsDir });
    try {
      initSyncTables(agentDb.db);
    } catch (e) {
      process.stderr.write(`[warn] initSyncTables failed: ${e instanceof Error ? e.message : String(e)}\n`);
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
      fetch: (app as any).fetch,
      hostname: "127.0.0.1",
    });

    process.stderr.write(`swe HTTP listening on http://localhost:${String(server.port)}\n`);

    const onStop = (): void => {
      stopSched();
      stopSync();
      try { closeAgentDB(agentDb); } catch { /* best effort */ }
      try { closeWorkspaceDB(workspace); } catch { /* best effort */ }
      server.stop();
      process.exit(0);
    };

    process.on("SIGINT", onStop);
    process.on("SIGTERM", onStop);
  });
