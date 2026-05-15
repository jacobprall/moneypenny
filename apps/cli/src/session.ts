import { existsSync, mkdirSync, readdirSync, renameSync, writeFileSync } from "node:fs";
import * as path from "node:path";
import {
  createAgentDB,
  createWorkspaceDB,
  closeAgentDB,
  getConfig,
  DEFAULT_BLUEPRINT,
  DEFAULT_AGENT_MD,
  DEFAULT_GLOBAL_YAML,
  discoverAgentDefs,
  type AgentDB,
  type AgentBlueprint,
  type AgentDefInfo,
  type WorkspaceDB,
} from "@moneypenny/db";
import { Database } from "bun:sqlite";

export function getMpDir(repoPath: string): string {
  return path.join(repoPath, ".mp");
}

/** Single DB path: `.mp/mp.db`. */
export function getDbPath(repoPath: string, _agentName?: string): string {
  return path.join(getMpDir(repoPath), "mp.db");
}

export function getBlueprintsDir(repoPath: string): string {
  return path.join(getMpDir(repoPath), "blueprints");
}

/**
 * Open the shared workspace index DB. Created once per workspace at
 * `.mp/workspace.sqlite`; all sessions share the same index.
 */
export function openWorkspace(repoPath: string): WorkspaceDB {
  return createWorkspaceDB(repoPath);
}

export interface AgentInfo {
  name: string;
  dbPath: string;
  blueprintName: string | null;
  blueprintDescription: string | null;
}

/** @deprecated List agent DBs — replaced by listAgentDefs(). */
export function listAgents(repoPath: string): AgentInfo[] {
  const agentsDir = path.join(getMpDir(repoPath), "agents");
  const results: AgentInfo[] = [];

  if (existsSync(agentsDir)) {
    try {
      const files = readdirSync(agentsDir).filter((f) => f.endsWith(".db"));
      for (const f of files) {
        const name = f.replace(".db", "");
        const info = readAgentInfo(name, path.join(agentsDir, f));
        if (info) results.push(info);
      }
    } catch { /* skip */ }
  }

  return results;
}

function readAgentInfo(name: string, dbPath: string): AgentInfo | null {
  let db: AgentDB | undefined;
  try {
    db = createAgentDB(dbPath);
    const blueprintName = getConfig(db, "blueprint_name") ?? null;
    const blueprintDescription = getConfig(db, "blueprint_description") ?? null;
    return { name, dbPath, blueprintName, blueprintDescription };
  } catch {
    return { name, dbPath, blueprintName: null, blueprintDescription: null };
  } finally {
    if (db) try { closeAgentDB(db); } catch { /* best effort */ }
  }
}

/**
 * Open the single per-repo database at `.mp/mp.db`.
 * The `name` option is accepted for backward compat but ignored — all
 * agents share one DB file now.
 */
export function openAgent(
  repoPath: string,
  opts?: { name?: string; blueprint?: AgentBlueprint; workspace?: WorkspaceDB },
): AgentDB {
  const dbPath = getDbPath(repoPath);
  const dir = path.dirname(dbPath);
  if (!existsSync(dir)) {
    mkdirSync(dir, { recursive: true });
  }

  return createAgentDB(dbPath, {
    repoPath,
    workspace: opts?.workspace,
    blueprint: opts?.blueprint ?? DEFAULT_BLUEPRINT,
  });
}

/** @deprecated Use openAgent instead. Compat shim for other commands. */
export function openSession(
  repoPath: string,
  opts?: { session?: string; forceNew?: boolean; workspace?: WorkspaceDB },
): AgentDB {
  return openAgent(repoPath, {
    workspace: opts?.workspace,
  });
}

/** List agent definition files (.md) in `.mp/agents/`. */
export function listAgentDefs(repoPath: string): AgentDefInfo[] {
  return discoverAgentDefs(repoPath);
}

/**
 * Ensure `.mp/agents/default.md` and `.mp/agents/_global.yaml` exist.
 * Called on startup so users always have a working baseline.
 */
export function ensureAgentDefaults(repoPath: string): void {
  const agentsDir = path.join(getMpDir(repoPath), "agents");
  if (!existsSync(agentsDir)) {
    mkdirSync(agentsDir, { recursive: true });
  }

  const defaultMd = path.join(agentsDir, "default.md");
  if (!existsSync(defaultMd)) {
    writeFileSync(defaultMd, DEFAULT_AGENT_MD, "utf8");
  }

  const globalYaml = path.join(agentsDir, "_global.yaml");
  if (!existsSync(globalYaml)) {
    writeFileSync(globalYaml, DEFAULT_GLOBAL_YAML, "utf8");
  }
}

export { migrateToSingleDb } from "./migrate.js";
