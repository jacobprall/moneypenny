/**
 * Tool-result cache — reserved for future use.
 * Not currently imported; kept as intentional placeholder infrastructure.
 */
import { sqlError } from "./errors";
import type { AgentDB } from "./types";

const DEFAULT_MAX_ENTRIES = 10_000;
const DEFAULT_TTL_MS = 7 * 24 * 60 * 60 * 1000; // 7 days

function cacheHash(toolName: string, input: string): string {
  const h = new Bun.CryptoHasher("sha256");
  h.update(`${toolName}\0${input}`);
  return h.digest("hex");
}

export function getCachedResult(db: AgentDB, toolName: string, input: string): string | undefined {
  const h = cacheHash(toolName, input);
  try {
    const row = db.db
      .prepare(`SELECT tool_name, input, output, created_at FROM tool_cache WHERE hash = ?`)
      .get(h) as { tool_name: string; input: string; output: string; created_at: number } | undefined;
    if (!row) return undefined;
    if (row.tool_name !== toolName || row.input !== input) return undefined;
    const age = Date.now() - row.created_at;
    if (age > DEFAULT_TTL_MS) {
      db.writer.exclusive((raw) => {
        raw.prepare(`DELETE FROM tool_cache WHERE hash = ?`).run(h);
      });
      return undefined;
    }
    return row.output;
  } catch (e) {
    throw sqlError("getCachedResult", e);
  }
}

export function setCachedResult(db: AgentDB, toolName: string, input: string, output: string): void {
  const h = cacheHash(toolName, input);
  const createdAt = Date.now();
  db.writer.defer((raw) => {
    raw
      .prepare(`INSERT OR REPLACE INTO tool_cache (hash, tool_name, input, output, created_at) VALUES (?,?,?,?,?)`)
      .run(h, toolName, input, output, createdAt);
  });
}

/**
 * Evict expired entries and enforce max size.
 * Call periodically (e.g., after indexing or at session start).
 */
export function evictCache(db: AgentDB, opts?: { maxEntries?: number; ttlMs?: number }): number {
  const ttl = opts?.ttlMs ?? DEFAULT_TTL_MS;
  const maxEntries = opts?.maxEntries ?? DEFAULT_MAX_ENTRIES;
  const cutoff = Date.now() - ttl;

  try {
    return db.writer.exclusive((raw) => {
      let evicted = 0;
      const result = raw.prepare(`DELETE FROM tool_cache WHERE created_at < ?`).run(cutoff);
      evicted += result.changes;

      const countRow = raw.prepare(`SELECT COUNT(*) AS c FROM tool_cache`).get() as { c: number };
      if (countRow.c > maxEntries) {
        const excess = countRow.c - maxEntries;
        const pruneResult = raw
          .prepare(`DELETE FROM tool_cache WHERE hash IN (SELECT hash FROM tool_cache ORDER BY created_at ASC LIMIT ?)`)
          .run(excess);
        evicted += pruneResult.changes;
      }
      return evicted;
    });
  } catch (e) {
    throw sqlError("evictCache", e);
  }
}
