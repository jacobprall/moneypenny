import { join, resolve } from "node:path";

export type ResolvedPaths = {
  repoRoot: string;
  dataDir: string;
  dbPath: string;
  extensionsDir: string;
  v2SqlDir: string;
  uiDistDir: string;
  /** User-level ~/.moneypenny (literal HOME path, ignores MP_DATA). */
  globalHomeMpDir: string;
};

/** Resolve DB and UI paths used by the CLI runtime. */
export function resolvePaths(repoRoot = process.cwd()): ResolvedPaths {
  const home = process.env.HOME ?? "";
  const dataDir =
    process.env.MP_DATA ?? join(home || ".", ".moneypenny");
  const dbPath = join(dataDir, "moneypenny.db");
  const extensionsDir = join(dataDir, "extensions");
  const v2SqlDir = resolve(import.meta.dir, "../../../packages/db/sql/v2");
  const uiDistDir = resolve(repoRoot, "apps/ui/dist");
  const globalHomeMpDir = join(home || ".", ".moneypenny");
  return {
    repoRoot,
    dataDir,
    dbPath,
    extensionsDir,
    v2SqlDir,
    uiDistDir,
    globalHomeMpDir,
  };
}
