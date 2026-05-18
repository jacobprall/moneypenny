import chokidar from "chokidar";
import type { Database } from "bun:sqlite";
import { existsSync } from "node:fs";
import { mkdir } from "node:fs/promises";
import { join, relative } from "node:path";

import type { BlueprintDirs } from "@moneypenny/core";
import type { ActionContext } from "@moneypenny/core";
import { launchAgent } from "@moneypenny/core";

import { createApi } from "@moneypenny/api";

import { configureLlm, setLlmDatabase } from "@moneypenny/engine";
import {
  BlueprintRegistry,
  Custodian,
  EngineSessionRunner,
  EventBus,
  Scheduler,
  ToolRegistry,
  registerBuiltins,
  Watcher,
  WorkLoop,
  type FileWatcherFn,
} from "@moneypenny/engine";

import { Hono } from "hono";
import { serveStatic } from "hono/bun";

import { syncAllConfigs } from "./config.js";
import { openDatabasesMigrated } from "./db-migrate.js";
import { loadRootConfig, scaffoldRootConfig } from "./root-config.js";
import { scaffoldConfig } from "./scaffold.js";
import { indexDirectory, indexFile } from "./indexer.js";
import type { ResolvedPaths } from "./paths.js";
import type { ShutdownHandles } from "./shutdown.js";
import { shutdownRuntime } from "./shutdown.js";

function makeBlueprintWatcher(): FileWatcherFn {
  return (pathsToWatch) => {
    const w = chokidar.watch(pathsToWatch, {
      ignoreInitial: false,
      ignored: (path: string) =>
        /node_modules|\.git|\/\.git(\/|$)|dist|build|\.next|coverage/.test(path),
    });
    return {
      on(ev, handler) {
        w.on(ev, handler);
      },
      close() {
        return w.close();
      },
    };
  };
}

function applyPersistedRunningAsFailed(writeDb: Database, events: EventBus): void {
  const ids = writeDb
    .query<{ id: string }, []>(
      `SELECT id FROM sessions WHERE status = 'running'`,
    )
    .all();
  for (const row of ids) {
    writeDb
      .query(
        `UPDATE sessions SET status = 'failed', failed_at = unixepoch(), last_active_at = unixepoch() WHERE id = ?`,
      )
      .run(row.id);
    events.emit({
      type: "session.failed",
      session_id: row.id,
      detail: { reason: "runtime_crash" },
    });
  }
}

async function wireLlm(
  writeDb: Database,
  repoRoot: string,
  aiDb: Database,
): Promise<Awaited<ReturnType<typeof loadRootConfig>>> {
  setLlmDatabase(aiDb);

  const saved = writeDb
    .query<{ value: string }, [string]>(`SELECT value FROM config WHERE key = ?`)
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
  return rootCfg;
}

async function prepareProject(writeDb: Database, paths: ResolvedPaths): Promise<void> {
  await mkdir(paths.dataDir, { recursive: true });
  await mkdir(paths.extensionsDir, { recursive: true });
  await mkdir(join(paths.globalHomeMpDir, "blueprints"), { recursive: true });
  await mkdir(join(paths.globalHomeMpDir, "ideas"), { recursive: true });
  await mkdir(join(paths.globalHomeMpDir, "policies"), { recursive: true });
  await Bun.write(join(paths.dataDir, ".keep"), "");

  await scaffoldRootConfig(paths.repoRoot);
  await scaffoldConfig(paths.repoRoot);
  const repoMpDir = join(paths.repoRoot, ".moneypenny");
  await mkdir(repoMpDir, { recursive: true });
  await syncAllConfigs(writeDb, repoMpDir);
}

function ideaBlueprintDirs(paths: ResolvedPaths): BlueprintDirs {
  return {
    global: join(paths.globalHomeMpDir, "blueprints"),
    repo: join(paths.repoRoot, ".moneypenny", "blueprints"),
  };
}

