# Embedding Pipeline

### Problem

The indexer writes `embedding` as NULL for every code chunk. The search
module has a vector leg (`hybridSearch` calls `llm_embed_generate` at
query time) but with no stored embeddings, vector search returns nothing
and the entire hybrid search degrades to BM25-only. The unified query
engine (§3), reactive auto-embed (sprint 3), and context quality eval
(sprint 4) all depend on working embeddings.

### Design

Use the `@sqliteai/sqlite-ai` extension's `llm_embed_generate()` function,
which is already loaded in `database.ts` / `workspace.ts` on a best-effort
basis.

```typescript
// @moneypenny/search

export interface EmbedConfig {
  model: string;          // default: "text-embedding-3-small" (via sqlite-ai)
  batchSize: number;      // default: 50 chunks per transaction
  enabled: boolean;       // default: true if extension loads
}

export function embedChunks(
  db: Database,
  chunks: Array<{ rowid: number; content: string }>,
  config: EmbedConfig,
): { embedded: number; failed: number; durationMs: number };
```

### Integration with indexer

After `chunkFileContent` writes chunks with NULL embeddings, a second pass
calls `embedChunks` on the new/modified chunks:

```typescript
// In indexer.ts, after chunk insertion:
if (embedConfig.enabled) {
  const nullChunks = db.prepare(
    "SELECT rowid, content FROM code_chunks WHERE embedding IS NULL LIMIT ?"
  ).all(embedConfig.batchSize);

  embedChunks(db, nullChunks, embedConfig);
}
```

### Backfill command

`mp index --embed` re-embeds all chunks with NULL embeddings. This is
idempotent and can be interrupted and resumed.

### Graceful degradation

If the sqlite-ai extension fails to load (missing native binary, unsupported
platform), embeddings remain NULL and search falls back to BM25-only.
The `mp doctor` command reports embedding status:

```
Embeddings: ✗ sqlite-ai extension not available
  Vector search will be disabled. BM25 full-text search still works.
  Install: https://github.com/nickhudkins/moneypenny/wiki/sqlite-ai
```

### Acceptance criteria

- [ ] `mp index` populates embeddings for all code chunks when extension is available
- [ ] `mp index --embed` backfills NULL embeddings on existing databases
- [ ] `hybridSearch` returns vector results when embeddings are populated
- [ ] Search works (BM25-only) when extension is unavailable
- [ ] `mp doctor` reports embedding status accurately
- [ ] Embedding errors for individual chunks don't fail the entire index operation

### Implementation

| Phase | Scope | Effort |
|-------|-------|--------|
| 1.1 | `embedChunks` function with batch processing and error handling | 1.5 days |
| 1.2 | Wire into indexer post-chunk-insertion pass | 0.5 days |
| 1.3 | `mp index --embed` backfill command | 0.5 days |
| 1.4 | `mp doctor` embedding status check | 0.5 days |
