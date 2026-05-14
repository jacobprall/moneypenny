import { existsSync, readdirSync, readFileSync } from "node:fs";
import { join, basename } from "node:path";
import type { AgentBlueprint } from "./types";
import { DEFAULT_BLUEPRINT, DEFAULT_EXCLUDE_PATTERNS } from "./blueprint";

/**
 * Discover available blueprints from built-in defaults and user-defined JSON
 * files in `.mp/blueprints/`.
 * @deprecated Use discoverAgentDefs() instead.
 */
export function discoverBlueprints(repoPath: string): AgentBlueprint[] {
  const results: AgentBlueprint[] = [DEFAULT_BLUEPRINT];

  const blueprintsDir = join(repoPath, ".mp", "blueprints");
  if (!existsSync(blueprintsDir)) return results;

  let files: string[];
  try {
    files = readdirSync(blueprintsDir).filter((f) => f.endsWith(".json"));
  } catch {
    return results;
  }

  for (const file of files) {
    try {
      const raw = readFileSync(join(blueprintsDir, file), "utf8");
      const parsed = JSON.parse(raw) as Record<string, unknown>;
      if (typeof parsed.name !== "string" || !parsed.name) continue;

      const bp: AgentBlueprint = {
        name: parsed.name,
        description: typeof parsed.description === "string" ? parsed.description : undefined,
        tools: Array.isArray(parsed.tools) ? parsed.tools : [],
        permissions: Array.isArray(parsed.permissions) ? parsed.permissions : [],
        excludePatterns: Array.isArray(parsed.excludePatterns) ? parsed.excludePatterns : DEFAULT_EXCLUDE_PATTERNS,
        config: typeof parsed.config === "object" && parsed.config !== null && !Array.isArray(parsed.config)
          ? (parsed.config as Record<string, string>)
          : {},
        systemInstructions: typeof parsed.systemInstructions === "string" ? parsed.systemInstructions : undefined,
        seedMessages: Array.isArray(parsed.seedMessages) ? parsed.seedMessages : undefined,
        skills: Array.isArray(parsed.skills) ? parsed.skills : undefined,
        subagents: Array.isArray(parsed.subagents) ? parsed.subagents : undefined,
      };

      results.push(bp);
    } catch {
      /* skip malformed files */
    }
  }

  return results;
}

// ── Agent definition discovery ──────────────────────────────────────────

export interface AgentDefInfo {
  name: string;
  description: string | null;
  filePath: string;
}

/**
 * Parse YAML frontmatter from a markdown string.
 * Simple parser that avoids requiring gray-matter as a dependency in @moneypenny/db.
 */
function parseFrontmatter(content: string): { data: Record<string, unknown>; body: string } {
  const match = content.match(/^---\r?\n([\s\S]*?)\r?\n---\r?\n?([\s\S]*)$/);
  if (!match) return { data: {}, body: content };

  const yamlBlock = match[1]!;
  const body = match[2] ?? "";
  const data: Record<string, unknown> = {};

  let currentKey: string | null = null;
  let listItems: string[] | null = null;

  for (const line of yamlBlock.split("\n")) {
    const trimmed = line.trimEnd();

    if (listItems !== null && /^\s+-\s/.test(trimmed)) {
      listItems.push(trimmed.replace(/^\s+-\s+/, "").replace(/^["']|["']$/g, ""));
      continue;
    } else if (listItems !== null && currentKey) {
      data[currentKey] = listItems;
      listItems = null;
      currentKey = null;
    }

    const kvMatch = trimmed.match(/^(\w[\w_-]*)\s*:\s*(.*)$/);
    if (kvMatch) {
      const key = kvMatch[1]!;
      const val = kvMatch[2]!.trim();

      if (val === "" || val === "|" || val === ">") {
        currentKey = key;
        listItems = [];
        continue;
      }

      if (val === "true") data[key] = true;
      else if (val === "false") data[key] = false;
      else if (/^-?\d+$/.test(val)) data[key] = parseInt(val, 10);
      else if (/^-?\d+\.\d+$/.test(val)) data[key] = parseFloat(val);
      else data[key] = val.replace(/^["']|["']$/g, "");

      currentKey = key;
    }
  }

  if (listItems !== null && currentKey) {
    data[currentKey] = listItems;
  }

  return { data, body };
}

/**
 * Discover agent definition files from `.mp/agents/*.md`.
 * Skips files starting with `_` (e.g. `_global.yaml`).
 */
export function discoverAgentDefs(repoPath: string): AgentDefInfo[] {
  const agentsDir = join(repoPath, ".mp", "agents");
  if (!existsSync(agentsDir)) return [];

  let files: string[];
  try {
    files = readdirSync(agentsDir)
      .filter((f) => f.endsWith(".md") && !f.startsWith("_"))
      .sort();
  } catch {
    return [];
  }

  const results: AgentDefInfo[] = [];
  for (const file of files) {
    try {
      const raw = readFileSync(join(agentsDir, file), "utf8");
      const { data } = parseFrontmatter(raw);
      const name = typeof data.name === "string" ? data.name : basename(file, ".md");
      const description = typeof data.description === "string" ? data.description : null;
      results.push({ name, description, filePath: join(agentsDir, file) });
    } catch {
      /* skip malformed files */
    }
  }

  return results;
}
