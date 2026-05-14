import { Database } from "bun:sqlite";
import { existsSync } from "node:fs";

const HOMEBREW_SQLITE_PATHS = [
  "/opt/homebrew/opt/sqlite/lib/libsqlite3.dylib",
  "/usr/local/opt/sqlite/lib/libsqlite3.dylib",
];

let initialized = false;

/**
 * On macOS, Apple's system SQLite is compiled with SQLITE_OMIT_LOAD_EXTENSION,
 * preventing dynamic extension loading. This swaps in Homebrew's SQLite which
 * supports it. Must be called before any Database instances are created.
 *
 * No-ops on Linux/Windows (their system SQLite supports extensions) and
 * if Homebrew SQLite isn't installed.
 */
export function ensureCustomSQLite(): void {
  if (initialized) return;
  initialized = true;

  if (process.platform !== "darwin") return;

  for (const candidate of HOMEBREW_SQLITE_PATHS) {
    if (existsSync(candidate)) {
      try {
        Database.setCustomSQLite(candidate);
      } catch {
        /* Bun version may not support setCustomSQLite */
      }
      return;
    }
  }
}
