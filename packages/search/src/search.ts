import { sqlError } from "@moneypenny/db/errors";
import { globMatch } from "@moneypenny/db/glob";
import type { AgentDB, SearchOptions, SearchResult } from "@moneypenny/db/types";
import { getWorkspaceHandle } from "@moneypenny/db/workspace";
import {
  RRF_K,
  DEFAULT_SEARCH_LIMIT,
  DEFAULT_BM25_WEIGHT,
  DEFAULT_VECTOR_WEIGHT,
  FETCH_LIMIT_MULTIPLIER,
  FETCH_LIMIT_FLOOR,
} from "./constants";

// Strip FTS5 operators that survive inside double-quotes or at token boundaries.
const FTS5_OPERATOR_RE = /\b(AND|OR|NOT|NEAR)\b/gi;
const FTS5_WILDCARD_RE = /\*/g;

function ftsMatchExpression(query: string): string {
  const sanitized = query
    .replace(FTS5_OPERATOR_RE, "")
    .replace(FTS5_WILDCARD_RE, "");

  const terms = sanitized
    .trim()
    .split(/\s+/)
    .filter((t) => t.length > 0)
    .map((t) => `"${t.replace(/"/g, '""')}"`);
  if (terms.length === 0) return '""';
  return terms.join(" AND ");
}

function matchesFilters(r: SearchResult, languages?: string[], paths?: string[]): boolean {
  if (languages != null && languages.length > 0) {
    const lang = r.language ?? "";
    if (!languages.includes(lang)) return false;
  }
  if (paths != null && paths.length > 0) {
    if (!paths.some((p) => globMatch(p, r.path))) return false;
  }
  return true;
}

// ---------------------------------------------------------------------------
// Vector availability cache — uses a WeakMap instead of monkey-patching
// ---------------------------------------------------------------------------

const vectorAvailableCache = new WeakMap<object, boolean>();

function probeVectorOnHandle(handle: import("bun:sqlite").Database, cacheKey: object): boolean {
  const cached = vectorAvailableCache.get(cacheKey);
  if (cached !== undefined) return cached;

  let available = false;
  try {
    handle
      .prepare(`SELECT 1 FROM vector_quantize_scan('code_chunks', 'embedding', ?, 1) LIMIT 1`)
      .get(new Uint8Array(4));
    available = true;
  } catch {
    available = false;
  }
  vectorAvailableCache.set(cacheKey, available);
  return available;
}

function probeVectorScan(db: AgentDB): boolean {
  if (db.workspace) {
    return probeVectorOnHandle(db.workspace.db, db.workspace);
  }
  return probeVectorOnHandle(db.db, db);
}

function tryQueryEmbedding(database: import("bun:sqlite").Database, query: string): Uint8Array | null {
  const prefixed = `search_query: ${query}`;
  try {
    const row = database
      .prepare(`SELECT llm_embed_generate(?) AS e`)
      .get(prefixed) as { e: Uint8Array | Buffer } | undefined;
    if (!row?.e) return null;
    return new Uint8Array(row.e);
  } catch {
    return null;
  }
}

interface ChunkRow {
  path: string;
  chunk_index: number;
  start_line: number;
  end_line: number;
  language: string | null;
  chunk_text: string;
}

function rowToResult(r: ChunkRow): SearchResult {
  return {
    path: r.path,
    chunkIndex: r.chunk_index,
    startLine: r.start_line,
    endLine: r.end_line,
    language: r.language,
    chunkText: r.chunk_text,
    score: 0,
  };
}

function bm25Search(database: import("bun:sqlite").Database, match: string, fetchLimit: number): SearchResult[] {
  try {
    const rows = database
      .prepare(
        `SELECT c.path AS path, c.chunk_index AS chunk_index, c.start_line AS start_line,
                c.end_line AS end_line, c.language AS language, c.chunk_text AS chunk_text
         FROM code_fts AS f
         JOIN code_chunks AS c ON c.rowid = f.rowid
         WHERE f MATCH ?
         ORDER BY bm25(code_fts)
         LIMIT ?`,
      )
      .all(match, fetchLimit) as ChunkRow[];
    return rows.map(rowToResult);
  } catch (e) {
    throw sqlError("hybridSearch (BM25)", e);
  }
}

function vectorSearch(database: import("bun:sqlite").Database, embedding: Uint8Array, fetchLimit: number): SearchResult[] {
  const rows = database
    .prepare(
      `SELECT c.path AS path, c.chunk_index AS chunk_index, c.start_line AS start_line,
              c.end_line AS end_line, c.language AS language, c.chunk_text AS chunk_text
       FROM code_chunks AS c
       JOIN vector_quantize_scan('code_chunks', 'embedding', ?, ?) AS v ON c.rowid = v.rowid`,
    )
    .all(embedding, fetchLimit) as ChunkRow[];
  return rows.map(rowToResult);
}

function mergeRrf(
  bm25List: SearchResult[],
  vectorList: SearchResult[],
  bm25Weight: number,
  vectorWeight: number,
  limit: number,
  languages?: string[],
  paths?: string[],
): SearchResult[] {
  const scores = new Map<string, number>();
  const rows = new Map<string, SearchResult>();

  const add = (list: SearchResult[], weight: number) => {
    list.forEach((r, i) => {
      const rank = i + 1;
      const key = `${r.path}\0${r.chunkIndex}`;
      scores.set(key, (scores.get(key) ?? 0) + weight * (1 / (RRF_K + rank)));
      if (!rows.has(key)) rows.set(key, { ...r, score: 0 });
    });
  };

  add(bm25List, bm25Weight);
  add(vectorList, vectorWeight);

  const merged: SearchResult[] = [];
  for (const [key, base] of rows) {
    const s = scores.get(key) ?? 0;
    const next = { ...base, score: s };
    if (!matchesFilters(next, languages, paths)) continue;
    merged.push(next);
  }

  merged.sort((a, b) => b.score - a.score);
  return merged.slice(0, limit);
}

/** BM25 via FTS5, optional vector ANN when extensions + embeddings exist; fused with weighted RRF. */
export function hybridSearch(db: AgentDB, query: string, opts?: SearchOptions): SearchResult[] {
  if (!query.trim()) return [];

  const wsHandle = getWorkspaceHandle(db);
  const limit = opts?.limit ?? DEFAULT_SEARCH_LIMIT;
  const bm25Weight = opts?.bm25Weight ?? DEFAULT_BM25_WEIGHT;
  const vectorWeight = opts?.vectorWeight ?? DEFAULT_VECTOR_WEIGHT;
  const fetchLimit = Math.max(limit * FETCH_LIMIT_MULTIPLIER, FETCH_LIMIT_FLOOR);

  const match = ftsMatchExpression(query);
  const bm25Raw = bm25Search(wsHandle, match, fetchLimit);

  let vectorList: SearchResult[] = [];
  if (probeVectorScan(db)) {
    const emb = tryQueryEmbedding(wsHandle, query);
    if (emb) {
      try {
        vectorList = vectorSearch(wsHandle, emb, fetchLimit);
      } catch (e) {
        console.warn(`[mp] vector search failed, falling back to BM25-only: ${e instanceof Error ? e.message : String(e)}`);
      }
    }
  }

  if (vectorList.length === 0) {
    const wSum = bm25Weight + vectorWeight;
    const scale = wSum > 0 ? 1 / wSum : 1;
    return mergeRrf(bm25Raw, [], bm25Weight * scale, 0, limit, opts?.languages, opts?.paths);
  }

  return mergeRrf(bm25Raw, vectorList, bm25Weight, vectorWeight, limit, opts?.languages, opts?.paths);
}
