/**
 * Policy file loader — scans a directory of YAML files, parses policy
 * definitions, and syncs them into the policies table. File-sourced
 * policies are tracked by `source = 'file'` and checksummed for
 * change detection.
 */

import { createHash } from "crypto";
import { existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from "fs";
import { join, basename } from "path";
import YAML from "yaml";

import {
  createPolicy,
  deleteFilePolicies,
  listFilePolicies,
  updatePolicy,
  type PolicyEffect,
  type Policy,
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

export interface PolicyScanResult {
  added: number;
  updated: number;
  removed: number;
  errors: Array<{ file: string; message: string }>;
}

const VALID_EFFECTS = new Set<string>(["allow", "deny", "audit", "confirm"]);

function sha256(content: string): string {
  return createHash("sha256").update(content).digest("hex");
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
 * Scan `.swe/policies/` for YAML files and sync into the DB.
 * File-sourced policies are upserted; policies from removed files are deleted.
 */
export function syncPolicyFiles(db: AgentDB, policiesDir: string): PolicyScanResult {
  const result: PolicyScanResult = { added: 0, updated: 0, removed: 0, errors: [] };

  if (!existsSync(policiesDir)) {
    mkdirSync(policiesDir, { recursive: true });
    scaffoldDefaults(policiesDir);
  }

  const files = readdirSync(policiesDir).filter(
    (f) => (f.endsWith(".yaml") || f.endsWith(".yml")) && !f.startsWith("."),
  );

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

const DEFAULT_POLICIES = `# Default swe policies — edit or add new .yaml files to this directory.
# Each file can contain a single policy or a YAML array of policies.
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
#   message            — human-readable reason shown when policy triggers
#   enabled            — true | false (default true)

- name: protect-env-files
  effect: deny
  path: "*.env*"
  message: Prevent agent from reading or writing environment files

- name: no-force-push
  effect: deny
  tool: bash
  args: "push.*--force|push.*-f"
  message: Block force-push to any remote
`;

function scaffoldDefaults(policiesDir: string): void {
  const defaultFile = join(policiesDir, "defaults.yaml");
  if (!existsSync(defaultFile)) {
    writeFileSync(defaultFile, DEFAULT_POLICIES, "utf8");
  }
}

export function getPoliciesDir(sweDir: string): string {
  return join(sweDir, "policies");
}
