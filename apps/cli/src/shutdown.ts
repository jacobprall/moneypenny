import type { Database } from "bun:sqlite";
import type {
  Custodian,
  EngineSessionRunner,
  EventBus,
  Scheduler,
  Watcher,
  WorkLoop,
} from "@moneypenny/engine";

/** Bun HTTP server minimal surface for graceful stop. */
export type HttpServerLike = {
  stop?: () => void;
};

export type ShutdownHandles = {
  server?: HttpServerLike;
  runner: EngineSessionRunner;
  custodian: Custodian;
  scheduler: Scheduler;
  workLoop: WorkLoop;
  watcher: Watcher;
  events: EventBus;
  aiDb: Database;
  writeDb: Database;
  readDb: Database;
  registryHandles?: Array<{ close(): Promise<void> | void }>;
};

const DRAIN_MS = 5000;

/**
 * Shutdown order from docs/v2/08-runtime §Shutdown — stop HTTP, abort loops,
 * wait for drain (≤5s), stop background components, emit shutdown, release DB.
 */
export async function shutdownRuntime(h: ShutdownHandles): Promise<void> {
  if (h.server?.stop) h.server.stop();

  const ids = h.runner.activeSessionIds();
  await Promise.all(ids.map((id) => h.runner.kill(id)));

  const deadline = Date.now() + DRAIN_MS;
  while (h.runner.activeSessionIds().length > 0 && Date.now() < deadline) {
    await Bun.sleep(50);
  }

  h.scheduler.stop();
  h.custodian.stop();
  h.workLoop.stop();
  await h.watcher.stop();

  if (h.registryHandles) {
    for (const rh of h.registryHandles) {
      try { await rh.close(); } catch {}
    }
  }

  h.events.emit({ type: "system.shutdown" });

  try {
    h.aiDb.exec("SELECT llm_model_free()");
  } catch (e) {
    console.error("[shutdown] llm_model_free:", e instanceof Error ? e.message : e);
  }
  try {
    h.aiDb.close();
  } catch (e) {
    console.error("[shutdown] aiDb.close:", e instanceof Error ? e.message : e);
  }
  try {
    h.readDb.close();
  } catch (e) {
    console.error("[shutdown] readDb.close:", e instanceof Error ? e.message : e);
  }
  try {
    h.writeDb.close();
  } catch (e) {
    console.error("[shutdown] writeDb.close:", e instanceof Error ? e.message : e);
  }
}
