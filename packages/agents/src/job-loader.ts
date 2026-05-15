/**
 * Load job definitions from `.mp/jobs/*.yaml`, sync to the `jobs` table.
 */

import cronParser from "cron-parser";
import { createHash } from "node:crypto";
import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import YAML from "yaml";
import type { AgentDB } from "@moneypenny/db";
import type { Database } from "bun:sqlite";
import * as jobsRepo from "./jobs-repo.js";
import type { Job } from "./jobs-repo.js";
import {
  AGENT_RUN_OPERATION,
  CUSTOM_RUN_OPERATION,
  INDEX_RUN_OPERATION,
  PIPELINE_RUN_OPERATION,
  SYNC_RUN_OPERATION,
} from "./operations.js";

const OPS: ReadonlySet<string> = new Set([
  AGENT_RUN_OPERATION,
  PIPELINE_RUN_OPERATION,
  INDEX_RUN_OPERATION,
  SYNC_RUN_OPERATION,
  CUSTOM_RUN_OPERATION,
]);

export interface MpJobFileRecord {
  name: string;
  description: string | null;
  schedule: string;
  operation: string;
  payload: Record<string, unknown>;
  enabled: boolean;
  timeoutMs: number;
  overlapPolicy: string;
  maxRetries: number;
  sourceBasename: string;
}

function listYamlFiles(dir: string): string[] {
  if (!existsSync(dir)) return [];
  return readdirSync(dir)
    .filter((f) => (f.endsWith(".yaml") || f.endsWith(".yml")) && !f.startsWith("."))
    .map((f) => join(dir, f))
    .filter((p) => {
      try {
        return statSync(p).isFile();
      } catch {
        return false;
      }
    });
}

function syncDigest(rec: MpJobFileRecord): string {
  return createHash("sha256")
    .update(
      JSON.stringify({
        name: rec.name,
        description: rec.description,
        schedule: rec.schedule,
        operation: rec.operation,
        payload: rec.payload,
        enabled: rec.enabled,
        timeoutMs: rec.timeoutMs,
        overlapPolicy: rec.overlapPolicy,
        maxRetries: rec.maxRetries,
        sourceBasename: rec.sourceBasename,
      }),
    )
    .digest("hex");
}

function storedPayload(userPayload: Record<string, unknown>, basename: string, digest: string): string {
  return JSON.stringify({
    ...userPayload,
    __mp_job_file: basename,
    __mp_sync_digest: digest,
  });
}

function parseJobFile(path: string, basename: string): MpJobFileRecord {
  const content = readFileSync(path, "utf8");
  const doc = YAML.parse(content) as Record<string, unknown> | null;
  if (!doc || typeof doc !== "object") {
    throw new Error(`${basename}: invalid YAML root`);
  }
  const name = doc.name;
  const schedule = doc.schedule;
  const operation = doc.operation;
  if (typeof name !== "string" || !name.trim()) {
    throw new Error(`${basename}: missing string field "name"`);
  }
  if (typeof schedule !== "string" || !schedule.trim()) {
    throw new Error(`${basename}: missing string field "schedule"`);
  }
  if (typeof operation !== "string" || !OPS.has(operation)) {
    throw new Error(`${basename}: "operation" must be one of: ${[...OPS].join(", ")}`);
  }

  const description =
    doc.description === undefined || doc.description === null
      ? null
      : String(doc.description);

  let payload: Record<string, unknown> = {};
  if (doc.payload !== undefined && doc.payload !== null) {
    if (typeof doc.payload !== "object" || Array.isArray(doc.payload)) {
      throw new Error(`${basename}: "payload" must be an object`);
    }
    payload = { ...(doc.payload as Record<string, unknown>) };
  }

  const enabled = doc.enabled === false ? false : true;
  const timeoutMs =
    typeof doc.timeout_ms === "number" && Number.isFinite(doc.timeout_ms) ? doc.timeout_ms : 30_000;
  const overlapPolicy = typeof doc.overlap_policy === "string" ? doc.overlap_policy : "skip";
  const maxRetries =
    typeof doc.max_retries === "number" && Number.isFinite(doc.max_retries) ? doc.max_retries : 3;

  return {
    name: name.trim(),
    description,
    schedule: schedule.trim(),
    operation,
    payload,
    enabled,
    timeoutMs,
    overlapPolicy,
    maxRetries,
    sourceBasename: basename,
  };
}

