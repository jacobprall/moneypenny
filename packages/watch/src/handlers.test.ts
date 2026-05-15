import { describe, expect, test } from "bun:test";

import {
  createBlueprintHandler,
  createIgnoreHandler,
  createJobHandler,
  createPolicyHandler,
  createSkillHandler,
  createSourceFileHandler,
} from "./handlers.js";

describe("watch handler factories", () => {
  test("source extensions", () => {
    const h = createSourceFileHandler({
      extensions: ["ts"],
      onReindex: () => {},
    });
    expect(h.match("src/foo.ts")).toBe(true);
    expect(h.match("src/foo.js")).toBe(false);
  });

  test("policy yaml depth", () => {
    const h = createPolicyHandler({ policiesDir: ".mp/policies", onSync: () => {} });
    expect(h.match(".mp/policies/foo.yaml")).toBe(true);
    expect(h.match(".mp/policies/nested/foo.yaml")).toBe(false);
  });

  test("skills under configured dirs", () => {
    const h = createSkillHandler({
      skillDirs: [".mp/skills"],
      onScan: () => {},
    });
    expect(h.match(".mp/skills/a/b.md")).toBe(true);
    expect(h.match(".mp/skills/a/b.txt")).toBe(false);
  });

  test("jobs yaml", () => {
    const h = createJobHandler({ jobsDir: ".mp/jobs", onSync: () => {} });
    expect(h.match(".mp/jobs/cron.yaml")).toBe(true);
  });

  test("blueprint subtree", () => {
    const h = createBlueprintHandler({
      blueprintsDir: ".mp/agents",
      onChanged: () => {},
    });
    expect(h.match(".mp/agents/x/agent.md")).toBe(true);
    expect(h.match(".mp/other/x.md")).toBe(false);
  });

  test("ignore leaves", () => {
    const h = createIgnoreHandler({ onRecompute: () => {} });
    expect(h.match(".gitignore")).toBe(true);
    expect(h.match("pkg/.mpignore")).toBe(true);
  });
});