function engineConfigWatchDirs(paths: ResolvedPaths): string[] {
  const home = paths.globalHomeMpDir;
  const r = paths.repoRoot;
  const uniq = [
    join(home, "blueprints"),
    join(home, "ideas"),
    join(home, "policies"),
    join(r, ".moneypenny"),
    join(r, ".moneypenny", "blueprints"),
    join(r, ".moneypenny", "ideas"),
    join(r, ".moneypenny", "policies"),
  ];
  return [...new Set(uniq)].filter(existsSync);
}

function mountUiAndApi(apiInner: Hono, uiDistDir: string): Hono {
  const root = new Hono();
  const indexHtml = join(uiDistDir, "index.html");
  root.all("/api", (c) => apiInner.fetch(c.req.raw));
  root.all("/api/*", (c) => apiInner.fetch(c.req.raw));
  root.use("/assets/*", serveStatic({ root: uiDistDir }));
  root.get("*", (c) => {
    if (!existsSync(indexHtml)) return c.notFound();
    return new Response(Bun.file(indexHtml), {
      headers: { "Content-Type": "text/html; charset=UTF-8" },
    });
  });
  return root;
}

/** Minimal runtime for MCP stdio: DBs, LLM, tools, registries, EventBus, SessionRunner. */
export async function bootstrapMcpContext(paths: ResolvedPaths): Promise<ActionContext> {
  const { writeDb, readDb, aiDb } = await openDatabasesMigrated(paths);
  await prepareProject(writeDb, paths);
  await wireLlm(writeDb, paths.repoRoot, aiDb);

  const tools = new ToolRegistry();
  registerBuiltins(tools);

  const blueprints = new BlueprintRegistry({ watch: makeBlueprintWatcher() });
  blueprints.start(
    join(paths.globalHomeMpDir, "blueprints"),
    join(paths.repoRoot, ".moneypenny", "blueprints"),
  );

  const ideasDirs = {
    global: join(paths.globalHomeMpDir, "ideas"),
    repo: join(paths.repoRoot, ".moneypenny", "ideas"),
  };

  const events = new EventBus(writeDb);
  const runner = new EngineSessionRunner({
    writeDb,
    readDb,
    events,
    blueprints,
    tools,
  });

  return {
    writeDb,
    readDb,
    events,
    runner,
    registry: blueprints,
    ideasDirs,
    tools,
  };
}

