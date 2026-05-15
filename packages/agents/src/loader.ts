/**
 * Agent loader — scans `blueprintsDir` for directory-defined agent
 * definitions, parses `agent.md` frontmatter + body, validates, and upserts
 * an `agents` row.
 */

import cronParser from "cron-parser";
import matter from "gray-matter";
import { createHash } from "crypto";
import type { AgentDB } from "@moneypenny/db";
import {
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  statSync,
  writeFileSync,
} from "fs";
import { join } from "path";
import type { Database } from "bun:sqlite";

import * as repo from "./repository.js";
import * as jobsRepo from "./jobs-repo.js";
import { AGENT_RUN_OPERATION } from "./operations.js";
import {
  validateFrontmatter,
  validateAgentId,
  type AgentFrontmatter,
  type ValidationError,
} from "./schema.js";
import { DEFAULT_AGENTS } from "./defaults.js";

export interface LoaderOptions {
  agentDb: AgentDB;
  blueprintsDir: string;
  onChange?: (id: string, reason: "added" | "updated" | "removed" | "error") => void;
}

export interface LoadedAgent {
  id: string;
  status: "ok" | "error" | "deleted";
  errors?: ValidationError[];
  config?: AgentFrontmatter;
}

export interface ScanResult {
  loaded: LoadedAgent[];
  removed: string[];
}

function ensureDir(path: string): void {
  if (!existsSync(path)) {
    mkdirSync(path, { recursive: true });
  }
}

function sha256(content: string): string {
  return createHash("sha256").update(content).digest("hex");
}

function listAgentDirs(root: string): string[] {
  if (!existsSync(root)) return [];
  return readdirSync(root)
    .filter((name) => !name.startsWith(".") && !name.startsWith("_"))
    .map((name) => join(root, name))
    .filter((p) => {
      try {
        return statSync(p).isDirectory();
      } catch {
        return false;
      }
    });
}

export interface ParsedAgent {
  id: string;
  dirPath: string;
  agentMdPath: string;
  checksum: string;
  status: "ok" | "error";
  errors: ValidationError[];
  config: AgentFrontmatter | null;
  prompt: string;
  rawFrontmatter: unknown;
}

function parseAgentDir(dirPath: string): ParsedAgent {
  const id = dirPath.split("/").pop() ?? "";
  const agentMdPath = join(dirPath, "agent.md");

  const idError = validateAgentId(id);
  if (idError) {
    return {
      id,
      dirPath,
      agentMdPath,
      checksum: "",
      status: "error",
      errors: [idError],
      config: null,
      prompt: "",
      rawFrontmatter: null,
    };
  }

  if (!existsSync(agentMdPath)) {
    return {
      id,
      dirPath,
      agentMdPath,
      checksum: "",
      status: "error",
      errors: [{ field: "(file)", message: "missing agent.md" }],
      config: null,
      prompt: "",
      rawFrontmatter: null,
    };
  }

  let content: string;
  try {
    content = readFileSync(agentMdPath, "utf8");
  } catch (e) {
    return {
      id,
      dirPath,
      agentMdPath,
      checksum: "",
      status: "error",
      errors: [{ field: "(file)", message: e instanceof Error ? e.message : String(e) }],
      config: null,
      prompt: "",
      rawFrontmatter: null,
    };
  }

  const checksum = sha256(content);

  let parsed: ReturnType<typeof matter>;
  try {
    parsed = matter(content);
  } catch (e) {
    return {
      id,
      dirPath,
      agentMdPath,
      checksum,
      status: "error",
      errors: [{ field: "(frontmatter)", message: e instanceof Error ? e.message : String(e) }],
      config: null,
      prompt: "",
      rawFrontmatter: null,
    };
  }

  const validation = validateFrontmatter(parsed.data);
  if (!validation.ok || !validation.config) {
    return {
      id,
      dirPath,
      agentMdPath,
      checksum,
      status: "error",
      errors: validation.errors,
      config: validation.config ?? null,
      prompt: parsed.content.trim(),
      rawFrontmatter: parsed.data,
    };
  }

  return {
    id,
    dirPath,
    agentMdPath,
    checksum,
    status: "ok",
    errors: [],
    config: validation.config,
    prompt: parsed.content.trim(),
    rawFrontmatter: parsed.data,
  };
}

