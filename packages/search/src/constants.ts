/** Maximum file size (in bytes) to read into memory. Files larger than this are skipped. */
export const MAX_FILE_SIZE = 2 * 1024 * 1024; // 2 MB

/** Default number of characters per chunk. */
export const DEFAULT_CHUNK_SIZE = 1000;

/** Default character overlap between consecutive chunks within a large block. */
export const DEFAULT_CHUNK_OVERLAP = 150;

/** Minimum chunk length (chars) before a small block is merged into the previous chunk. */
export const MIN_CHUNK_SIZE = 250;

/** Reciprocal Rank Fusion constant — controls how quickly low-ranked results lose weight. */
export const RRF_K = 60;

/** Default result limit for hybrid search. */
export const DEFAULT_SEARCH_LIMIT = 20;

/** Default BM25 weight in RRF fusion. */
export const DEFAULT_BM25_WEIGHT = 0.3;

/** Default vector weight in RRF fusion. */
export const DEFAULT_VECTOR_WEIGHT = 0.7;

/**
 * Multiplier applied to the requested result limit to determine how many
 * candidate rows to fetch from each retrieval backend before RRF merge.
 */
export const FETCH_LIMIT_MULTIPLIER = 5;

/** Minimum number of candidate rows fetched regardless of requested limit. */
export const FETCH_LIMIT_FLOOR = 50;

/** Maximum directory recursion depth for the file walker (symlink-loop safety net). */
export const MAX_WALK_DEPTH = 100;

export const DEFAULT_BINARY_EXTENSIONS = new Set([
  "png", "jpg", "jpeg", "gif", "webp", "ico", "bmp", "tif", "tiff",
  "pdf", "zip", "gz", "tgz", "bz2", "xz", "7z", "rar",
  "woff", "woff2", "ttf", "otf", "eot",
  "mp3", "mp4", "wav", "webm", "mov", "avi", "mkv",
  "exe", "dll", "so", "dylib", "bin", "o", "a",
  "class", "jar", "wasm", "sqlite", "db", "db-wal", "db-shm", "db-journal", "parquet", "gifv",
  "lock", "lockb",
]);
