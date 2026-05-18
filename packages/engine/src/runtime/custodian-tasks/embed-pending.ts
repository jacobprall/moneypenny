import type { Database } from "bun:sqlite";
import { listChunksMissingEmbedding, updateChunkEmbedding } from "@moneypenny/db";
import { embeddingToBlob, generateEmbeddings } from "../../embeddings.js";

const EMBED_DIM = 1536;

export async function embedPendingTask(
  db: Database,
  batchSize: number,
): Promise<number> {
  if (!process.env.OPENAI_API_KEY) return 0;
  const chunks = listChunksMissingEmbedding(db, batchSize);
  if (chunks.length === 0) return 0;
  const embeddings = await generateEmbeddings(
    chunks.map((c) => c.content),
  );
  for (let i = 0; i < chunks.length; i++) {
    const vec = embeddings[i];
    if (!vec) continue;
    updateChunkEmbedding(db, chunks[i]!.id, embeddingToBlob(vec), EMBED_DIM);
  }
  return chunks.length;
}
