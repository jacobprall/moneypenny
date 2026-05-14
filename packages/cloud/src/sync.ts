/**
 * sqlite-sync (cloudsync) CRDT integration.
 */

import type { Database } from "bun:sqlite";

export const SYNC_TABLES = ["policies", "sync_config", "agents", "skills"] as const;

export function hasCloudsync(db: Database): boolean {
  try {
    db.query("SELECT cloudsync_version()").get();
    return true;
  } catch {
    return false;
  }
}

export function initSyncTables(db: Database): number {
  if (!hasCloudsync(db)) return 0;
  let count = 0;
  for (const table of SYNC_TABLES) {
    try {
      const existsRow = db
        .query("SELECT COUNT(*) as c FROM sqlite_master WHERE type='table' AND name=?")
        .get(table) as { c: number } | undefined;
      if (!existsRow || existsRow.c === 0) continue;
      const enabledRow = db.query("SELECT cloudsync_is_enabled(?) as e").get(table) as { e: number } | undefined;
      if (enabledRow?.e === 1) continue;
      db.run("SELECT cloudsync_init(?)", [table]);
      count++;
    } catch {
      /* */
    }
  }
  return count;
}

export interface SyncStatus {
  siteId: string | null;
  dbVersion: number | null;
  available: boolean;
}

export function getSyncStatus(db: Database): SyncStatus {
  if (!hasCloudsync(db)) {
    return { siteId: null, dbVersion: null, available: false };
  }
  try {
    const siteRow = db.query("SELECT quote(cloudsync_siteid()) as sid").get() as { sid: string } | undefined;
    const versionRow = db.query("SELECT cloudsync_db_version() as v").get() as { v: number } | undefined;
    return {
      siteId: siteRow?.sid ?? null,
      dbVersion: versionRow?.v ?? null,
      available: true,
    };
  } catch {
    return { siteId: null, dbVersion: null, available: false };
  }
}

export interface SyncResult {
  sent: number;
  received: number;
}

export async function runCloudSync(db: Database, cloudUrl: string): Promise<SyncResult> {
  if (!hasCloudsync(db)) {
    return { sent: 0, received: 0 };
  }
  try {
    db.run("SELECT cloudsync_network_init(?1)", [cloudUrl]);
    const row = db.query("SELECT cloudsync_network_sync(5000, 20) as code").get() as { code: number } | undefined;
    const code = row?.code ?? -1;
    db.run("SELECT cloudsync_terminate()");
    if (code === 0) return { sent: 0, received: 0 };
    return { sent: code > 0 ? code : 0, received: code < 0 ? -code : 0 };
  } catch (e) {
    try {
      db.run("SELECT cloudsync_terminate()");
    } catch {
      /* */
    }
    throw e;
  }
}

export interface SyncConfig {
  cloudUrl?: string;
  intervalMs: number;
}

export function getSyncConfig(db: Database): SyncConfig {
  const rows = db.query("SELECT key, value FROM sync_config WHERE key IN ('cloud_url', 'interval_ms')").all() as Array<{
    key: string;
    value: string;
  }>;
  const config: SyncConfig = { intervalMs: 300_000 };
  for (const r of rows) {
    if (r.key === "cloud_url") config.cloudUrl = r.value || undefined;
    if (r.key === "interval_ms") config.intervalMs = parseInt(r.value, 10) || 300_000;
  }
  return config;
}

export function setSyncConfig(db: Database, key: string, value: string): void {
  const now = Date.now();
  db.run(
    "INSERT INTO sync_config (key, value, updated_at) VALUES (?, ?, ?) ON CONFLICT(key) DO UPDATE SET value=?, updated_at=?",
    [key, value, now, value, now],
  );
}
