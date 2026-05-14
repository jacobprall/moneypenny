import type { AgentBlueprint, AgentDef, AgentDB, Permission, ToolDef } from "./types";
import { sqlError } from "./errors";
import { generateUUIDv7 } from "./uuid";
import { DEFAULT_SKILLS, DEFAULT_SUBAGENT_DEFS } from "./default-skills";

export const DEFAULT_EXCLUDE_PATTERNS: string[] = [
  "**/node_modules/**",
  "**/vendor/**",
  "**/.git/**",
  "**/.mp/**",
  "**/dist/**",
  "**/build/**",
  "**/out/**",
  "**/.next/**",
  "**/.nuxt/**",
  "**/target/**",
  "**/coverage/**",
  "**/__pycache__/**",
  "**/.venv/**",
  "**/venv/**",
  "**/.tox/**",
  "**/.gradle/**",
  "**/.idea/**",
  "**/.vscode/**",
  "**/*.lock",
  "**/package-lock.json",
  "**/pnpm-lock.yaml",
  "**/yarn.lock",
  "**/Cargo.lock",
  "**/composer.lock",
  "**/Gemfile.lock",
  "**/*.min.js",
  "**/*.min.css",
  "**/*.map",
  "**/*.png",
  "**/*.jpg",
  "**/*.jpeg",
  "**/*.gif",
  "**/*.ico",
  "**/*.pdf",
  "**/*.zip",
  "**/*.tar",
  "**/*.gz",
  "**/*.wasm",
  "**/*.exe",
  "**/*.dll",
  "**/*.so",
  "**/*.dylib",
];

const defaultTool = (name: string, description: string, inputSchema: Record<string, unknown>): ToolDef => ({
  name,
  description,
  inputSchema,
  enabled: true,
});

export const DEFAULT_BLUEPRINT: AgentBlueprint = {
  name: "mp-default",
  description: "General-purpose coding assistant",
  tools: [
    defaultTool("read_file", "Read file contents at a path.", {
      type: "object",
      properties: { path: { type: "string" } },
      required: ["path"],
    }),
    defaultTool("write_file", "Create or overwrite a file.", {
      type: "object",
      properties: {
        path: { type: "string" },
        content: { type: "string" },
      },
      required: ["path", "content"],
    }),
    defaultTool("list_dir", "List files in a directory.", {
      type: "object",
      properties: { path: { type: "string" } },
      required: ["path"],
    }),
    defaultTool("grep", "Search for a pattern in the repo.", {
      type: "object",
      properties: {
        pattern: { type: "string" },
        path: { type: "string" },
      },
      required: ["pattern"],
    }),
    defaultTool("run_terminal_cmd", "Run a shell command (sandboxed).", {
      type: "object",
      properties: { command: { type: "string" } },
      required: ["command"],
    }),
  ],
  permissions: [
    { id: "deny-node-modules", type: "path_deny", pattern: "**/node_modules/**" },
    { id: "deny-git", type: "path_deny", pattern: "**/.git/**" },
  ],
  excludePatterns: DEFAULT_EXCLUDE_PATTERNS,
  config: {
    model: "default",
    max_turns: "64",
  },
  skills: DEFAULT_SKILLS,
  subagents: DEFAULT_SUBAGENT_DEFS,
};

/**
 * Apply a blueprint's tools, permissions, patterns, config, and seed messages atomically.
 * All operations (including message inserts) are within a single transaction.
 */