function syncJobForAgent(
  db: Database,
  agentId: string,
  parsed: ParsedAgent,
  previousJobId: string | null,
  effectiveEnabled: 0 | 1,
): string | null {
  const cfg = parsed.config;
  const shouldHaveJob = !!cfg && !!cfg.schedule;

  if (!shouldHaveJob) {
    if (previousJobId) {
      db.run("UPDATE jobs SET enabled = 0, updated_at = ? WHERE id = ?", [Date.now(), previousJobId]);
    }
    return null;
  }

  const now = Date.now();
  const nextRunAt = cronParser
    .parse(cfg!.schedule!, { tz: cfg!.timezone })
    .next()
    .toDate()
    .getTime();

  if (previousJobId) {
    const existing = jobsRepo.getById(db, previousJobId);
    if (existing) {
      db.run(
        `UPDATE jobs SET
           name = ?, description = ?, schedule = ?, operation = ?, payload = ?,
           next_run_at = ?, timeout_ms = ?, enabled = ?, status = 'active', updated_at = ?
         WHERE id = ?`,
        [
          `agent:${agentId}`,
          cfg!.description ?? null,
          cfg!.schedule!,
          AGENT_RUN_OPERATION,
          JSON.stringify({ agent_id: agentId }),
          nextRunAt,
          cfg!.timeout_ms,
          effectiveEnabled,
          now,
          previousJobId,
        ],
      );
      return previousJobId;
    }
  }

  const jobId = crypto.randomUUID();
  jobsRepo.insert(db, {
    id: jobId,
    name: `agent:${agentId}`,
    description: cfg!.description ?? null,
    schedule: cfg!.schedule!,
    operation: AGENT_RUN_OPERATION,
    payload: JSON.stringify({ agent_id: agentId }),
    nextRunAt,
    overlapPolicy: "skip",
    maxRetries: 0,
    timeoutMs: cfg!.timeout_ms,
    status: "active",
    enabled: effectiveEnabled,
    createdAt: now,
    updatedAt: now,
  });
  return jobId;
}

function scaffoldDefaults(blueprintsDir: string): void {
  const existing = listAgentDirs(blueprintsDir);
  if (existing.length > 0) return;

  for (const [id, content] of Object.entries(DEFAULT_AGENTS)) {
    const dir = join(blueprintsDir, id);
    mkdirSync(dir, { recursive: true });
    writeFileSync(join(dir, "agent.md"), content, "utf8");
  }
}

export function scan(options: LoaderOptions): ScanResult {
  const { agentDb, blueprintsDir, onChange } = options;
  return agentDb.writer.exclusive((db) => {
  const isNew = !existsSync(blueprintsDir);
  ensureDir(blueprintsDir);
  if (isNew) scaffoldDefaults(blueprintsDir);

  const dirs = listAgentDirs(blueprintsDir);
  const known = new Set(repo.allKnownIds(db));
  const seen = new Set<string>();
  const loaded: LoadedAgent[] = [];

  for (const dir of dirs) {
    const parsed = parseAgentDir(dir);
    seen.add(parsed.id);
    const existing = repo.getById(db, parsed.id);
    const isNew = !existing;
    const isChanged = existing && existing.checksum !== parsed.checksum;

    const enabledForWrite: 0 | 1 = existing
      ? ((existing.enabled as 0 | 1) ?? 0)
      : parsed.config?.enabled === false
        ? 0
        : 1;

    const jobId = syncJobForAgent(db, parsed.id, parsed, existing?.jobId ?? null, enabledForWrite);

    repo.upsert(db, {
      id: parsed.id,
      dirPath: parsed.dirPath,
      agentMdPath: parsed.agentMdPath,
      checksum: parsed.checksum,
      name: parsed.config?.name ?? parsed.id,
      description: parsed.config?.description ?? null,
      schedule: parsed.config?.schedule ?? null,
      timezone: parsed.config?.timezone ?? null,
      enabled: enabledForWrite,
      status: parsed.status,
      validationErrors: parsed.errors.length > 0 ? JSON.stringify(parsed.errors) : null,
      configJson: JSON.stringify(parsed.config ?? parsed.rawFrontmatter ?? {}),
      prompt: parsed.prompt,
      jobId,
    });

    loaded.push({
      id: parsed.id,
      status: parsed.status,
      errors: parsed.errors.length ? parsed.errors : undefined,
      config: parsed.config ?? undefined,
    });

    if (onChange) {
      if (parsed.status === "error") onChange(parsed.id, "error");
      else if (isNew) onChange(parsed.id, "added");
      else if (isChanged) onChange(parsed.id, "updated");
    }
  }

  const removed: string[] = [];
  for (const id of known) {
    if (!seen.has(id)) {
      const existing = repo.getById(db, id);
      if (existing?.jobId) {
        db.run("UPDATE jobs SET enabled = 0, updated_at = ? WHERE id = ?", [Date.now(), existing.jobId]);
      }
      repo.markDeleted(db, id);
      removed.push(id);
      onChange?.(id, "removed");
    }
  }

  return { loaded, removed };
  });
}

