import path from "node:path";
import { readdir, lstat } from "node:fs/promises";
import { realpathSync } from "node:fs";
import { z } from "zod";
import { globMatch } from "@moneypenny/db";
import { DEFAULT_BINARY_EXTENSIONS } from "@moneypenny/search";
import type { SearchOptions, SearchResult } from "@moneypenny/db/types";
import type { ToolDefinition } from "../types.js";
import { truncate, spawnWithTimeout, MAX_FILE_SIZE } from "../utils.js";

const LANG_EXT: Record<string, string[]> = {
  typescript: [".ts", ".tsx"],
  javascript: [".js", ".jsx", ".mjs", ".cjs"],
  python: [".py"],
  rust: [".rs"],
  go: [".go"],
  java: [".java"],
  kotlin: [".kt", ".kts"],
  csharp: [".cs"],
  cpp: [".cpp", ".cc", ".cxx", ".hpp"],
  c: [".c", ".h"],
  ruby: [".rb"],
  php: [".php"],
  swift: [".swift"],
};

const SKIP_DIRS = new Set([
  "node_modules", ".git", ".mp", "dist", "build", ".svn", ".hg",
  ".next", "target", "coverage", "__pycache__", ".venv", "venv", "out",
  ".mp.db", ".mp.db-wal", ".mp.db-shm", "mp.db", "mp.db-wal", "mp.db-shm",
  "workspace.db", "workspace.db-wal", "workspace.db-shm",
]);

const NUL_CHECK_BYTES = 8192;

function looksLikeBinary(buf: Buffer): boolean {
  const len = Math.min(buf.length, NUL_CHECK_BYTES);
  for (let i = 0; i < len; i++) {
    if (buf[i] === 0) return true;
  }
  return false;
}

function isBinaryExt(filePath: string): boolean {
  const seg = filePath.split("/").pop() ?? filePath;
  const dot = seg.lastIndexOf(".");
  if (dot <= 0) return false;
  return DEFAULT_BINARY_EXTENSIONS.has(seg.slice(dot + 1).toLowerCase());
}

async function* walkFiles(
  dir: string,
  skipDirs: Set<string>,
  excludePatterns: string[],
  signal?: AbortSignal,
  rootReal?: string,
): AsyncGenerator<string> {
  if (signal?.aborted) return;
  let entries;
  try {
    entries = await readdir(dir, { withFileTypes: true });
  } catch {
    return;
  }
  const root = rootReal ?? dir;
  for (const e of entries) {
    if (signal?.aborted) return;
    const full = path.join(dir, e.name);
    const rel = path.relative(root, full).split(path.sep).join("/");
    if (e.isSymbolicLink()) continue;
    if (e.isDirectory()) {
      if (skipDirs.has(e.name)) continue;
      if (excludePatterns.some((p) => globMatch(p, rel) || globMatch(p, `${rel}/`))) continue;
      yield* walkFiles(full, skipDirs, excludePatterns, signal, root);
    } else if (e.isFile()) {
      if (isBinaryExt(rel)) continue;
      if (excludePatterns.some((p) => globMatch(p, rel))) continue;
      try {
        const st = await lstat(full);
        if (st.size > MAX_FILE_SIZE) continue;
        const real = realpathSync(full);
        if (!real.startsWith(root + path.sep) && real !== root) continue;
      } catch {
        continue;
      }
      yield full;
    }
  }
}

function matchesLanguages(filePath: string, langs?: string[]): boolean {
  if (!langs?.length) return true;
  const lower = filePath.toLowerCase();
  for (const lang of langs) {
    const exts = LANG_EXT[lang.toLowerCase()];
    if (!exts) continue;
    if (exts.some((ext) => lower.endsWith(ext))) return true;
  }
  return false;
}

function matchesPaths(relPath: string, prefixes?: string[]): boolean {
  if (!prefixes?.length) return true;
  const norm = relPath.split(path.sep).join("/");
  return prefixes.some((p) => {
    const prefix = p.replace(/\\/g, "/");
    return norm === prefix || norm.startsWith(prefix.endsWith("/") ? prefix : prefix + "/");
  });
}

interface GrepFallbackOpts {
  limit?: number;
  languages?: string[];
  paths?: string[];
  signal?: AbortSignal;
  excludePatterns?: string[];
}

function buildRgArgs(query: string, opts: GrepFallbackOpts): string[] {
  const args = [
    "--no-heading",
    "--line-number",
    "--max-count", String(opts.limit ?? 20),
    "--color", "never",
  ];

  for (const pat of opts.excludePatterns ?? []) {
    args.push("--glob", `!${pat}`);
  }

  if (opts.paths?.length) {
    for (const p of opts.paths) {
      args.push("--glob", p.endsWith("/") ? `${p}**` : `${p}/**`);
    }
  }

  if (opts.languages?.length) {
    for (const lang of opts.languages) {
      const exts = LANG_EXT[lang.toLowerCase()];
      if (exts) {
        for (const ext of exts) {
          args.push("--glob", `*${ext}`);
        }
      }
    }
  }

  args.push("--fixed-strings", "--", query, ".");
  return args;
}