export function applyBlueprint(db: AgentDB, blueprint: AgentBlueprint): void {
  const now = Date.now();
  const tx = db.db.transaction(() => {
    for (const tool of blueprint.tools) {
      const definition = JSON.stringify({
        name: tool.name,
        description: tool.description,
        inputSchema: tool.inputSchema,
        enabled: tool.enabled ?? true,
        config: tool.config,
      });
      const cfg = tool.config != null ? JSON.stringify(tool.config) : null;
      db.db
        .prepare(`INSERT OR REPLACE INTO tools (name, definition, enabled, config) VALUES (?,?,?,?)`)
        .run(tool.name, definition, tool.enabled === false ? 0 : 1, cfg);
    }

    for (const perm of blueprint.permissions) {
      db.db
        .prepare(`INSERT OR REPLACE INTO permissions (id, type, pattern, created_at) VALUES (?,?,?,?)`)
        .run(perm.id, perm.type, perm.pattern, now);
    }

    for (const pattern of blueprint.excludePatterns) {
      db.db
        .prepare(`INSERT OR REPLACE INTO exclude_patterns (pattern, source) VALUES (?,?)`)
        .run(pattern, "blueprint");
    }

    for (const [key, value] of Object.entries(blueprint.config)) {
      db.db.prepare(`INSERT OR REPLACE INTO config (key, value) VALUES (?,?)`).run(key, value);
    }

    if (blueprint.systemInstructions != null && blueprint.systemInstructions.length > 0) {
      db.db
        .prepare(`INSERT OR REPLACE INTO config (key, value) VALUES (?,?)`)
        .run("system_instructions", blueprint.systemInstructions);

      const id = generateUUIDv7();
      db.db
        .prepare(
          `INSERT INTO messages (id, turn, role, content, tool_calls, tool_call_id, tokens_in, tokens_out, cost_usd, session_id, created_at)
           VALUES (?,?,?,?,?,?,?,?,?,?,?)`,
        )
        .run(id, 0, "system", blueprint.systemInstructions, null, null, null, null, null, null, now);
    }

    if (blueprint.name) {
      db.db.prepare(`INSERT OR REPLACE INTO config (key, value) VALUES (?,?)`).run("blueprint_name", blueprint.name);
    }

    if (blueprint.description) {
      db.db.prepare(`INSERT OR REPLACE INTO config (key, value) VALUES (?,?)`).run("blueprint_description", blueprint.description);
    }

    if (blueprint.seedMessages != null) {
      for (const msg of blueprint.seedMessages) {
        const id = generateUUIDv7();
        db.db
          .prepare(
            `INSERT INTO messages (id, turn, role, content, tool_calls, tool_call_id, tokens_in, tokens_out, cost_usd, session_id, created_at)
             VALUES (?,?,?,?,?,?,?,?,?,?,?)`,
          )
          .run(
            id,
            msg.turn,
            msg.role,
            msg.content ?? null,
            msg.toolCalls ?? null,
            msg.toolCallId ?? null,
            msg.tokensIn ?? null,
            msg.tokensOut ?? null,
            msg.costUsd ?? null,
            null,
            now,
          );
      }
    }

    if (blueprint.skills != null) {
      for (const skill of blueprint.skills) {
        db.db
          .prepare(
            `INSERT OR REPLACE INTO skills (name, description, instructions, source, created_at)
             VALUES (?, ?, ?, ?, ?)`,
          )
          .run(skill.name, skill.description, skill.instructions, skill.source ?? "blueprint", now);
      }
    }

    if (blueprint.subagents != null) {
      for (const sa of blueprint.subagents) {
        db.db
          .prepare(
            `INSERT OR REPLACE INTO subagent_defs
             (name, skill, description, allowed_tools, max_iterations, max_cost_usd, source, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)`,
          )
          .run(
            sa.name,
            sa.skill,
            sa.description,
            JSON.stringify(sa.allowedTools),
            sa.maxIterations ?? 10,
            sa.maxCostUsd ?? null,
            sa.source ?? "blueprint",
            now,
          );
      }
    }
  });

  try {
    tx();
  } catch (e) {
    throw sqlError("applyBlueprint", e);
  }
}