export function rescanOne(options: LoaderOptions, dirPath: string): LoadedAgent {
  const { agentDb } = options;
  return agentDb.writer.exclusive((db) => {
    const parsed = parseAgentDir(dirPath);
    const existing = repo.getById(db, parsed.id);
    const isNew = !existing;
    const isChanged = existing && existing.checksum !== parsed.checksum;

    const enabledForWrite: 0 | 1 = existing
      ? ((existing.enabled as 0 | 1) ?? 0)
      : parsed.config?.enabled === false
        ? 0
        : 1;

    const jobId = syncJobForAgent(db, parsed.id, parsed, existing?.jobId ?? null, enabledForWrite);

    repo.upsert(db, {
      id: parsed.id,
      dirPath: parsed.dirPath,
      agentMdPath: parsed.agentMdPath,
      checksum: parsed.checksum,
      name: parsed.config?.name ?? parsed.id,
      description: parsed.config?.description ?? null,
      schedule: parsed.config?.schedule ?? null,
      timezone: parsed.config?.timezone ?? null,
      enabled: enabledForWrite,
      status: parsed.status,
      validationErrors: parsed.errors.length > 0 ? JSON.stringify(parsed.errors) : null,
      configJson: JSON.stringify(parsed.config ?? parsed.rawFrontmatter ?? {}),
      prompt: parsed.prompt,
      jobId,
    });

    const reason: "added" | "updated" | "error" =
      parsed.status === "error" ? "error" : isNew ? "added" : isChanged ? "updated" : "updated";
    options.onChange?.(parsed.id, reason);

    return {
      id: parsed.id,
      status: parsed.status,
      errors: parsed.errors.length ? parsed.errors : undefined,
      config: parsed.config ?? undefined,
    };
  });
}

export async function startWatcher(options: LoaderOptions): Promise<() => void> {
  const { blueprintsDir } = options;
  ensureDir(blueprintsDir);

  let chokidar: typeof import("chokidar") | null = null;
  try {
    chokidar = await import("chokidar");
  } catch {
    chokidar = null;
  }

  if (chokidar) {
    const watcher = chokidar.watch(blueprintsDir, {
      ignoreInitial: true,
      depth: 3,
      ignored: (p: string) => /\/\.|\/node_modules\//.test(p),
    }) as unknown as {
      on: (ev: string, fn: (path: string) => void) => unknown;
      close: () => Promise<void> | void;
    };

    const handle = (p: string) => {
      const rel = p.slice(blueprintsDir.length).replace(/^\/+/, "");
      const topSegment = rel.split("/")[0];
      if (!topSegment) {
        scan(options);
        return;
      }
      const dirPath = join(blueprintsDir, topSegment);
      if (!existsSync(dirPath)) {
        scan(options);
        return;
      }
      rescanOne(options, dirPath);
    };

    watcher.on("add", handle);
    watcher.on("change", handle);
    watcher.on("unlink", handle);
    watcher.on("addDir", handle);
    watcher.on("unlinkDir", handle);

    return () => {
      void watcher.close();
    };
  }

  const fs = await import("fs");
  const watcher = fs.watch(blueprintsDir, { recursive: true }, (_event, filename) => {
    if (!filename) {
      scan(options);
      return;
    }
    const topSegment = String(filename).split("/")[0];
    if (!topSegment) {
      scan(options);
      return;
    }
    const dirPath = join(blueprintsDir, topSegment);
    if (!existsSync(dirPath)) {
      scan(options);
      return;
    }
    rescanOne(options, dirPath);
  });

  return () => {
    watcher.close();
  };
}
