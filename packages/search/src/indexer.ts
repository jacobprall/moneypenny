import { readFileSync, readdirSync, lstatSync } from "node:fs";
import { join, relative, normalize } from "node:path";
import { sqlError } from "@moneypenny/db/errors";
import type { AgentDB, FileEntry, IndexOptions, IndexResult, IndexStatus, TreeDiff, WorkspaceDB } from "@moneypenny/db/types";
import { getExcludePatterns, getExcludePatternsFromDb } from "./file-tree";
import { getWorkspaceHandle } from "@moneypenny/db/workspace";
import { globMatch } from "@moneypenny/db/glob";
import { languageFromExt, sha256Hex, chunkFileContent } from "./chunker";
import { tryStat, mapFileRow, type FileRow } from "./fs-utils";
import { loadGitRules, gitIgnored, type GitRule } from "./gitignore";
import {
  MAX_FILE_SIZE,
  DEFAULT_CHUNK_SIZE,
  DEFAULT_CHUNK_OVERLAP,
  MIN_CHUNK_SIZE,
  DEFAULT_BINARY_EXTENSIONS,
  MAX_WALK_DEPTH,
} from "./constants";

// ---------------------------------------------------------------------------
// Binary detection
// ---------------------------------------------------------------------------

function isBinaryPath(rel: string, extensions: Set<string>): boolean {
  const seg = rel.split("/").pop() ?? rel;
  const dot = seg.lastIndexOf(".");
  if (dot <= 0) return false;
  return extensions.has(seg.slice(dot + 1).toLowerCase());
}

// ---------------------------------------------------------------------------
// File walking (with depth limit for symlink-loop safety)
// ---------------------------------------------------------------------------

interface WalkOptions {
  repoPath: string;
  exclude: string[];
  include: string[] | undefined;
  gitRules: GitRule[];
  binaryExtensions: Set<string>;
}

function listSourceFiles(opts: WalkOptions): string[] {
  const { repoPath, exclude, include, gitRules, binaryExtensions } = opts;
  const out: string[] = [];
  const normRoot = normalize(repoPath);
  const visitedInodes = new Set<string>();

  const walk = (dir: string, rules: GitRule[], depth: number) => {
    if (depth > MAX_WALK_DEPTH) return;

    let entries;
    try {
      entries = readdirSync(dir, { withFileTypes: true });
    } catch {
      return;
    }

    const rel = relative(normRoot, dir).replace(/\\/g, "/");
    const localRules = [...rules, ...loadGitRules(dir, rel)];

    for (const e of entries) {
      const full = join(dir, e.name);
      const entryRel = relative(normRoot, full).replace(/\\/g, "/");

      let lst;
      try {
        lst = lstatSync(full);
      } catch {
        continue;
      }
      if (lst.isSymbolicLink()) continue;

      if (e.isDirectory()) {
        const inodeKey = `${lst.dev}:${lst.ino}`;
        if (visitedInodes.has(inodeKey)) continue;
        visitedInodes.add(inodeKey);

        if (e.name === ".git" || e.name === ".mp") continue;
        if (exclude.some((p) => globMatch(p, entryRel) || globMatch(p, `${entryRel}/`))) continue;
        if (gitIgnored(entryRel, true, localRules)) continue;
        walk(full, localRules, depth + 1);
      } else if (e.isFile()) {
        if (e.name === ".gitignore") continue;
        if (isBinaryPath(entryRel, binaryExtensions)) continue;
        if (exclude.some((p) => globMatch(p, entryRel))) continue;
        if (gitIgnored(entryRel, false, localRules)) continue;
        if (include != null && include.length > 0) {
          if (!include.some((p) => globMatch(p, entryRel))) continue;
        }
        out.push(entryRel);
      }
    }
  };

  const rootRules = loadGitRules(normRoot, "");
  walk(normRoot, [...gitRules, ...rootRules], 0);
  return out;
}

// ---------------------------------------------------------------------------
// DB helpers
// ---------------------------------------------------------------------------

function loadFileTreeMap(database: import("bun:sqlite").Database): Map<string, FileEntry> {
  const rows = database
    .prepare(`SELECT path, hash, size, modified_at, language, indexed_at FROM file_tree`)
    .all() as FileRow[];
  const m = new Map<string, FileEntry>();
  for (const r of rows) m.set(r.path, mapFileRow(r));
  return m;
}

/** Check if file likely changed using mtime+size before expensive hash. */
function fileAppearsChanged(fullPath: string, prev: FileEntry): boolean {
  const st = tryStat(fullPath);
  if (!st) return true;
  if (prev.size != null && st.size !== prev.size) return true;
  if (prev.modifiedAt != null && Math.trunc(st.mtimeMs) !== prev.modifiedAt) return true;
  return false;
}

// ---------------------------------------------------------------------------
// Phase 1: Scan disk — read, hash, and chunk changed files into memory
// ---------------------------------------------------------------------------