export function getEnabledTools(db: AgentDB): ToolDef[] {
  try {
    const rows = db.db.prepare(`SELECT definition FROM tools WHERE enabled = 1 ORDER BY name`).all() as {
      definition: string;
    }[];
    const out: ToolDef[] = [];
    for (const r of rows) {
      try {
        const parsed = JSON.parse(r.definition) as ToolDef;
        out.push(parsed);
      } catch {
        /* skip malformed */
      }
    }
    return out;
  } catch (e) {
    throw sqlError("getEnabledTools", e);
  }
}

export function getPermissions(db: AgentDB): Permission[] {
  try {
    return db.db
      .prepare(`SELECT id, type, pattern FROM permissions ORDER BY created_at ASC`)
      .all()
      .map((r) => {
        const row = r as { id: string; type: string; pattern: string };
        return {
          id: row.id,
          type: row.type as Permission["type"],
          pattern: row.pattern,
        };
      });
  } catch (e) {
    throw sqlError("getPermissions", e);
  }
}

// ── New agent definition helpers ────────────────────────────────────────

/** Alias for DEFAULT_BLUEPRINT — the canonical name going forward. */
export const DEFAULT_AGENT_DEF: AgentDef = DEFAULT_BLUEPRINT;

/**
 * Parse flat permission keys (deny_paths, deny_tools, allow_paths) into
 * the internal Permission[] format.
 */
export function parseFlatPermissions(data: Record<string, unknown>): Permission[] {
  const perms: Permission[] = [];
  const add = (type: Permission["type"], patterns: unknown) => {
    if (!Array.isArray(patterns)) return;
    for (const p of patterns) {
      if (typeof p === "string") {
        perms.push({ id: `${type}:${p}`, type, pattern: p });
      }
    }
  };
  add("path_deny", data.deny_paths);
  add("path_allow", data.allow_paths);
  add("tool_deny", data.deny_tools);
  add("tool_allow", data.allow_tools);
  return perms;
}

/** Content for the auto-generated `.mp/agents/default.md`. */
export const DEFAULT_AGENT_MD = `---
name: default
description: General-purpose coding assistant
tools:
  - read_file
  - write_file
  - list_dir
  - grep
  - run_terminal_cmd
max_turns: 64
---

You are a skilled software engineer. Help the user with coding tasks
including writing, debugging, refactoring, and explaining code. Use the
available tools to read files, search the codebase, and make changes.

Be concise. Prefer showing code over explaining it. When making changes,
always read the relevant file first to understand context.
`;

/** Content for the auto-generated `.mp/agents/_global.yaml`. */
export const DEFAULT_GLOBAL_YAML = `# .mp/agents/_global.yaml
# Repo-wide defaults applied to all agents.

# Paths the agent cannot read or write (glob patterns)
deny_paths:
  - "**/.git/**"
  - "**/node_modules/**"

# Tools the agent cannot use (exact name or glob)
# deny_tools:
#   - "run_terminal_cmd"

# If set, ONLY these paths are accessible (allowlist mode)
# allow_paths:
#   - "src/**"

# Files to skip during indexing and search (glob patterns)
exclude_patterns:
  - "**/node_modules/**"
  - "**/.git/**"
  - "**/.mp/**"
  - "**/dist/**"
  - "**/build/**"
  - "**/out/**"
  - "**/.next/**"
  - "**/target/**"
  - "**/coverage/**"
  - "**/__pycache__/**"
  - "**/.venv/**"
  - "**/venv/**"
  - "**/*.lock"
  - "**/package-lock.json"
  - "**/*.min.js"
  - "**/*.min.css"
  - "**/*.map"
  - "**/*.png"
  - "**/*.jpg"
  - "**/*.gif"
  - "**/*.ico"
  - "**/*.pdf"
  - "**/*.zip"
  - "**/*.wasm"

# Default turn limit (agents can override)
max_turns: 64

# Default model (agents can override)
# model: claude-sonnet-4-6

# Knowledge extraction after sessions
extraction:
  enabled: true
  # model: claude-haiku
`;
