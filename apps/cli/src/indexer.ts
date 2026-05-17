import type { Database } from "bun:sqlite";
import { readdir, stat, readFile } from "node:fs/promises";
import { readFileSync } from "node:fs";
import { relative, extname, join } from "node:path";

const SUPPORTED_EXTENSIONS = new Set([
  ".ts", ".tsx", ".js", ".jsx",
  ".py", ".rs", ".go", ".java",
  ".c", ".cpp", ".h",
  ".rb", ".swift", ".kt",
  ".sql", ".md",
]);

const MAX_FILE_SIZE = 512 * 1024;
const MAX_CHUNK_SIZE = 100 * 1024;

const ALWAYS_IGNORE = new Set([
  "node_modules", ".git", ".moneypenny", ".next", ".turbo",
  "dist", "build", ".cache", "coverage", "__pycache__",
  ".venv", "venv", "target",
]);

export function detectLanguage(ext: string): string | null {
  const map: Record<string, string> = {
    ".ts": "typescript", ".tsx": "typescript",
    ".js": "javascript", ".jsx": "javascript",
    ".py": "python", ".rs": "rust", ".go": "go",
    ".java": "java", ".c": "c", ".cpp": "cpp", ".h": "c",
    ".rb": "ruby", ".swift": "swift", ".kt": "kotlin",
    ".sql": "sql", ".md": "markdown",
  };
  return map[ext] ?? null;
}

function parseGitignore(repoRoot: string): Set<string> {
  try {
    const raw = readFileSync(join(repoRoot, ".gitignore"), "utf-8");
    const patterns = new Set<string>();
    for (const line of raw.split("\n")) {
      const trimmed = line.trim();
      if (trimmed && !trimmed.startsWith("#")) {
        patterns.add(trimmed.replace(/\/$/, ""));
      }
    }
    return patterns;
  } catch {
    return new Set();
  }
}

function shouldIgnore(name: string, gitignorePatterns: Set<string>): boolean {
  if (ALWAYS_IGNORE.has(name)) return true;
  if (name.startsWith(".")) return true;
  if (gitignorePatterns.has(name)) return true;
  return false;
}

interface CodeChunk {
  symbolName: string | null;
  content: string;
  startLine: number;
  endLine: number;
}

const TS_PATTERNS = [
  /^(?:export\s+)?(?:async\s+)?function\s+(\w+)/,
  /^(?:export\s+)?class\s+(\w+)/,
  /^(?:export\s+)?interface\s+(\w+)/,
  /^(?:export\s+)?type\s+(\w+)/,
  /^(?:export\s+)?(?:const|let|var)\s+(\w+)\s*=\s*(?:async\s+)?(?:\([^)]*\)|[^=])\s*=>/,
  /^(?:export\s+)?(?:const|let|var)\s+(\w+)\s*=\s*function/,
  /^(?:export\s+)?enum\s+(\w+)/,
];

const PY_PATTERNS = [
  /^(?:async\s+)?def\s+(\w+)/,
  /^class\s+(\w+)/,
];

const GO_PATTERNS = [
  /^func\s+(?:\([^)]+\)\s+)?(\w+)/,
  /^type\s+(\w+)\s+struct/,
  /^type\s+(\w+)\s+interface/,
];

const RUST_PATTERNS = [
  /^(?:pub\s+)?(?:async\s+)?fn\s+(\w+)/,
  /^(?:pub\s+)?struct\s+(\w+)/,
  /^(?:pub\s+)?enum\s+(\w+)/,
  /^(?:pub\s+)?trait\s+(\w+)/,
  /^impl(?:<[^>]+>)?\s+(\w+)/,
];

function getPatternsForLanguage(lang: string | null): RegExp[] {
  switch (lang) {
    case "typescript":
    case "javascript":
      return TS_PATTERNS;
    case "python":
      return PY_PATTERNS;
    case "go":
      return GO_PATTERNS;
    case "rust":
      return RUST_PATTERNS;
    default:
      return [];
  }
}