function findJobByMpSource(db: Database, basename: string): Job | null {
  for (const j of jobsRepo.listJobsWithMpFileSource(db)) {
    try {
      const p = j.payload ? (JSON.parse(j.payload) as { __mp_job_file?: string }) : {};
      if (p.__mp_job_file === basename) return j;
    } catch {
      /* */
    }
  }
  return null;
}

export function syncJobFiles(agentDb: AgentDB, jobsDir: string): { added: number; updated: number; disabled: number } {
  const paths = listYamlFiles(jobsDir);
  const presentBasenames = new Set(paths.map((p) => p.split(/[/\\]/).pop()!));
  let added = 0;
  let updated = 0;
  let disabled = 0;

  return agentDb.writer.exclusive((db) => {
    const now = Date.now();

    for (const filePath of paths) {
      const basename = filePath.split(/[/\\]/).pop()!;
      let rec: MpJobFileRecord;
      try {
        rec = parseJobFile(filePath, basename);
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        process.stderr.write(`[mp jobs] skip ${basename}: ${msg}\n`);
        continue;
      }

      const digest = syncDigest(rec);
      const payloadStr = storedPayload(rec.payload, basename, digest);

      let nextRunAt: number;
      try {
        nextRunAt = cronParser.parse(rec.schedule, { tz: undefined }).next().toDate().getTime();
      } catch (e) {
        process.stderr.write(
          `[mp jobs] skip ${basename}: invalid schedule — ${e instanceof Error ? e.message : String(e)}\n`,
        );
        continue;
      }

      const existing = findJobByMpSource(db, basename);

      if (!existing) {
        const byName = jobsRepo.getByName(db, rec.name);
        if (byName) {
          process.stderr.write(
            `[mp jobs] skip ${basename}: job name "${rec.name}" already exists (id ${byName.id}); choose a unique name.\n`,
          );
          continue;
        }
        const id = crypto.randomUUID();
        jobsRepo.insert(db, {
          id,
          name: rec.name,
          description: rec.description,
          schedule: rec.schedule,
          operation: rec.operation,
          payload: payloadStr,
          nextRunAt,
          overlapPolicy: rec.overlapPolicy,
          maxRetries: rec.maxRetries,
          timeoutMs: rec.timeoutMs,
          status: "active",
          enabled: rec.enabled ? 1 : 0,
          createdAt: now,
          updatedAt: now,
        });
        added += 1;
        continue;
      }

      let prevDigest = "";
      try {
        const p = existing.payload
          ? (JSON.parse(existing.payload) as { __mp_sync_digest?: string })
          : {};
        prevDigest = typeof p.__mp_sync_digest === "string" ? p.__mp_sync_digest : "";
      } catch {
        prevDigest = "";
      }

      if (prevDigest === digest) {
        continue;
      }

      jobsRepo.updateJob(db, existing.id, {
        name: rec.name,
        description: rec.description,
        schedule: rec.schedule,
        operation: rec.operation,
        payload: payloadStr,
        nextRunAt,
        overlapPolicy: rec.overlapPolicy,
        maxRetries: rec.maxRetries,
        timeoutMs: rec.timeoutMs,
        status: "active",
        enabled: rec.enabled ? 1 : 0,
      });
      updated += 1;
    }

    for (const j of jobsRepo.listJobsWithMpFileSource(db)) {
      let basename: string | null = null;
      try {
        const p = j.payload ? (JSON.parse(j.payload) as { __mp_job_file?: string }) : {};
        basename = typeof p.__mp_job_file === "string" ? p.__mp_job_file : null;
      } catch {
        basename = null;
      }
      if (!basename || presentBasenames.has(basename)) continue;
      if (j.enabled === 0) continue;
      jobsRepo.updateJob(db, j.id, { enabled: 0 });
      disabled += 1;
    }

    return { added, updated, disabled };
  });
}
