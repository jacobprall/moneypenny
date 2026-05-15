import { dirname as posixDirname } from "node:path/posix";

import type { WatchHandler } from "./watcher.js";

function normalizeFsPath(p: string): string {
  return p.replaceAll("\\", "/").replace(/\/+$/, "");
}

function escapeRegex(s: string): string {
  return s.replace(/[.+^${}()|[\]\\]/g, "\\$&");
}

function extnamePosix(rel: string): string {
  const slash = Math.max(rel.lastIndexOf("/"), rel.lastIndexOf("\\"));
  const base = slash >= 0 ? rel.slice(slash + 1) : rel;
  const dot = base.lastIndexOf(".");
  if (dot <= 0) return "";
  return base.slice(dot).toLowerCase();
}

/**
 * Indexes source files matching configured extensions (e.g. `.ts`, `.py`).
 */
export function createSourceFileHandler(config: {
  extensions: string[];
  onReindex: (paths: string[]) => void | Promise<void>;
}): WatchHandler {
  const exts = new Set(
    config.extensions.map((e) =>
      e.startsWith(".") ? e.toLowerCase() : `.${e.toLowerCase()}`,
    ),
  );

  return {
    name: "source-files",
    match(rel): boolean {
      const ext = extnamePosix(rel);
      return ext !== "" && exts.has(ext);
    },

    async handle(events): Promise<void> {
      const paths = events.map((e) => e.relativePath);
      await Promise.resolve(config.onReindex(paths));
    },
  };
}

function blueprintAgentDir(rel: string, base: string): string | null {
  const prefix = `${base}/`;
  if (!rel.startsWith(prefix)) return null;
  return posixDirname(rel);
}

/**
 * Tracks changes under `{blueprintsDir}/**`.
 */
export function createBlueprintHandler(config: {
  blueprintsDir: string;
  onChanged: (dirPath: string) => void;
}): WatchHandler {
  const base = normalizeFsPath(config.blueprintsDir);

  return {
    name: "blueprints",
    match(rel): boolean {
      return rel.startsWith(`${base}/`);
    },

    async handle(events): Promise<void> {
      const dirs = new Set<string>();
      for (const e of events) {
        const dir = blueprintAgentDir(e.relativePath, base);
        if (dir) dirs.add(dir);
      }
      for (const d of dirs) config.onChanged(d);
    },
  };
}

/**
 * Policy YAML updates under `{policiesDir}/*.yaml`.
 */
export function createPolicyHandler(config: {
  policiesDir: string;
  onSync: () => void | Promise<void>;
}): WatchHandler {
  const policiesDir = normalizeFsPath(config.policiesDir);
  const re = new RegExp(`^${escapeRegex(policiesDir)}/[^/]+\\.yaml$`, "i");

  return {
    name: "policies",
    match(rel): boolean {
      return re.test(rel);
    },

    async handle(_events): Promise<void> {
      await Promise.resolve(config.onSync());
    },
  };
}

/**
 * Skill markdown updates under configured skill directories (recursive `.md` files).
 */
export function createSkillHandler(config: {
  skillDirs: string[];
  onScan: () => void | Promise<void>;
}): WatchHandler {
  const prefixes = config.skillDirs.map((d) => normalizeFsPath(d));

  return {
    name: "skills",
    match(rel): boolean {
      const lower = rel.toLowerCase();
      if (!lower.endsWith(".md")) return false;
      return prefixes.some((p) => rel.startsWith(`${p}/`) || rel === p);
    },

    async handle(_events): Promise<void> {
      await Promise.resolve(config.onScan());
    },
  };
}

/**
 * Job YAML updates under `{jobsDir}/*.yaml`.
 */
export function createJobHandler(config: {
  jobsDir: string;
  onSync: () => void | Promise<void>;
}): WatchHandler {
  const jobsDir = normalizeFsPath(config.jobsDir);
  const re = new RegExp(`^${escapeRegex(jobsDir)}/[^/]+\\.yaml$`, "i");

  return {
    name: "jobs",
    match(rel): boolean {
      return re.test(rel);
    },

    async handle(_events): Promise<void> {
      await Promise.resolve(config.onSync());
    },
  };
}

function isIgnoreLeaf(rel: string): boolean {
  const n = normalizeFsPath(rel);
  return (
    n === ".gitignore" ||
    n === ".mpignore" ||
    n.endsWith("/.gitignore") ||
    n.endsWith("/.mpignore")
  );
}

/**
 * Repo ignore metadata touched — caller may refresh watchers; excludes reload inside watcher.
 */
export function createIgnoreHandler(config: { onRecompute: () => void }): WatchHandler {
  return {
    name: "ignore-files",
    match(rel): boolean {
      return isIgnoreLeaf(rel);
    },

    handle(_events): void {
      config.onRecompute();
    },
  };
}