function chunkBySymbols(content: string, language: string | null): CodeChunk[] {
  const patterns = getPatternsForLanguage(language);
  if (patterns.length === 0) {
    return [{
      symbolName: null,
      content: content.slice(0, MAX_CHUNK_SIZE),
      startLine: 1,
      endLine: content.split("\n").length,
    }];
  }

  const lines = content.split("\n");
  const boundaries: Array<{ line: number; name: string }> = [];

  for (let i = 0; i < lines.length; i++) {
    const trimmed = lines[i].trimStart();
    for (const pattern of patterns) {
      const match = trimmed.match(pattern);
      if (match) {
        boundaries.push({ line: i, name: match[1] });
        break;
      }
    }
  }

  if (boundaries.length === 0) {
    return [{
      symbolName: null,
      content: content.slice(0, MAX_CHUNK_SIZE),
      startLine: 1,
      endLine: lines.length,
    }];
  }

  const chunks: CodeChunk[] = [];

  if (boundaries[0].line > 0) {
    const headerLines = lines.slice(0, boundaries[0].line);
    const headerContent = headerLines.join("\n").trim();
    if (headerContent.length > 10) {
      chunks.push({
        symbolName: null,
        content: headerContent.slice(0, MAX_CHUNK_SIZE),
        startLine: 1,
        endLine: boundaries[0].line,
      });
    }
  }

  for (let i = 0; i < boundaries.length; i++) {
    const start = boundaries[i].line;
    const end = i < boundaries.length - 1 ? boundaries[i + 1].line : lines.length;
    const chunkLines = lines.slice(start, end);
    const chunkContent = chunkLines.join("\n").trim();

    if (chunkContent.length > 0) {
      chunks.push({
        symbolName: boundaries[i].name,
        content: chunkContent.slice(0, MAX_CHUNK_SIZE),
        startLine: start + 1,
        endLine: end,
      });
    }
  }

  return chunks;
}

export async function indexFile(
  db: Database,
  repoRoot: string,
  filePath: string,
): Promise<number> {
  let content: string;
  try {
    const buf = await readFile(filePath);
    if (buf.byteLength > MAX_FILE_SIZE) return 0;
    content = buf.toString("utf-8");
  } catch {
    return 0;
  }

  const relPath = relative(repoRoot, filePath);
  const ext = extname(filePath);
  const language = detectLanguage(ext);

  db.query("DELETE FROM code_chunks WHERE file_path = ?").run(relPath);

  const chunks = chunkBySymbols(content, language);

  const stmt = db.prepare(
    `INSERT OR REPLACE INTO code_chunks (id, file_path, chunk_index, content, language, symbol_name, start_line, end_line, updated_at)
     VALUES (?, ?, ?, ?, ?, ?, ?, ?, unixepoch())`,
  );

  db.transaction(() => {
    for (let i = 0; i < chunks.length; i++) {
      const chunk = chunks[i];
      const id = chunks.length === 1 ? relPath : `${relPath}#${i}`;
      stmt.run(
        id,
        relPath,
        i,
        chunk.content,
        language,
        chunk.symbolName,
        chunk.startLine,
        chunk.endLine,
      );
    }
  })();

  db.query(
    `INSERT OR REPLACE INTO file_tree (path, is_dir, size_bytes, language, updated_at)
     VALUES (?, 0, ?, ?, unixepoch())`,
  ).run(relPath, content.length, language);

  return chunks.length;
}

export async function indexDirectory(
  db: Database,
  repoRoot: string,
  onProgress?: (indexed: number, path: string) => void,
): Promise<number> {
  const gitignorePatterns = parseGitignore(repoRoot);
  let indexed = 0;

  async function walk(dir: string): Promise<void> {
    let entries: string[];
    try {
      entries = await readdir(dir);
    } catch {
      return;
    }

    for (const entry of entries) {
      if (shouldIgnore(entry, gitignorePatterns)) continue;

      const full = join(dir, entry);
      let s;
      try {
        s = await stat(full);
      } catch {
        continue;
      }

      if (s.isDirectory()) {
        await walk(full);
      } else if (SUPPORTED_EXTENSIONS.has(extname(entry))) {
        await indexFile(db, repoRoot, full);
        indexed++;
        if (onProgress && indexed % 100 === 0) {
          onProgress(indexed, relative(repoRoot, full));
        }
      }
    }
  }

  await walk(repoRoot);
  return indexed;
}
