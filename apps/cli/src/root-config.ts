import type { Database } from "bun:sqlite";
import { join } from "node:path";

export interface RootConfig {
  agent: {
    name: string;
    model: string;
    strategy: string;
  };
  models: {
    strong: string;
    fast: string;
    local: string;
    ollama_base_url: string;
  };
  pointers: {
    cap: number;
    auto_summarize: boolean;
    auto_consolidate: boolean;
  };
  worker: {
    interval_ms: number;
    batch_size: number;
  };
  custodian: {
    compact_after_turns: number;
    archive_after_days: number;
    purge_after_days: number;
    chunk_prune_after_days: number;
  };
  search: {
    fts_weight: number;
    semantic_weight: number;
  };
}

const DEFAULTS: RootConfig = {
  agent: { name: "Moneypenny", model: "claude-sonnet-4-20250514", strategy: "standard" },
  models: { strong: "claude-sonnet-4-20250514", fast: "claude-sonnet-4-20250514", local: "", ollama_base_url: "http://localhost:11434/v1" },
  pointers: { cap: 20, auto_summarize: true, auto_consolidate: true },
  worker: { interval_ms: 30000, batch_size: 10 },
  custodian: { compact_after_turns: 50, archive_after_days: 30, purge_after_days: 90, chunk_prune_after_days: 14 },
  search: { fts_weight: 0.4, semantic_weight: 0.6 },
};

export async function loadRootConfig(repoRoot: string): Promise<RootConfig> {
  const configPath = join(repoRoot, "moneypenny.toml");
  try {
    const raw = await Bun.file(configPath).text();
    const parsed = Bun.TOML.parse(raw) as Record<string, unknown>;
    return deepMerge(DEFAULTS, parsed) as RootConfig;
  } catch {
    return DEFAULTS;
  }
}

export async function scaffoldRootConfig(repoRoot: string): Promise<boolean> {
  const configPath = join(repoRoot, "moneypenny.toml");
  if (await Bun.file(configPath).exists()) return false;

  const toml = `# Moneypenny configuration
# See README.md for full documentation

[agent]
name = "Moneypenny"
model = "claude-sonnet-4-20250514"
strategy = "standard"           # standard | research | evolution

[models]
strong = "claude-sonnet-4-20250514"     # interactive chat, complex reasoning
fast = "claude-sonnet-4-20250514"       # summarization, conventions, skills
local = ""                              # labeling, compaction (e.g. "ollama:llama3.2")
ollama_base_url = "http://localhost:11434/v1"

[pointers]
cap = 20                        # max active session pointers before consolidation
auto_summarize = true
auto_consolidate = true

[worker]
interval_ms = 30000             # background work loop interval
batch_size = 10

[custodian]
compact_after_turns = 50        # compact sessions longer than this
archive_after_days = 30         # archive inactive sessions after this
purge_after_days = 90           # purge archived sessions after this
chunk_prune_after_days = 14     # remove stale code chunks

[search]
fts_weight = 0.4                # BM25 weight in hybrid search
semantic_weight = 0.6           # embedding similarity weight
`;

  await Bun.write(configPath, toml);
  return true;
}

function deepMerge(target: any, source: any): any {
  const result = { ...target };
  for (const key of Object.keys(source)) {
    if (
      source[key] &&
      typeof source[key] === "object" &&
      !Array.isArray(source[key]) &&
      target[key] &&
      typeof target[key] === "object"
    ) {
      result[key] = deepMerge(target[key], source[key]);
    } else {
      result[key] = source[key];
    }
  }
  return result;
}
