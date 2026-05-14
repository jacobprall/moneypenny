/**
 * Policy file loader — scans `.mp/policies/` for YAML files, parses policy
 * definitions, and syncs them into the policies table. File-sourced policies
 * are tracked by `source = 'file'` and checksummed for change detection.
 *
 * Convention: `base.yaml` is always loaded if it exists on disk, regardless
 * of the `only` filter. This provides a non-negotiable governance floor.
 */

import { createHash } from "crypto";
import { existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from "fs";
import { join, basename } from "path";
import YAML from "yaml";

import {
  createPolicy,
  deleteFilePolicies,
  listFilePolicies,
  type PolicyEffect,
} from "./policies";
import type { AgentDB } from "./types";

export interface PolicyFileEntry {
  name: string;
  effect: PolicyEffect;
  priority?: number;
  tool?: string;
  path?: string;
  cost?: string;
  args?: string;
  actor?: string;
  message?: string;
  enabled?: boolean;
}

export interface PolicySyncOptions {
  /**
   * If set, only load the named policy files (plus base.yaml).
   * `only: []` = base.yaml only (yolo mode).
   * If omitted, all `.yaml` files are loaded.
   */
  only?: string[];
}

export interface PolicyScanResult {
  added: number;
  updated: number;
  removed: number;
  errors: Array<{ file: string; message: string }>;
}

const BASE_FILENAME = "base.yaml";
const VALID_EFFECTS = new Set<string>(["allow", "deny", "audit", "confirm"]);

function sha256(content: string): string {
  return createHash("sha256").update(content).digest("hex");
}

function stripExt(filename: string): string {
  return filename.replace(/\.ya?ml$/, "");
}

function validateEntry(entry: unknown, file: string, index: number): PolicyFileEntry | string {
  if (typeof entry !== "object" || entry === null) {
    return `${file}[${index}]: policy must be an object`;
  }
  const e = entry as Record<string, unknown>;
  if (typeof e.name !== "string" || !e.name) {
    return `${file}[${index}]: 'name' is required`;
  }
  if (typeof e.effect !== "string" || !VALID_EFFECTS.has(e.effect)) {
    return `${file}[${index}]: 'effect' must be allow, deny, audit, or confirm`;
  }
  return {
    name: e.name,
    effect: e.effect as PolicyEffect,
    priority: typeof e.priority === "number" ? e.priority : undefined,
    tool: typeof e.tool === "string" ? e.tool : undefined,
    path: typeof e.path === "string" ? e.path : undefined,
    cost: typeof e.cost === "string" ? e.cost : undefined,
    args: typeof e.args === "string" ? e.args : undefined,
    actor: typeof e.actor === "string" ? e.actor : undefined,
    message: typeof e.message === "string" ? e.message : undefined,
    enabled: typeof e.enabled === "boolean" ? e.enabled : undefined,
  };
}

function parseFile(filePath: string): { entries: PolicyFileEntry[]; errors: string[] } {
  const file = basename(filePath);
  let content: string;
  try {
    content = readFileSync(filePath, "utf8");
  } catch (e) {
    return { entries: [], errors: [`${file}: ${e instanceof Error ? e.message : String(e)}`] };
  }

  let parsed: unknown;
  try {
    parsed = YAML.parse(content);
  } catch (e) {
    return { entries: [], errors: [`${file}: invalid YAML — ${e instanceof Error ? e.message : String(e)}`] };
  }

  if (parsed == null) {
    return { entries: [], errors: [] };
  }

  const items: unknown[] = Array.isArray(parsed) ? parsed : [parsed];
  const entries: PolicyFileEntry[] = [];
  const errors: string[] = [];

  for (let i = 0; i < items.length; i++) {
    const result = validateEntry(items[i], file, i);
    if (typeof result === "string") {
      errors.push(result);
    } else {
      entries.push(result);
    }
  }

  return { entries, errors };
}

/**
 * Determine which YAML files to load based on the `only` filter.
 *
 * - No filter → all `.yaml` files in the directory
 * - `only: ["readonly"]` → `base.yaml` + `readonly.yaml`
 * - `only: []` → `base.yaml` only
 *
 * `base.yaml` is always included if it exists on disk.
 */
function resolveFileList(
  policiesDir: string,
  opts?: PolicySyncOptions,
): { files: string[]; errors: Array<{ file: string; message: string }> } {
  const allFiles = readdirSync(policiesDir).filter(
    (f) => (f.endsWith(".yaml") || f.endsWith(".yml")) && !f.startsWith("."),
  );

  if (!opts?.only) {
    return { files: allFiles, errors: [] };
  }

  const requested = new Set(opts.only.map((n) => n.toLowerCase()));
  const selected = new Set<string>();
  const errors: Array<{ file: string; message: string }> = [];

  if (allFiles.includes(BASE_FILENAME)) {
    selected.add(BASE_FILENAME);
  }

  for (const name of requested) {
    const match = allFiles.find((f) => stripExt(f).toLowerCase() === name);
    if (match) {
      selected.add(match);
    } else {
      errors.push({ file: name, message: `policy file not found: ${name}.yaml` });
    }
  }

  return { files: [...selected], errors };
}

/**
 * Sync policy files from `.mp/policies/` into the agent DB.
 *
 * - `syncPolicyFiles(db, dir)` — loads base.yaml + all other files
 * - `syncPolicyFiles(db, dir, { only: ["readonly"] })` — loads base.yaml + readonly.yaml
 * - `syncPolicyFiles(db, dir, { only: [] })` — loads base.yaml only (yolo mode)
 */
export function syncPolicyFiles(
  db: AgentDB,
  policiesDir: string,
  opts?: PolicySyncOptions,
): PolicyScanResult {
  const result: PolicyScanResult = { added: 0, updated: 0, removed: 0, errors: [] };

  if (!existsSync(policiesDir)) {
    mkdirSync(policiesDir, { recursive: true });
    scaffoldDefaults(policiesDir);
  }

  const { files, errors: resolveErrors } = resolveFileList(policiesDir, opts);
  result.errors.push(...resolveErrors);

  const seenFiles = new Set<string>();

  for (const file of files) {
    const filePath = join(policiesDir, file);
    seenFiles.add(filePath);

    let content: string;
    try {
      content = readFileSync(filePath, "utf8");
    } catch {
      continue;
    }
    const checksum = sha256(content);

    const existing = listFilePolicies(db, filePath);
    if (existing.length > 0 && existing[0]?.checksum === checksum) {
      continue;
    }

    const { entries, errors } = parseFile(filePath);
    for (const err of errors) {
      result.errors.push({ file, message: err });
    }

    deleteFilePolicies(db, filePath);
    if (existing.length > 0) {
      result.removed += existing.length;
    }

    for (const entry of entries) {
      createPolicy(db, {
        name: entry.name,
        effect: entry.effect,
        priority: entry.priority ?? 0,
        toolPattern: entry.tool ?? null,
        pathPattern: entry.path ?? null,
        costCondition: entry.cost ?? null,
        argsPattern: entry.args ?? null,
        actorPattern: entry.actor ?? null,
        message: entry.message ?? null,
        enabled: entry.enabled === false ? 0 : 1,
        source: "file",
        filePath,
        checksum,
      });
      if (existing.length > 0) {
        result.updated++;
      } else {
        result.added++;
      }
    }
  }

  const allFilePolicies = listFilePolicies(db);
  const orphanFiles = new Set<string>();
  for (const p of allFilePolicies) {
    if (p.filePath && !seenFiles.has(p.filePath)) {
      orphanFiles.add(p.filePath);
    }
  }
  for (const fp of orphanFiles) {
    result.removed += deleteFilePolicies(db, fp);
  }

  return result;
}

// ---------------------------------------------------------------------------
// Default policy files scaffolded on first init
// ---------------------------------------------------------------------------

const BASE_POLICIES = `# Base security policies — always loaded for every agent.
# Edit or extend these rules. They apply even when an agent
# specifies policies: [] (yolo mode). Delete this file to remove
# the governance floor entirely (visible in git).
#
# Fields:
#   name     (required) — human-readable policy name
#   effect   (required) — allow | deny | audit | confirm
#   priority           — higher wins (default 0)
#   tool               — glob pattern matching tool names
#   path               — glob pattern matching file paths
#   cost               — e.g. "session_cost > 5.00"
#   args               — regex tested against serialized tool arguments
#   actor              — glob pattern matching actor identity
#   message            — reason shown when policy triggers
#   enabled            — true | false (default true)

- name: protect-env-files
  effect: deny
  path: "*.env*"
  message: Prevent reading or writing environment files

- name: no-force-push
  effect: deny
  tool: bash
  args: "push.*--force|push.*-f"
  message: Block force-push to any remote

- name: no-rm-rf
  effect: deny
  tool: bash
  args: "rm\\\\s+-rf\\\\s+/"
  message: Block recursive delete from root

- name: audit-all-bash
  effect: audit
  tool: bash
  message: Log all shell commands for review
`;

const READONLY_POLICIES = `# Read-only agent profile. Reference from an agent definition:
#
#   ---
#   name: PR Reviewer
#   policies: [readonly]
#   ---
#
# base.yaml is always loaded alongside any referenced policies.

- name: no-file-writes
  effect: deny
  tool: file_write
  message: This agent is read-only

- name: no-file-edits
  effect: deny
  tool: file_edit
  message: This agent is read-only

- name: no-bash
  effect: deny
  tool: bash
  message: This agent cannot run shell commands
`;

function scaffoldDefaults(policiesDir: string): void {
  const basePath = join(policiesDir, "base.yaml");
  if (!existsSync(basePath)) {
    writeFileSync(basePath, BASE_POLICIES, "utf8");
  }
  const readonlyPath = join(policiesDir, "readonly.yaml");
  if (!existsSync(readonlyPath)) {
    writeFileSync(readonlyPath, READONLY_POLICIES, "utf8");
  }
}

export function getPoliciesDir(mpDir: string): string {
  return join(mpDir, "policies");
}