function parseRgOutput(stdout: string, limit: number): string[] {
  const matches: string[] = [];
  const lines = stdout.split("\n").filter(Boolean);
  for (const line of lines) {
    const m = line.match(/^\.\/(.+?):(\d+):(.*)$/);
    if (!m) continue;
    const [, file, lineNo, content] = m;
    const n = parseInt(lineNo!, 10);
    matches.push(`--- ${file}:${Math.max(1, n - 1)}-${n + 1} ---\n${content}`);
    if (matches.length >= limit) break;
  }
  return matches;
}

async function rgFallback(
  repoPath: string,
  query: string,
  opts: GrepFallbackOpts,
): Promise<string | null> {
  try {
    const args = buildRgArgs(query, opts);
    const result = await spawnWithTimeout(["rg", ...args], {
      cwd: repoPath,
      timeoutMs: 10_000,
      signal: opts.signal,
    });
    if (result.timedOut) return null;
    if (result.exitCode !== 0 && result.exitCode !== 1) return null;
    const limit = opts.limit ?? 20;
    const matches = parseRgOutput(result.stdout, limit);
    if (matches.length === 0) return `No matches for "${query}".`;
    return truncate(matches.join("\n\n"));
  } catch {
    return null;
  }
}

async function jsGrepFallback(
  repoPath: string,
  query: string,
  opts: GrepFallbackOpts,
): Promise<string> {
  const limit = opts.limit ?? 20;
  const matches: string[] = [];
  const root = path.resolve(repoPath);

  outer: for await (const abs of walkFiles(root, SKIP_DIRS, opts.excludePatterns ?? [], opts.signal)) {
    const rel = path.relative(root, abs);
    if (!matchesPaths(rel, opts.paths)) continue;
    if (!matchesLanguages(abs, opts.languages)) continue;

    let buf: Buffer;
    try {
      buf = Buffer.from(await Bun.file(abs).arrayBuffer());
    } catch {
      continue;
    }
    if (looksLikeBinary(buf)) continue;

    const content = buf.toString("utf-8");
    if (!content.includes(query)) continue;

    const lines = content.split(/\r?\n/);
    for (let i = 0; i < lines.length; i++) {
      const line = lines[i]!;
      if (!line.includes(query)) continue;
      const start = Math.max(0, i - 1);
      const end = Math.min(lines.length - 1, i + 1);
      const snippet = lines.slice(start, end + 1).join("\n");
      matches.push(`--- ${rel}:${start + 1}-${end + 1} ---\n${snippet}`);
      if (matches.length >= limit) break outer;
    }
  }

  if (matches.length === 0) {
    return `No matches for "${query}" (filesystem grep fallback).`;
  }
  return truncate(matches.join("\n\n"));
}

async function grepFallback(
  repoPath: string,
  query: string,
  getExcludePatterns: () => string[],
  opts?: Omit<GrepFallbackOpts, "excludePatterns">,
): Promise<string> {
  const excludePatterns = getExcludePatterns();
  const fullOpts: GrepFallbackOpts = { ...opts, excludePatterns };

  const rgResult = await rgFallback(repoPath, query, fullOpts);
  if (rgResult !== null) return rgResult;

  return jsGrepFallback(repoPath, query, fullOpts);
}

function formatHybridResults(results: SearchResult[]): string {
  if (results.length === 0) return "No search results.";
  const chunks = results.map((r) => {
    const lang = r.language ?? "?";
    const score =
      typeof r.score === "number" && Number.isFinite(r.score) ? r.score.toFixed(4) : String(r.score);
    return `--- ${r.path}:${r.startLine}-${r.endLine} (${lang}, score=${score}) ---\n${r.chunkText}`;
  });
  return truncate(chunks.join("\n\n"));
}

const inputSchema = z.object({
  query: z.string(),
  limit: z.number().int().positive().optional(),
  languages: z.array(z.string()).optional(),
  paths: z.array(z.string()).optional(),
});

export const codeSearchTool: ToolDefinition = {
  name: "code_search",
  description:
    "Search the codebase by keyword or natural-language query. This is the fastest way to locate relevant code — prefer it over reading files when you don't already know the exact path. Uses hybrid lexical/semantic search when an index is available, falls back to substring scan otherwise.",
  inputSchema,
  async execute(input, context): Promise<string> {
    try {
      const parsed = input as z.infer<typeof inputSchema>;
      const opts: SearchOptions = {
        limit: parsed.limit,
        languages: parsed.languages,
        paths: parsed.paths,
      };
      const { search } = context.services;

      try {
        let results = search.hybridSearch(parsed.query, opts);
        results = search.validateAndRefreshResults(results);
        return formatHybridResults(results);
      } catch (searchErr) {
        const isHarmless = searchErr instanceof Error && searchErr.name === "NotIndexedError";
        const fallback = await grepFallback(
          context.repoPath,
          parsed.query,
          () => search.getExcludePatterns(),
          { ...opts, signal: context.signal },
        );
        if (isHarmless) {
          return fallback;
        }
        const errMsg = searchErr instanceof Error ? searchErr.message : String(searchErr);
        return `[warning: index search failed: ${errMsg}]\n${fallback}`;
      }
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      return `Error: ${msg}`;
    }
  },
};
