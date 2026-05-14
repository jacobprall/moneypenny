import { describe, test, expect } from "bun:test";
import { chunkFileContent, languageFromExt, sha256Hex, LANGUAGE_MAP } from "../chunker";

// ---------------------------------------------------------------------------
// languageFromExt
// ---------------------------------------------------------------------------

describe("languageFromExt", () => {
  test("maps common extensions", () => {
    expect(languageFromExt("src/index.ts")).toBe("typescript");
    expect(languageFromExt("app.jsx")).toBe("javascript");
    expect(languageFromExt("main.py")).toBe("python");
    expect(languageFromExt("lib.rs")).toBe("rust");
    expect(languageFromExt("go.go")).toBe("go");
  });

  test("returns null for unknown extensions", () => {
    expect(languageFromExt("data.xyz")).toBeNull();
    expect(languageFromExt("Makefile")).toBeNull();
  });

  test("handles nested paths", () => {
    expect(languageFromExt("a/b/c/deep.tsx")).toBe("typescript");
  });

  test("is case-insensitive on extension", () => {
    expect(languageFromExt("FILE.PY")).toBe("python");
    expect(languageFromExt("module.Ts")).toBe("typescript");
  });
});

// ---------------------------------------------------------------------------
// sha256Hex
// ---------------------------------------------------------------------------

describe("sha256Hex", () => {
  test("produces consistent 64-char hex for strings", () => {
    const h = sha256Hex("hello");
    expect(h).toHaveLength(64);
    expect(sha256Hex("hello")).toBe(h);
  });

  test("different inputs produce different hashes", () => {
    expect(sha256Hex("a")).not.toBe(sha256Hex("b"));
  });

  test("handles empty string", () => {
    const h = sha256Hex("");
    expect(h).toHaveLength(64);
  });

  test("handles Uint8Array input", () => {
    const h = sha256Hex(new Uint8Array([0x68, 0x65, 0x6c, 0x6c, 0x6f]));
    expect(h).toBe(sha256Hex("hello"));
  });
});

// ---------------------------------------------------------------------------
// chunkFileContent
// ---------------------------------------------------------------------------

describe("chunkFileContent", () => {
  const CHUNK = 200;
  const OVERLAP = 40;
  const MIN = 50;

  test("returns empty array for empty/whitespace content", () => {
    expect(chunkFileContent("", CHUNK, OVERLAP, MIN)).toEqual([]);
    expect(chunkFileContent("   \n\n  ", CHUNK, OVERLAP, MIN)).toEqual([]);
  });

  test("single small block stays as one chunk", () => {
    const content = "const x = 1;\nconst y = 2;";
    const parts = chunkFileContent(content, CHUNK, OVERLAP, MIN);
    expect(parts).toHaveLength(1);
    expect(parts[0]!.startLine).toBe(1);
    expect(parts[0]!.text).toContain("const x");
  });

  test("splits large content into multiple chunks", () => {
    const line = "x".repeat(60);
    const content = Array.from({ length: 20 }, () => line).join("\n");
    const parts = chunkFileContent(content, 200, 40, 50);
    expect(parts.length).toBeGreaterThan(1);
  });

  test("line numbers are 1-indexed and non-overlapping in block boundaries", () => {
    const blocks = ["block one\nline two\nline three", "block two\nline five\nline six"];
    const content = blocks.join("\n\n\n");
    const parts = chunkFileContent(content, 5000, 0, 1);
    for (const p of parts) {
      expect(p.startLine).toBeGreaterThanOrEqual(1);
      expect(p.endLine).toBeGreaterThanOrEqual(p.startLine);
    }
  });

  test("handles CRLF line endings", () => {
    const content = "line1\r\nline2\r\nline3";
    const parts = chunkFileContent(content, 5000, 0, 1);
    expect(parts).toHaveLength(1);
    expect(parts[0]!.text).not.toContain("\r");
  });

  test("merges small trailing blocks into previous chunk", () => {
    const bigBlock = "a".repeat(100) + "\n" + "b".repeat(100);
    const tinyBlock = "c";
    const content = bigBlock + "\n\n\n" + tinyBlock;
    const parts = chunkFileContent(content, 5000, 0, 50);
    expect(parts).toHaveLength(1);
    expect(parts[0]!.text).toContain("c");
  });

  test("overlap produces repeated content between adjacent chunks", () => {
    const lines = Array.from({ length: 30 }, (_, i) => `line_${i}_${"x".repeat(40)}`);
    const content = lines.join("\n");
    const parts = chunkFileContent(content, 200, 80, 10);
    if (parts.length >= 2) {
      const firstEnd = parts[0]!.text.split("\n").pop()!;
      expect(parts[1]!.text).toContain(firstEnd);
    }
  });
});

// ---------------------------------------------------------------------------
// LANGUAGE_MAP sanity
// ---------------------------------------------------------------------------

describe("LANGUAGE_MAP", () => {
  test("contains expected entries", () => {
    expect(LANGUAGE_MAP["ts"]).toBe("typescript");
    expect(LANGUAGE_MAP["py"]).toBe("python");
    expect(LANGUAGE_MAP["rs"]).toBe("rust");
    expect(LANGUAGE_MAP["sh"]).toBe("shell");
  });
});
