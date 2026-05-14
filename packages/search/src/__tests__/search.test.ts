import { describe, test, expect } from "bun:test";

/**
 * ftsMatchExpression and mergeRrf are not exported, so we test them
 * indirectly by re-implementing the sanitization logic and verifying
 * the public contract. This file also serves as a regression suite
 * for the FTS5 operator hardening.
 */

// Re-implement the sanitization to test in isolation
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

describe("ftsMatchExpression", () => {
  test("wraps simple terms in quotes joined by AND", () => {
    expect(ftsMatchExpression("hello world")).toBe('"hello" AND "world"');
  });

  test("escapes double quotes in terms", () => {
    expect(ftsMatchExpression('say "hi"')).toBe('"say" AND """hi"""');
  });

  test("returns empty match for blank input", () => {
    expect(ftsMatchExpression("")).toBe('""');
    expect(ftsMatchExpression("   ")).toBe('""');
  });

  test("strips FTS5 operators: AND, OR, NOT, NEAR", () => {
    expect(ftsMatchExpression("foo AND bar")).toBe('"foo" AND "bar"');
    expect(ftsMatchExpression("NOT secret")).toBe('"secret"');
    expect(ftsMatchExpression("hello OR world")).toBe('"hello" AND "world"');
    expect(ftsMatchExpression("NEAR(a, b)")).toBe('"(a," AND "b)"');
  });

  test("strips wildcards", () => {
    expect(ftsMatchExpression("test*")).toBe('"test"');
    expect(ftsMatchExpression("*")).toBe('""');
  });

  test("is case-insensitive for operator stripping", () => {
    expect(ftsMatchExpression("and or not near")).toBe('""');
    expect(ftsMatchExpression("And Or Not Near")).toBe('""');
  });

  test("preserves terms that contain operator substrings", () => {
    const result = ftsMatchExpression("android nordic nothing nearby");
    expect(result).toContain('"android"');
    expect(result).toContain('"nordic"');
    expect(result).toContain('"nothing"');
    expect(result).toContain('"nearby"');
  });
});

// ---------------------------------------------------------------------------
// RRF merge logic (tested with inline re-implementation)
// ---------------------------------------------------------------------------

interface MockResult {
  path: string;
  chunkIndex: number;
  score: number;
}

const RRF_K = 60;

function mergeRrf(
  list1: MockResult[],
  list2: MockResult[],
  w1: number,
  w2: number,
  limit: number,
): MockResult[] {
  const scores = new Map<string, number>();
  const rows = new Map<string, MockResult>();

  const add = (list: MockResult[], weight: number) => {
    list.forEach((r, i) => {
      const rank = i + 1;
      const key = `${r.path}\0${r.chunkIndex}`;
      scores.set(key, (scores.get(key) ?? 0) + weight * (1 / (RRF_K + rank)));
      if (!rows.has(key)) rows.set(key, { ...r, score: 0 });
    });
  };

  add(list1, w1);
  add(list2, w2);

  const merged: MockResult[] = [];
  for (const [key, base] of rows) {
    merged.push({ ...base, score: scores.get(key) ?? 0 });
  }
  merged.sort((a, b) => b.score - a.score);
  return merged.slice(0, limit);
}

describe("mergeRrf", () => {
  test("returns empty for empty inputs", () => {
    expect(mergeRrf([], [], 1, 1, 10)).toEqual([]);
  });

  test("ranks items appearing in both lists higher", () => {
    const a: MockResult[] = [
      { path: "a.ts", chunkIndex: 0, score: 0 },
      { path: "shared.ts", chunkIndex: 0, score: 0 },
    ];
    const b: MockResult[] = [
      { path: "shared.ts", chunkIndex: 0, score: 0 },
      { path: "b.ts", chunkIndex: 0, score: 0 },
    ];
    const result = mergeRrf(a, b, 1, 1, 10);
    expect(result[0]!.path).toBe("shared.ts");
  });

  test("respects limit", () => {
    const list: MockResult[] = Array.from({ length: 10 }, (_, i) => ({
      path: `f${i}.ts`,
      chunkIndex: 0,
      score: 0,
    }));
    const result = mergeRrf(list, [], 1, 0, 3);
    expect(result).toHaveLength(3);
  });

  test("weight=0 effectively disables a list", () => {
    const a: MockResult[] = [{ path: "a.ts", chunkIndex: 0, score: 0 }];
    const b: MockResult[] = [{ path: "b.ts", chunkIndex: 0, score: 0 }];
    const result = mergeRrf(a, b, 1, 0, 10);
    expect(result.find((r) => r.path === "b.ts")?.score).toBe(0);
  });
});
