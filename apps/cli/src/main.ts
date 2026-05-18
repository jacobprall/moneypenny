import { mkdir } from "node:fs/promises";
import { join } from "node:path";

import type { Database } from "bun:sqlite";

import {
  detectConventions,
  embedChunks,
  setLlmDatabase,
  configureLlm,
} from "@moneypenny/engine";

import { startStdioServer } from "@moneypenny/mcp";

import { migrateOnly } from "./db-migrate.js";
import {
  openWriteDb,
  openAiDb,
  migrateV1ToV2,
  migrateV2,
} from "@moneypenny/db";

import { syncAllConfigs } from "./config.js";
import { indexDirectory } from "./indexer.js";
import { resolvePaths } from "./paths.js";
import { loadRootConfig, scaffoldRootConfig } from "./root-config.js";
import { scaffoldConfig } from "./scaffold.js";
import { bootstrapMcpContext, runHttpServer } from "./startup.js";

const RESET = "\x1b[0m";
const BOLD = "\x1b[1m";

async function wireLlmForCli(
  writeDb: Database,
  repoRoot: string,
  aiDb: Database,
): Promise<void> {
  setLlmDatabase(aiDb);
  const saved = writeDb
    .query<{ value: string }, [string]>(
      `SELECT value FROM config WHERE key = ?`,
    )
    .get("llm.config");
  if (saved) {
    try {
      configureLlm(JSON.parse(saved.value) as Parameters<typeof configureLlm>[0]);
    } catch (e) {
      console.error("[llm] failed to parse saved config:", e instanceof Error ? e.message : e);
    }
  }
  const rootCfg = await loadRootConfig(repoRoot);
  if (rootCfg.models) {
    const sai = rootCfg.models.sqlite_ai;
    configureLlm({
      strong: rootCfg.models.strong,
      fast: rootCfg.models.fast,
      local: rootCfg.models.local || undefined,
      ollamaBaseUrl: rootCfg.models.ollama_base_url,
      sqliteAi: sai
        ? {
            modelsDir: sai.models_dir || undefined,
            contextSize: sai.context_size,
            nPredict: sai.n_predict,
            nThreads: sai.n_threads,
            gpuLayers: sai.gpu_layers,
          }
        : undefined,
    });
  }
}

async function migrateCommand(paths: ReturnType<typeof resolvePaths>): Promise<void> {
  await mkdir(paths.dataDir, { recursive: true });
  await mkdir(paths.extensionsDir, { recursive: true });
  const applied = await migrateOnly(paths);
  if (applied > 0) {
    console.log(`migrations: applied ${applied} v2 file(s)`);
  } else {
    console.log("migrations: already up to date");
  }
}

function printUsage(): void {
  console.log("moneypenny v2 — local-first agent runtime\n");
  console.log("Usage: mp <command>\n");
  console.log(`${BOLD}Core:${RESET}`);
  console.log("  start              HTTP API + web UI + background runtime");
  console.log("  serve              MCP server (stdio)");
  console.log("  migrate            Apply DB migrations and exit");
  console.log("  chat               (see message — web UI is primary)");
  console.log("");
  console.log(`${BOLD}Tooling:${RESET}`);
  console.log("  index [path]       Index codebase into code_chunks");
  console.log("  embed [batch]      Embed pending chunks (engine/schema dependent)");
  console.log("  detect             Detect conventions via LLM");
  console.log("  custodian          Notes for v2 maintenance");
}

async function main(): Promise<void> {
  const args = process.argv.slice(2);
  const command = args[0] ?? "start";
  const paths = resolvePaths(process.cwd());

  switch (command) {
    case "start": {
      console.log("moneypenny v2 — starting");
      await runHttpServer(paths);
      await new Promise(() => {});
      break;
    }

    case "serve": {
      const ctx = await bootstrapMcpContext(paths);
      await startStdioServer(ctx);
      break;
    }

    case "migrate": {
      await migrateCommand(paths);
      break;
    }

    case "chat": {
      const port = process.env.MP_PORT ?? "4966";
      console.log(
        `Interactive REPL is replaced by the web UI in v2. Open http://127.0.0.1:${port} (run "mp start").`,
      );
      break;
    }

    case "index": {
      const force = args.includes("--force");
      const repoRoot =
        args.find((a, i) => i > 0 && !a.startsWith("-")) ?? process.cwd();
      const p = resolvePaths(repoRoot);
      await mkdir(p.dataDir, { recursive: true });
      const writeDb = openWriteDb(p.dbPath);
      await migrateV1ToV2(writeDb, p.v2SqlDir, p.dbPath);
      await migrateV2(writeDb, p.v2SqlDir);
      await scaffoldRootConfig(p.repoRoot);
      await scaffoldConfig(p.repoRoot);
      await mkdir(join(p.repoRoot, ".moneypenny"), { recursive: true });
      await syncAllConfigs(writeDb, join(p.repoRoot, ".moneypenny"));
      if (force) {
        writeDb.exec("DELETE FROM code_chunks");
        writeDb.exec("DELETE FROM file_tree");
      }
      const n = await indexDirectory(writeDb, p.repoRoot);
      console.log(`indexed ${n} files under ${p.repoRoot}`);
      writeDb.close();
      break;
    }

    case "embed": {
      await mkdir(paths.dataDir, { recursive: true });
      const writeDb = openWriteDb(paths.dbPath);
      await migrateV1ToV2(writeDb, paths.v2SqlDir, paths.dbPath);
      await migrateV2(writeDb, paths.v2SqlDir);
      const aiDb = openAiDb(paths.dbPath, paths.extensionsDir);
      await wireLlmForCli(writeDb, paths.repoRoot, aiDb);
      const batch = parseInt(args[1] ?? "50", 10);
      try {
        let total = 0;
        for (;;) {
          const n = await embedChunks(writeDb, batch);
          if (n === 0) break;
          total += n;
        }
        if (total === 0) {
          console.log("no chunks to embed (already embedded or embedChunks schema mismatch)");
        } else {
          console.log(`embedded ${total} chunk row(s)`);
        }
      } catch (e) {
        console.error(
          "embed failed (v2 schema may not match legacy embed_chunks columns yet):",
          e instanceof Error ? e.message : e,
        );
        process.exitCode = 1;
      }
      aiDb.close();
      writeDb.close();
      break;
    }

    case "detect": {
      await mkdir(paths.dataDir, { recursive: true });
      const writeDb = openWriteDb(paths.dbPath);
      await migrateV1ToV2(writeDb, paths.v2SqlDir, paths.dbPath);
      await migrateV2(writeDb, paths.v2SqlDir);
      const aiDb = openAiDb(paths.dbPath, paths.extensionsDir);
      await wireLlmForCli(writeDb, paths.repoRoot, aiDb);
      const rootCfg = await loadRootConfig(paths.repoRoot);
      const n = await detectConventions(writeDb, rootCfg.agent.model);
      console.log(`detect conventions: inserted/updated (${n})`);
      aiDb.close();
      writeDb.close();
      break;
    }

    case "custodian": {
      console.log(
        'v2: custodian timers run alongside "mp start". Offline one-shot custodian CLI is deferred.',
      );
      break;
    }

    case "dashboard": {
      console.log('Command "dashboard" was removed — use `mp start` for the integrated UI.');
      process.exitCode = 1;
      break;
    }

    default:
      printUsage();
      process.exitCode = command ? 1 : 0;
  }
}

main().catch((err) => {
  console.error("Fatal:", err);
  process.exit(1);
});
