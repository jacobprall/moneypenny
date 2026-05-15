import { Command } from "commander";
import * as path from "node:path";
import { createAgentDB, closeAgentDB, closeWorkspaceDB, DEFAULT_BLUEPRINT, syncPolicyFiles } from "@moneypenny/db";
import { createHttpApp } from "@moneypenny/http";
import { scan, startScheduler } from "@moneypenny/agents";
import { startBackgroundSync, initSyncTables } from "@moneypenny/cloud";
import { getBlueprintsDir, getDbPath, openWorkspace } from "../session";

export const serveCommand = new Command("serve")
  .description("Run HTTP API + scheduler + background sync")
  .option("--repo <path>", "Repository path", process.cwd())
  .option("--port <n>", "HTTP port", "3123")
  .option("--session <id>", "Agent DB name (ignored, uses mp.db)", "default")
  .action(async (opts: { repo: string; port: string; session: string }) => {
    const repoPath = path.resolve(opts.repo);
    const workspace = openWorkspace(repoPath);
    const dbPath = getDbPath(repoPath);
    const agentDb = createAgentDB(dbPath, {
      repoPath,
      workspace,
      blueprint: DEFAULT_BLUEPRINT,
    });
    const blueprintsDir = getBlueprintsDir(repoPath);
    scan({ agentDb, blueprintsDir });
    syncPolicyFiles(agentDb, path.join(repoPath, ".mp", "policies"));
    try {
      initSyncTables(agentDb.db);
    } catch (e) {
      process.stderr.write(`[warn] initSyncTables failed: ${e instanceof Error ? e.message : String(e)}\n`);
    }

    const stopSched = startScheduler(agentDb, () => process.env.ANTHROPIC_API_KEY, 60_000);
    const stopSync = startBackgroundSync(agentDb.db);

    const port = parseInt(opts.port, 10) || 3123;
    const policiesDir = path.join(repoPath, ".mp", "policies");
    const app = createHttpApp({
      getDb: () => agentDb,
      getApiKey: () => process.env.ANTHROPIC_API_KEY,
      blueprintsDir,
      policiesDir,
      uiDistPath: path.join(repoPath, "ui", "dist"),
    });

    const server = Bun.serve({
      port,
      fetch: (app as any).fetch,
      hostname: "127.0.0.1",
    });

    process.stderr.write(`mp HTTP listening on http://localhost:${String(server.port)}\n`);

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