export async function runHttpServer(paths: ResolvedPaths): Promise<void> {
  const { writeDb, readDb, aiDb } = await openDatabasesMigrated(paths);
  await prepareProject(writeDb, paths);
  console.log("phase: db + migrations");

  const rootCfg = await wireLlm(writeDb, paths.repoRoot, aiDb);
  console.log("phase: llm configured");

  const tools = new ToolRegistry();
  registerBuiltins(tools);

  const blueprints = new BlueprintRegistry({ watch: makeBlueprintWatcher(), writeDb });
  blueprints.start(
    join(paths.globalHomeMpDir, "blueprints"),
    join(paths.repoRoot, ".moneypenny", "blueprints"),
  );
  console.log("phase: blueprints + ideas");

  const ideasDirs = {
    global: join(paths.globalHomeMpDir, "ideas"),
    repo: join(paths.repoRoot, ".moneypenny", "ideas"),
  };

  const events = new EventBus(writeDb);
  const runner = new EngineSessionRunner({
    writeDb,
    readDb,
    events,
    blueprints,
    tools,
  });

  const custodian = new Custodian({
    writeDb,
    readDb,
    events,
    blueprints,
    tools,
    archiveAfterDays: rootCfg.custodian.archive_after_days,
    compactMessageThreshold: rootCfg.custodian.compact_after_turns,
    eventRetentionDays: 30,
  });

  const actionCtx: ActionContext = {
    writeDb,
    readDb,
    events,
    runner,
    registry: blueprints,
    ideasDirs,
    tools,
    custodian,
  };

  const scheduler = new Scheduler({
    writeDb,
    readDb,
    events,
    blueprints,
    tools,
    repoRoot: paths.repoRoot,
    launchScheduledAgent: async (inp) => {
      const session = await launchAgent(actionCtx, inp);
      return { sessionId: session.id };
    },
  });

  const uiDistDir = paths.uiDistDir;
  const workLoop = new WorkLoop({
    writeDb,
    readDb,
    events,
    blueprints,
    tools,
    batchSize: rootCfg.worker.batch_size,
    onFullReindex: async () => {
      await indexDirectory(writeDb, paths.repoRoot);
    },
  });

  custodian.start();
  scheduler.start();
  workLoop.start();
  console.log("phase: custodian + scheduler + work loop");

  const fileChangeTriggerActive = new Set<string>();

  const checkFileChangeTriggers = (absPath: string) => {
    const rel = relative(paths.repoRoot, absPath);
    if (rel.startsWith("..")) return;
    for (const bp of blueprints.list()) {
      if (bp.trigger_on !== "file_change" || !bp.file_glob?.length) continue;
      const matched = bp.file_glob.some((pattern) => {
        const glob = new Bun.Glob(pattern);
        return glob.match(rel) || glob.match(absPath);
      });
      if (!matched) continue;
      if (fileChangeTriggerActive.has(bp.name)) continue;
      fileChangeTriggerActive.add(bp.name);
      void launchAgent(actionCtx, {
        blueprint: bp.name,
        task: `File changed: ${rel}`,
        cwd: paths.repoRoot,
        label: `${bp.name} (file_change)`,
      })
        .catch(() => {})
        .finally(() => fileChangeTriggerActive.delete(bp.name));
    }
  };

  const watcher = new Watcher({
    codeOnChange: (absPath: string) => {
      void indexFile(writeDb, paths.repoRoot, absPath);
      checkFileChangeTriggers(absPath);
    },
    codeOnRemove: (absPath: string) => {
      const rel = relative(paths.repoRoot, absPath);
      if (!rel.startsWith("..")) {
        writeDb
          .query(
            `DELETE FROM code_chunks WHERE file_path = ? OR file_path LIKE ?`,
          )
          .run(rel, `${rel}#%`);
      }
    },
    configDirs: engineConfigWatchDirs(paths),
    configOnChange: () => {
      // TODO(v3): sync policies table from filesystem once PolicySync ships.
    },
  });
  watcher.start(paths.repoRoot);
  console.log("phase: file watcher");

  const { app: apiInner } = createApi({
    ctx: actionCtx,
    blueprintDirs: ideaBlueprintDirs(paths),
  });

  const uiExists = existsSync(uiDistDir);
  if (!uiExists) {
    console.warn(
      "UI assets not built. Run 'pnpm build' in apps/ui first.",
    );
  }

  const rootApp =
    uiExists && existsSync(join(uiDistDir, "index.html"))
      ? mountUiAndApi(apiInner, uiDistDir)
      : apiInner;

  const hostname = process.env.MP_BIND?.trim() || "127.0.0.1";
  const port = parseInt(process.env.MP_PORT ?? "4966", 10);

  const server = Bun.serve({
    hostname,
    port,
    fetch: rootApp.fetch,
  });

  events.emit({ type: "system.started" });
  applyPersistedRunningAsFailed(writeDb, events);

  console.log(`phase: http http://${hostname}:${port}`);

  const chunkProbe = writeDb
    .query<{ n: number }, []>(`SELECT COUNT(1) as n FROM code_chunks`)
    .get()?.n ?? 0;
  if (chunkProbe === 0) {
    void indexDirectory(writeDb, paths.repoRoot).then((n) => {
      console.log(`indexed ${n} files (cold start)`);
    });
  }

  let shuttingDown = false;
  const shutdown = async (): Promise<void> => {
    if (shuttingDown) return;
    shuttingDown = true;
    const handles: ShutdownHandles = {
      server,
      runner,
      custodian,
      scheduler,
      workLoop,
      watcher,
      events,
      aiDb,
      writeDb,
      readDb,
      registryHandles: [blueprints],
    };
    await shutdownRuntime(handles);
  };

  for (const sig of ["SIGINT", "SIGTERM"] as const) {
    process.on(sig, () => {
      void shutdown().finally(() => process.exit(0));
    });
  }
}