interface MetaUpdate {
  rel: string;
  hash: string;
  size: number;
  mtimeMs: number;
  language: string | null;
  indexedAt: number | null;
}

interface FileUpdate {
  rel: string;
  hash: string;
  lang: string | null;
  parts: { startLine: number; endLine: number; text: string }[];
  size: number;
  mtimeMs: number | null;
}

interface ScanResult {
  tracked: Set<string>;
  metaUpdates: MetaUpdate[];
  fileUpdates: FileUpdate[];
  unreadable: string[];
  filesChanged: number;
  chunksCreated: number;
}

function scanFiles(
  files: string[],
  repoPath: string,
  prevMap: Map<string, FileEntry>,
  chunkSize: number,
  chunkOverlap: number,
  minChunk: number,
  force: boolean,
): ScanResult {
  const tracked = new Set<string>();
  const metaUpdates: MetaUpdate[] = [];
  const fileUpdates: FileUpdate[] = [];
  const unreadable: string[] = [];
  let filesChanged = 0;
  let chunksCreated = 0;

  for (const rel of files) {
    tracked.add(rel);
    const fullPath = join(repoPath, rel);
    const prev = prevMap.get(rel);

    if (!force && prev && !fileAppearsChanged(fullPath, prev)) continue;

    const st = tryStat(fullPath);
    if (st && st.size > MAX_FILE_SIZE) continue;

    let content: string;
    try {
      content = readFileSync(fullPath, "utf8");
    } catch {
      if (prevMap.has(rel)) unreadable.push(rel);
      continue;
    }

    const hash = sha256Hex(content);
    if (!force && prev && prev.hash === hash) {
      if (st) {
        metaUpdates.push({
          rel,
          hash,
          size: st.size,
          mtimeMs: Math.trunc(st.mtimeMs),
          language: prev.language,
          indexedAt: prev.indexedAt,
        });
      }
      continue;
    }

    filesChanged++;
    const lang = languageFromExt(rel);
    const parts = chunkFileContent(content, chunkSize, chunkOverlap, minChunk);
    chunksCreated += parts.length;
    fileUpdates.push({
      rel,
      hash,
      lang,
      parts,
      size: st?.size ?? content.length,
      mtimeMs: st != null ? Math.trunc(st.mtimeMs) : null,
    });
  }

  return { tracked, metaUpdates, fileUpdates, unreadable, filesChanged, chunksCreated };
}

// ---------------------------------------------------------------------------
// Phase 2: Write scan results into the DB in a single transaction
// ---------------------------------------------------------------------------

