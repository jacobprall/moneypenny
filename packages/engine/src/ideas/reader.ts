import { existsSync, readdirSync, unlinkSync, mkdirSync } from "node:fs";
import { join } from "node:path";
import { parseIdeaFile, writeIdeaFile, parseIdeaContent } from "./parse.js";
import type { Idea, IdeaSource } from "./types.js";
import yaml from "js-yaml";

export function listIdeas(globalDir: string, repoDir?: string): Idea[] {
  const out: Idea[] = [];
  for (const item of readDir(globalDir, "global")) out.push(item);
  if (repoDir) {
    for (const item of readDir(repoDir, "repo")) out.push(item);
  }
  return out;
}

export function getIdea(
  globalDir: string,
  repoDir: string | undefined,
  filename: string,
): Idea | undefined {
  const fn = filename.endsWith(".md") ? filename : `${filename}.md`;
  const repPath = repoDir ? join(repoDir, fn) : null;
  if (repPath && existsSync(repPath)) {
    const r = parseIdeaFile(repPath, "repo", fn);
    return "error" in r ? undefined : r;
  }
  const globPath = join(globalDir, fn);
  if (existsSync(globPath)) {
    const r = parseIdeaFile(globPath, "global", fn);
    return "error" in r ? undefined : r;
  }
  return undefined;
}

export function writeIdea(
  dir: string,
  filename: string,
  body: string,
  frontmatter: Record<string, unknown>,
): Idea {
  if (!existsSync(dir)) mkdirSync(dir, { recursive: true });
  const fn = filename.endsWith(".md") ? filename : `${filename}.md`;
  const path = join(dir, fn);

  if (existsSync(path)) {
    const cur = parseIdeaFile(path, "global", fn);
    if ("error" in cur) throw new Error(cur.error);
    cur.body = body;
    writeIdeaFile(path, cur);
    return cur;
  }

  const head = yaml.dump(frontmatter, { lineWidth: 120, noRefs: true }).trimEnd();
  const raw = `---\n${head}\n---\n\n${body.trimEnd()}\n`;
  const p = parseIdeaContent(raw, { filename: fn, path, source: "global" });
  if ("error" in p) throw new Error(p.error);
  writeIdeaFile(path, p);
  return p;
}

export function deleteIdea(dir: string, filename: string): void {
  const fn = filename.endsWith(".md") ? filename : `${filename}.md`;
  const path = join(dir, fn);
  if (existsSync(path)) unlinkSync(path);
}

function readDir(dir: string, source: IdeaSource): Idea[] {
  if (!existsSync(dir)) return [];
  const out: Idea[] = [];
  for (const name of readdirSync(dir)) {
    if (!name.endsWith(".md")) continue;
    const abs = join(dir, name);
    const r = parseIdeaFile(abs, source, name);
    if (!("error" in r)) out.push(r);
  }
  return out;
}
