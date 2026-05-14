export {
  indexCodebase,
  indexWorkspace,
  getIndexStatus,
  getFileTreeDiff,
} from "./indexer";
export { hybridSearch } from "./search";
export { getFileTree, getExcludePatterns, getExcludePatternsFromDb } from "./file-tree";
export { chunkFileContent, languageFromExt, sha256Hex, LANGUAGE_MAP } from "./chunker";
export type { ChunkPart } from "./chunker";
export { parseGitignoreLines, loadGitRules, gitIgnored } from "./gitignore";
export type { GitRule } from "./gitignore";
export { tryStat, mapFileRow } from "./fs-utils";
export type { FileRow } from "./fs-utils";
export {
  MAX_FILE_SIZE,
  DEFAULT_CHUNK_SIZE,
  DEFAULT_CHUNK_OVERLAP,
  MIN_CHUNK_SIZE,
  DEFAULT_BINARY_EXTENSIONS,
  MAX_WALK_DEPTH,
  RRF_K,
} from "./constants";
