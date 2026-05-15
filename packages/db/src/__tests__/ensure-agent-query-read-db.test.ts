import { mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { describe, expect, test } from "bun:test";
import { closeAgentDB, createAgentDB, ensureAgentQueryReadDb } from "../database.js";

describe("ensureAgentQueryReadDb", () => {
  test("memoizes read-only handle and runs SELECT", () => {
    const dir = mkdtempSync(join(tmpdir(), "mp-agent-"));
    const dbPath = join(dir, "agent.sqlite");
    const agent = createAgentDB(dbPath);
    try {
      const read = ensureAgentQueryReadDb(agent);
      expect(agent.queryReadDb).toBeDefined();
      expect(read).toBe(agent.queryReadDb!);
      expect(ensureAgentQueryReadDb(agent)).toBe(read);
      const rows = read.prepare("SELECT 1 AS n").all() as { n: number }[];
      expect(rows).toEqual([{ n: 1 }]);
    } finally {
      closeAgentDB(agent);
      rmSync(dir, { recursive: true, force: true });
    }
  });

  test("closeAgentDB clears read handle", () => {
    const dir = mkdtempSync(join(tmpdir(), "mp-agent-"));
    const dbPath = join(dir, "agent.sqlite");
    const agent = createAgentDB(dbPath);
    try {
      ensureAgentQueryReadDb(agent);
      expect(agent.queryReadDb).toBeDefined();
      closeAgentDB(agent);
      expect(agent.queryReadDb).toBeUndefined();
    } finally {
      try {
        rmSync(dir, { recursive: true, force: true });
      } catch {
        /* dir may be gone if close removed lock */
      }
    }
  });
});
