import { mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { describe, expect, test } from "bun:test";
import { closeAgentDB, createAgentDB } from "../database.js";

describe("AgentDB.reads (DbReadPool)", () => {
  test("runs SELECT on read pool", () => {
    const dir = mkdtempSync(join(tmpdir(), "mp-agent-"));
    const dbPath = join(dir, "agent.sqlite");
    const agent = createAgentDB(dbPath);
    try {
      const rows = agent.reads.read((readDb) => readDb.prepare("SELECT 1 AS n").all() as { n: number }[]);
      expect(rows).toEqual([{ n: 1 }]);
    } finally {
      closeAgentDB(agent);
      rmSync(dir, { recursive: true, force: true });
    }
  });

  test("closeAgentDB closes read pool", () => {
    const dir = mkdtempSync(join(tmpdir(), "mp-agent-"));
    const dbPath = join(dir, "agent.sqlite");
    const agent = createAgentDB(dbPath);
    try {
      expect(() => agent.reads.read((r) => r.prepare("SELECT 1").get())).not.toThrow();
      closeAgentDB(agent);
      expect(() => agent.reads.read((r) => r.prepare("SELECT 1").get())).toThrow();
    } finally {
      try {
        rmSync(dir, { recursive: true, force: true });
      } catch {
        /* dir may be gone */
      }
    }
  });
});
