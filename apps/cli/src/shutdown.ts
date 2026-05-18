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
  h.watcher.stop();

  h.events.emit({ type: "system.shutdown" });

  try {
    h.aiDb.exec("SELECT llm_model_free()");
  } catch {}
  try {
    h.aiDb.close();
  } catch {}
  try {
    h.readDb.close();
  } catch {}
  try {
    h.writeDb.close();
  } catch {}
}
