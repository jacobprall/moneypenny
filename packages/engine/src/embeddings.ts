import type { Database } from "bun:sqlite";

const EMBED_MODEL = "text-embedding-3-small";
const EMBED_DIMS = 1536;
const BATCH_SIZE = 20;

interface EmbeddingResponse {
  data: Array<{ embedding: number[]; index: number }>;
  usage: { prompt_tokens: number; total_tokens: number };
}

export async function generateEmbeddings(
  texts: string[],
  apiKey?: string,
): Promise<Float32Array[]> {
  const key = apiKey ?? process.env.OPENAI_API_KEY;
  if (!key) throw new Error("OPENAI_API_KEY required for embeddings");

  const results: Float32Array[] = [];

  for (let i = 0; i < texts.length; i += BATCH_SIZE) {
    const batch = texts.slice(i, i + BATCH_SIZE);
    const resp = await fetch("https://api.openai.com/v1/embeddings", {
      method: "POST",
      headers: {
        Authorization: `Bearer ${key}`,
        "Content-Type": "application/json",
      },
      body: JSON.stringify({
        model: EMBED_MODEL,
        input: batch.map((t) => t.slice(0, 8000)),
        dimensions: EMBED_DIMS,
      }),
    });

    if (!resp.ok) {
      const err = await resp.text();
      throw new Error(`Embedding API error ${resp.status}: ${err}`);
    }

    const json: EmbeddingResponse = await resp.json();
    json.data.sort((a, b) => a.index - b.index);

    for (const item of json.data) {
      results.push(new Float32Array(item.embedding));
    }
  }

  return results;
}

export function embeddingToBlob(vec: Float32Array): Buffer {
  return Buffer.from(vec.buffer);
}

export function blobToEmbedding(blob: Buffer): Float32Array {
  return new Float32Array(blob.buffer, blob.byteOffset, blob.byteLength / 4);
}

export function cosineSimilarity(a: Float32Array, b: Float32Array): number {
  let dot = 0;
  let normA = 0;
  let normB = 0;
  for (let i = 0; i < a.length; i++) {
    dot += a[i] * b[i];
    normA += a[i] * a[i];
    normB += b[i] * b[i];
  }
  const denom = Math.sqrt(normA) * Math.sqrt(normB);
  return denom === 0 ? 0 : dot / denom;
}

export async function embedChunks(
  db: Database,
  batchSize = 50,
): Promise<number> {
  const chunks = db
    .query<{ id: string; content: string }, [number]>(
      "SELECT id, content FROM code_chunks WHERE embedding IS NULL LIMIT ?",
    )
    .all(batchSize);

  if (chunks.length === 0) return 0;

  const texts = chunks.map((c) => c.content);
  const embeddings = await generateEmbeddings(texts);

  const stmt = db.prepare(
    "UPDATE code_chunks SET embedding = ?, embed_model = ?, embed_dims = ? WHERE id = ?",
  );

  db.transaction(() => {
    for (let i = 0; i < chunks.length; i++) {
      stmt.run(
        embeddingToBlob(embeddings[i]),
        EMBED_MODEL,
        EMBED_DIMS,
        chunks[i].id,
      );
    }
  })();

  return chunks.length;
}

export async function semanticSearch(
  db: Database,
  query: string,
  limit = 10,
): Promise<
  Array<{
    id: string;
    file_path: string;
    symbol_name: string | null;
    content: string;
    start_line: number | null;
    similarity: number;
  }>
> {
  const [queryVec] = await generateEmbeddings([query]);

  const rows = db
    .query<
      {
        id: string;
        file_path: string;
        symbol_name: string | null;
        content: string;
        start_line: number | null;
        embedding: Buffer;
      },
      []
    >(
      "SELECT id, file_path, symbol_name, content, start_line, embedding FROM code_chunks WHERE embedding IS NOT NULL",
    )
    .all();

  const scored = rows.map((row) => ({
    id: row.id,
    file_path: row.file_path,
    symbol_name: row.symbol_name,
    content: row.content,
    start_line: row.start_line,
    similarity: cosineSimilarity(queryVec, blobToEmbedding(row.embedding)),
  }));

  scored.sort((a, b) => b.similarity - a.similarity);
  return scored.slice(0, limit);
}

interface HybridResult {
  file_path: string;
  symbol_name: string | null;
  content: string;
  start_line: number | null;
  fts_rank: number;
  semantic_score: number;
  combined_score: number;
}

export async function hybridSearch(
  db: Database,
  query: string,
  limit = 10,
  ftsWeight = 0.4,
  semanticWeight = 0.6,
): Promise<HybridResult[]> {
  const sanitized = query
    .replace(/[^\w\s]/g, " ")
    .replace(/\s+/g, " ")
    .trim();

  const ftsResults = sanitized
    ? db
        .query<
          {
            file_path: string;
            symbol_name: string | null;
            content: string;
            start_line: number | null;
            rank: number;
          },
          [string]
        >(
          `SELECT c.file_path, c.symbol_name, c.content, c.start_line, fts.rank
           FROM code_chunks_fts fts
           JOIN code_chunks c ON c.rowid = fts.rowid
           WHERE code_chunks_fts MATCH ?
           ORDER BY rank LIMIT ?`,
        )
        .all(sanitized)
    : [];

  const hasEmbeddings =
    (
      db
        .query<{ cnt: number }, []>(
          "SELECT COUNT(*) as cnt FROM code_chunks WHERE embedding IS NOT NULL",
        )
        .get()?.cnt ?? 0
    ) > 0;

  if (!hasEmbeddings) {
    return ftsResults.slice(0, limit).map((r) => ({
      file_path: r.file_path,
      symbol_name: r.symbol_name,
      content: r.content,
      start_line: r.start_line,
      fts_rank: r.rank,
      semantic_score: 0,
      combined_score: r.rank,
    }));
  }

  let semResults: Awaited<ReturnType<typeof semanticSearch>>;
  try {
    semResults = await semanticSearch(db, query, limit * 2);
  } catch {
    return ftsResults.slice(0, limit).map((r) => ({
      file_path: r.file_path,
      symbol_name: r.symbol_name,
      content: r.content,
      start_line: r.start_line,
      fts_rank: r.rank,
      semantic_score: 0,
      combined_score: r.rank,
    }));
  }

  const merged = new Map<string, HybridResult>();

  const maxFtsRank = ftsResults.length > 0
    ? Math.max(...ftsResults.map((r) => Math.abs(r.rank)))
    : 1;

  for (const r of ftsResults) {
    const normalizedFts = 1 - Math.abs(r.rank) / (maxFtsRank + 1);
    merged.set(r.file_path, {
      file_path: r.file_path,
      symbol_name: r.symbol_name,
      content: r.content,
      start_line: r.start_line,
      fts_rank: normalizedFts,
      semantic_score: 0,
      combined_score: normalizedFts * ftsWeight,
    });
  }

  for (const r of semResults) {
    const existing = merged.get(r.file_path);
    if (existing) {
      existing.semantic_score = r.similarity;
      existing.combined_score =
        existing.fts_rank * ftsWeight + r.similarity * semanticWeight;
    } else {
      merged.set(r.file_path, {
        file_path: r.file_path,
        symbol_name: r.symbol_name,
        content: r.content,
        start_line: r.start_line,
        fts_rank: 0,
        semantic_score: r.similarity,
        combined_score: r.similarity * semanticWeight,
      });
    }
  }

  return [...merged.values()]
    .sort((a, b) => b.combined_score - a.combined_score)
    .slice(0, limit);
}
