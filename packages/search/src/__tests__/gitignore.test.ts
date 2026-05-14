import { describe, test, expect } from "bun:test";
import { parseGitignoreLines, gitIgnored, type GitRule } from "../gitignore";

// ---------------------------------------------------------------------------
// parseGitignoreLines
// ---------------------------------------------------------------------------

describe("parseGitignoreLines", () => {
  test("parses simple patterns", () => {
    const rules = parseGitignoreLines("node_modules\n*.log\n", "");
    expect(rules).toHaveLength(2);
    expect(rules[0]!.pattern).toBe("node_modules");
    expect(rules[1]!.pattern).toBe("*.log");
  });

  test("skips empty lines and comments", () => {
    const rules = parseGitignoreLines("# comment\n\nfoo\n  \nbar\n", "");
    expect(rules).toHaveLength(2);
  });

  test("detects negated patterns", () => {
    const rules = parseGitignoreLines("!important.txt\n", "");
    expect(rules).toHaveLength(1);
    expect(rules[0]!.negated).toBe(true);
    expect(rules[0]!.pattern).toBe("important.txt");
  });

  test("detects directory-only patterns", () => {
    const rules = parseGitignoreLines("build/\n", "");
    expect(rules).toHaveLength(1);
    expect(rules[0]!.dirOnly).toBe(true);
    expect(rules[0]!.pattern).toBe("build");
  });

  test("handles CRLF line endings", () => {
    const rules = parseGitignoreLines("foo\r\nbar\r\n", "");
    expect(rules).toHaveLength(2);
    expect(rules[0]!.pattern).toBe("foo");
    expect(rules[1]!.pattern).toBe("bar");
  });

  test("preserves basePath", () => {
    const rules = parseGitignoreLines("*.o\n", "src/lib");
    expect(rules[0]!.basePath).toBe("src/lib");
  });
});

// ---------------------------------------------------------------------------
// gitIgnored
// ---------------------------------------------------------------------------

describe("gitIgnored", () => {
  function makeRules(lines: string, basePath = ""): GitRule[] {
    return parseGitignoreLines(lines, basePath);
  }

  test("matches simple file pattern", () => {
    const rules = makeRules("*.log");
    expect(gitIgnored("app.log", false, rules)).toBe(true);
    expect(gitIgnored("app.txt", false, rules)).toBe(false);
  });

  test("directory-only rule ignores dirs but not files", () => {
    const rules = makeRules("build/");
    expect(gitIgnored("build", true, rules)).toBe(true);
    expect(gitIgnored("build", false, rules)).toBe(false);
  });

  test("negation un-ignores a previously ignored path", () => {
    const rules = makeRules("*.log\n!important.log");
    expect(gitIgnored("debug.log", false, rules)).toBe(true);
    expect(gitIgnored("important.log", false, rules)).toBe(false);
  });

  test("basePath scoping strips prefix before matching", () => {
    const rules = makeRules("*.o", "src");
    expect(gitIgnored("src/main.o", false, rules)).toBe(true);
    // Paths outside the basePath are matched against the full relative path
    expect(gitIgnored("lib/main.o", false, rules)).toBe(false);
  });

  test("normalizes backslash separators to forward slashes", () => {
    const rules = makeRules("dir/file.tmp");
    // gitIgnored normalizes \\ to / internally
    expect(gitIgnored("dir\\file.tmp", false, rules)).toBe(true);
  });

  test("returns false for empty rules", () => {
    expect(gitIgnored("anything.txt", false, [])).toBe(false);
    expect(gitIgnored("dir", true, [])).toBe(false);
  });
});
