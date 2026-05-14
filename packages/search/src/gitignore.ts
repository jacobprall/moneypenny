import { readFileSync, existsSync } from "node:fs";
import { join } from "node:path";
import { globMatch } from "@moneypenny/db/glob";

export interface GitRule {
  pattern: string;
  negated: boolean;
  dirOnly: boolean;
  basePath: string;
}

export function parseGitignoreLines(content: string, basePath: string): GitRule[] {
  const rules: GitRule[] = [];
  for (let line of content.split("\n")) {
    line = line.replace(/\r$/, "").trim();
    if (!line || line.startsWith("#")) continue;
    let negated = false;
    if (line.startsWith("!")) {
      negated = true;
      line = line.slice(1).trim();
    }
    let dirOnly = line.endsWith("/");
    if (dirOnly) line = line.slice(0, -1);
    if (line) rules.push({ pattern: line, negated, dirOnly, basePath });
  }
  return rules;
}

export function loadGitRules(dirPath: string, basePath: string): GitRule[] {
  const p = join(dirPath, ".gitignore");
  if (!existsSync(p)) return [];
  try {
    const raw = readFileSync(p, "utf8");
    return parseGitignoreLines(raw, basePath);
  } catch {
    return [];
  }
}

export function gitIgnored(rel: string, isDir: boolean, rules: GitRule[]): boolean {
  let ignored = false;
  const norm = rel.replace(/\\/g, "/");
  for (const r of rules) {
    if (r.dirOnly && !isDir) continue;
    const target = r.basePath
      ? norm.startsWith(r.basePath + "/") ? norm.slice(r.basePath.length + 1) : norm
      : norm;
    if (globMatch(r.pattern, target)) {
      ignored = !r.negated;
    }
  }
  return ignored;
}