function writeResults(
  targetDb: import("bun:sqlite").Database,
  scan: ScanResult,
  prevMap: Map<string, FileEntry>,
  now: number,
): { filesChanged: number } {
  let { filesChanged } = scan;

  const deleteChunksStmt = targetDb.prepare(`DELETE FROM code_chunks WHERE path = ?`);
  const insertChunkStmt = targetDb.prepare(
    `INSERT INTO code_chunks (path, chunk_index, start_line, end_line, language, chunk_text, embedding)
     VALUES (?,?,?,?,?,?,NULL)`,
  );
  const upsertFileStmt = targetDb.prepare(
    `INSERT OR REPLACE INTO file_tree (path, hash, size, modified_at, language, indexed_at)
     VALUES (?,?,?,?,?,?)`,
  );
  const deleteFileStmt = targetDb.prepare(`DELETE FROM file_tree WHERE path = ?`);

  const tx = targetDb.transaction(() => {
    for (const u of scan.metaUpdates) {
      upsertFileStmt.run(u.rel, u.hash, u.size, u.mtimeMs, u.language, u.indexedAt);
    }

    for (const u of scan.fileUpdates) {
      deleteChunksStmt.run(u.rel);
      let idx = 0;
      for (const part of u.parts) {
        insertChunkStmt.run(u.rel, idx++, part.startLine, part.endLine, u.lang, part.text);
      }
      upsertFileStmt.run(u.rel, u.hash, u.size, u.mtimeMs, u.lang, now);
    }

    for (const rel of scan.unreadable) {
      deleteChunksStmt.run(rel);
      deleteFileStmt.run(rel);
    }

    for (const p of prevMap.keys()) {
      if (!scan.tracked.has(p)) {
        deleteChunksStmt.run(p);
        deleteFileStmt.run(p);
        filesChanged++;
      }
    }
  });

  try {
    tx();
  } catch (e) {
    throw sqlError("indexCodebase", e);
  }

  return { filesChanged };
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/**
 * Incrementally index text files under repoPath; uses mtime+size for fast skip.
 * When the AgentDB has a workspace attached, indexing targets the workspace DB.
 */
export function indexCodebase(db: AgentDB, repoPath: string, opts?: IndexOptions): IndexResult {
  const wsHandle = getWorkspaceHandle(db);
  return indexCodebaseInto(wsHandle, db, repoPath, opts);
}

/** Index directly into a WorkspaceDB (no AgentDB needed). */
export function indexWorkspace(ws: WorkspaceDB, opts?: IndexOptions): IndexResult {
  return indexCodebaseInto(ws.db, undefined, ws.workspacePath, opts);
}

function indexCodebaseInto(
  targetDb: import("bun:sqlite").Database,
  agentDb: AgentDB | undefined,
  repoPath: string,
  opts?: IndexOptions,
): IndexResult {
  const t0 = Date.now();
  const chunkSize = opts?.chunkSize ?? DEFAULT_CHUNK_SIZE;
  const chunkOverlap = opts?.chunkOverlap ?? DEFAULT_CHUNK_OVERLAP;
  const force = opts?.forceReindex ?? false;

  const fromDb = agentDb ? getExcludePatterns(agentDb) : getExcludePatternsFromDb(targetDb);
  const exclude = [...fromDb, ...(opts?.exclude ?? [])];
  const binaryExtensions = opts?.binaryExtensions
    ? new Set([...DEFAULT_BINARY_EXTENSIONS, ...opts.binaryExtensions])
    : DEFAULT_BINARY_EXTENSIONS;

  const gitRules = loadGitRules(repoPath, "");
  let files: string[] = [];
  try {
    files = listSourceFiles({ repoPath, exclude, include: opts?.include, gitRules, binaryExtensions });
  } catch (e) {
    throw sqlError("indexCodebase (scan)", e);
  }

  const prevMap = loadFileTreeMap(targetDb);
  const now = Date.now();

  const scan = scanFiles(files, repoPath, prevMap, chunkSize, chunkOverlap, MIN_CHUNK_SIZE, force);
  const { filesChanged } = writeResults(targetDb, scan, prevMap, now);

  return {
    filesScanned: files.length,
    filesChanged,
    chunksCreated: scan.chunksCreated,
    embeddingsGenerated: 0,
    elapsedMs: Date.now() - t0,
  };
}

export function getIndexStatus(db: AgentDB): IndexStatus {
  const wsHandle = getWorkspaceHandle(db);
  try {
    const totalFilesRow = wsHandle.prepare(`SELECT COUNT(*) AS c FROM file_tree`).get() as { c: number };
    const totalChunksRow = wsHandle.prepare(`SELECT COUNT(*) AS c FROM code_chunks`).get() as { c: number };
    const lastRow = wsHandle.prepare(`SELECT MAX(indexed_at) AS m FROM file_tree`).get() as { m: number | null };
    const pendingRow = wsHandle.prepare(`SELECT COUNT(*) AS c FROM file_tree WHERE indexed_at IS NULL`).get() as { c: number };
    const langRows = wsHandle
      .prepare(`SELECT language, COUNT(*) AS c FROM code_chunks GROUP BY language`)
      .all() as { language: string | null; c: number }[];
    const languageBreakdown: Record<string, number> = {};
    for (const r of langRows) {
      const k = r.language ?? "unknown";
      languageBreakdown[k] = (languageBreakdown[k] ?? 0) + Number(r.c);
    }
    return {
      totalFiles: Number(totalFilesRow.c),
      totalChunks: Number(totalChunksRow.c),
      lastIndexedAt: lastRow.m != null ? Number(lastRow.m) : null,
      pendingFiles: Number(pendingRow.c),
      languageBreakdown,
    };
  } catch (e) {
    throw sqlError("getIndexStatus", e);
  }
}

/** Diff DB file_tree vs filesystem using mtime+size fast-path before hashing. */
export function getFileTreeDiff(db: AgentDB, repoPath: string): TreeDiff {
  const wsHandle = getWorkspaceHandle(db);
  const fromDb = getExcludePatterns(db);
  const gitRules = loadGitRules(repoPath, "");
  let files: string[] = [];
  try {
    files = listSourceFiles({
      repoPath,
      exclude: fromDb,
      include: undefined,
      gitRules,
      binaryExtensions: DEFAULT_BINARY_EXTENSIONS,
    });
  } catch (e) {
    throw sqlError("getFileTreeDiff", e);
  }
  const disk = new Set(files);
  const prevMap = loadFileTreeMap(wsHandle);

  const added: string[] = [];
  const changed: string[] = [];
  const removed: string[] = [];

  for (const p of disk) {
    const prev = prevMap.get(p);
    if (!prev) {
      added.push(p);
      continue;
    }
    const fullPath = join(repoPath, p);
    if (!fileAppearsChanged(fullPath, prev)) continue;

    const st = tryStat(fullPath);
    if (st && st.size > MAX_FILE_SIZE) continue;

    let content: string;
    try {
      content = readFileSync(fullPath, "utf8");
    } catch {
      continue;
    }
    const hash = sha256Hex(content);
    if (prev.hash !== hash) changed.push(p);
  }
  for (const p of prevMap.keys()) {
    if (!disk.has(p)) removed.push(p);
  }

  added.sort();
  changed.sort();
  removed.sort();
  return { added, changed, removed };
}
