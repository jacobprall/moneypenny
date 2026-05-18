import { readFileSync } from "node:fs";
import { basename } from "node:path";
import yaml from "js-yaml";
import type { Blueprint, BlueprintSource } from "./types.js";

function asBool(v: unknown, d: boolean): boolean {
  return typeof v === "boolean" ? v : d;
}

function asStrArr(v: unknown): string[] | undefined {
  if (v == null) return undefined;
  if (!Array.isArray(v)) return undefined;
  return v.filter((x): x is string => typeof x === "string");
}

export function parseBlueprint(
  path: string,
  source: BlueprintSource,
): Blueprint | { error: string } {
  let raw: string;
  try {
    raw = readFileSync(path, "utf-8");
  } catch (e) {
    return { error: e instanceof Error ? e.message : String(e) };
  }
  const m = raw.match(/^---\r?\n([\s\S]*?)\r?\n---\r?\n([\s\S]*)$/);
  if (!m) return { error: "invalid blueprint: missing yaml frontmatter" };
  let fm: Record<string, unknown>;
  try {
    fm = yaml.load(m[1]) as Record<string, unknown>;
    if (!fm || typeof fm !== "object") return { error: "frontmatter must be an object" };
  } catch (e) {
    return { error: e instanceof Error ? e.message : String(e) };
  }

  const stem = basename(path, ".md");
  const name =
    (typeof fm.name === "string" && fm.name.trim()) ? fm.name.trim() : stem;
  const perms = fm.permissions;
  const pfs =
    perms &&
    typeof perms === "object" &&
    ((perms as { filesystem?: string }).filesystem === "read" ||
      (perms as { filesystem?: string }).filesystem === "readwrite")
      ? ((perms as { filesystem: "read" | "readwrite" }).filesystem)
      : "read";

  const ctx = fm.context;
  const cConv =
    ctx && typeof ctx === "object" && typeof (ctx as { conventions?: boolean }).conventions === "boolean"
      ? (ctx as { conventions: boolean }).conventions
      : true;
  const cSkills =
    ctx && typeof ctx === "object" ? asStrArr((ctx as { skills?: unknown }).skills) : undefined;

  const toolsRaw = fm.tools;
  const tools: string[] | null =
    toolsRaw === null ? null : toolsRaw === undefined ? null : asStrArr(toolsRaw) ?? [];

  const trig = fm.trigger_on;
  const trigger_on =
    trig === "session_close" ||
    trig === "schedule" ||
    trig === "file_change"
      ? trig
      : "manual";

  const strat = fm.strategy;
  const strategy =
    strat === "hitl" || strat === "review" ? strat : "autonomous";

  return {
    name,
    model: typeof fm.model === "string" ? fm.model : undefined,
    tools,
    permissions: {
      filesystem: pfs,
      network:
        perms && typeof perms === "object"
          ? asBool((perms as { network?: boolean }).network, false)
          : false,
      shell:
        perms && typeof perms === "object"
          ? asBool((perms as { shell?: boolean }).shell, false)
          : false,
    },
    strategy,
    pause_after: asStrArr(fm.pause_after) ?? [],
    max_turns: typeof fm.max_turns === "number" ? fm.max_turns : 50,
    context: {
      conventions: cConv,
      skills: cSkills ?? [],
    },
    trigger_on,
    schedule: typeof fm.schedule === "string" ? fm.schedule : undefined,
    file_glob: asStrArr(fm.file_glob),
    body: m[2].trimEnd(),
    path,
    source,
  };
}
