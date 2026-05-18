import { Database } from "bun:sqlite";

export type CodeChunk = {
  id: string;
  file_path: string;
  chunk_index: number;
  content: string;
  language: string | null;
  symbol_name: string | null;
  start_line: number | null;
  end_line: number | null;
  embedding: Uint8Array | null;
  embedding_dim: number | null;
  updated_at: number;
};

export function upsertChunk(
  db: Database,
  input: Omit<CodeChunk, "updated_at"> & { updated_at?: number },
): void {
  const updatedAt = input.updated_at ?? Math.floor(Date.now() / 1000);
  db.query<
    unknown,
    [
      string,
      string,
      number,
      string,
      string | null,
      string | null,
      number | null,
      number | null,
      Uint8Array | null,
      number | null,
      number,
    ]
  >(
    `INSERT INTO code_chunks (
       id, file_path, chunk_index, content, language, symbol_name,
       start_line, end_line, embedding, embedding_dim, updated_at
     ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
     ON CONFLICT(id) DO UPDATE SET
       file_path = excluded.file_path,
       chunk_index = excluded.chunk_index,
       content = excluded.content,
       language = excluded.language,
       symbol_name = excluded.symbol_name,
       start_line = excluded.start_line,
       end_line = excluded.end_line,
       embedding = excluded.embedding,
       embedding_dim = excluded.embedding_dim,
       updated_at = excluded.updated_at`,
  ).run(
    input.id,
    input.file_path,
    input.chunk_index,
    input.content,
    input.language,
    input.symbol_name,
    input.start_line,
    input.end_line,
    input.embedding,
    input.embedding_dim,
    updatedAt,
  );
}

export function getChunksForFile(db: Database, filePath: string): CodeChunk[] {
  return db
    .query<CodeChunk, [string]>(
      `SELECT * FROM code_chunks WHERE file_path = ? ORDER BY chunk_index ASC`,
    )
    .all(filePath);
}

export function deleteChunksForFile(db: Database, filePath: string): void {
  db.query<unknown, [string]>(`DELETE FROM code_chunks WHERE file_path = ?`).run(
    filePath,
  );
}

export function listChunksMissingEmbedding(db: Database, limit: number): CodeChunk[] {
  const lim = Math.min(Math.max(limit, 1), 500);
  return db
    .query<CodeChunk, [number]>(
      `SELECT * FROM code_chunks WHERE embedding IS NULL ORDER BY updated_at ASC LIMIT ?`,
    )
    .all(lim);
}

export function updateChunkEmbedding(
  db: Database,
  id: string,
  embedding: Uint8Array,
  embeddingDim: number | null,
): void {
  db.query<unknown, [Uint8Array, number | null, string]>(
    `UPDATE code_chunks SET embedding = ?, embedding_dim = ?, updated_at = unixepoch() WHERE id = ?`,
  ).run(embedding, embeddingDim, id);
}

export function getFtsResults(db: Database, ftsQuery: string, limit = 50): CodeChunk[] {
  const lim = Math.min(Math.max(limit, 1), 200);
  return db
    .query<CodeChunk, [string, number]>(
      `SELECT c.* FROM code_chunks c
       JOIN code_chunks_fts f ON f.rowid = c.rowid
       WHERE f MATCH ?
       LIMIT ?`,
    )
    .all(ftsQuery, lim);
}
