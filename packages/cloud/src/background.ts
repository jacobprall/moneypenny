/**
 * Background sync loop — runs cloud sync at configurable interval.
 */

import type { Database } from "bun:sqlite";
import { getSyncConfig, runCloudSync, hasCloudsync } from "./sync.js";

let _intervalId: ReturnType<typeof setInterval> | null = null;

export function startBackgroundSync(db: Database): () => void {
  if (!hasCloudsync(db)) return () => {};

  const tick = async () => {
    const config = getSyncConfig(db);
    if (!config.cloudUrl) return;
    try {
      await runCloudSync(db, config.cloudUrl);
    } catch (e) {
      console.error("Background sync failed:", e instanceof Error ? e.message : e);
    }
  };

  const config = getSyncConfig(db);
  const intervalMs = config.intervalMs ?? 300_000;

  _intervalId = setInterval(tick, intervalMs);

  return () => {
    if (_intervalId) {
      clearInterval(_intervalId);
      _intervalId = null;
    }
  };
}
