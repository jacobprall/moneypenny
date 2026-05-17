import chokidar from "chokidar";
import type { Database } from "bun:sqlite";
import { syncConfigFile, removeConfig } from "./config.js";
import { indexFile, detectLanguage } from "./indexer.js";
import { readFileSync } from "node:fs";
import { relative, extname, join } from "node:path";

const SUPPORTED_EXTENSIONS = new Set([
  ".ts", ".tsx", ".js", ".jsx",
  ".py", ".rs", ".go", ".java",
  ".c", ".cpp", ".h",
  ".rb", ".swift", ".kt",
  ".sql", ".md",
]);

function parseGitignore(repoRoot: string): string[] {
  try {
    const raw = readFileSync(join(repoRoot, ".gitignore"), "utf-8");
    return raw
      .split("\n")
      .map((l) => l.trim())
      .filter((l) => l && !l.startsWith("#"));
  } catch {
    return [];
  }
}

function gitignoreToGlobs(patterns: string[]): string[] {
  const globs: string[] = [];
  for (const p of patterns) {
    const clean = p.replace(/\/$/, "");
    if (p.startsWith("*.")) {
      globs.push(`**/${p}`);
    } else if (!p.includes("/") && !p.includes("*")) {
      globs.push(`**/${clean}/**`);
      globs.push(`**/${clean}`);
    } else {
      globs.push(p);
    }
  }
  return globs;
}

function removeFile(db: Database, filePath: string): void {
  db.query("DELETE FROM code_chunks WHERE file_path = ?").run(filePath);
  db.query("DELETE FROM file_tree WHERE path = ?").run(filePath);
}

export function startWatcher(
  db: Database,
  repoRoot: string,
  configDir: string,
): {
  configWatcher: ReturnType<typeof chokidar.watch>;
  codeWatcher: ReturnType<typeof chokidar.watch>;
} {
  const configWatcher = chokidar.watch(configDir, {
    ignoreInitial: false,
  });

  configWatcher.on("add", (path) => {
    if (path.endsWith(".toml")) syncConfigFile(db, path);
  });
  configWatcher.on("change", (path) => {
    if (path.endsWith(".toml")) syncConfigFile(db, path);
  });
  configWatcher.on("unlink", (path) => {
    if (path.endsWith(".toml")) removeConfig(db, path);
  });

  const gitignorePatterns = parseGitignore(repoRoot);
  const ignoreGlobs = [
    "**/.git/**",
    "**/.moneypenny/**",
    ...gitignoreToGlobs(gitignorePatterns),
  ];

  const codeWatcher = chokidar.watch(repoRoot, {
    ignored: ignoreGlobs,
    ignoreInitial: false,
  });

  codeWatcher.on("add", (path) => {
    if (SUPPORTED_EXTENSIONS.has(extname(path))) indexFile(db, repoRoot, path);
  });
  codeWatcher.on("change", (path) => {
    if (SUPPORTED_EXTENSIONS.has(extname(path))) indexFile(db, repoRoot, path);
  });
  codeWatcher.on("unlink", (path) => {
    if (SUPPORTED_EXTENSIONS.has(extname(path))) {
      removeFile(db, relative(repoRoot, path));
    }
  });

  return { configWatcher, codeWatcher };
}
