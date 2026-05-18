import { z } from "zod";
import type { Database } from "bun:sqlite";
import { readdir, stat } from "node:fs/promises";
import { join, basename } from "node:path";

const AgentDefSchema = z.object({
  name: z.string(),
  model: z.string().optional(),
  system_prompt: z.string().optional(),
  tools: z.array(z.string()).optional(),
  trigger_on: z.enum(["manual", "session_close", "schedule"]).optional(),
});

const PolicySchema = z.object({
  name: z.string(),
  effect: z.enum(["allow", "warn", "deny"]).default("warn"),
  description: z.string(),
  conditions: z.record(z.unknown()).optional(),
});

const JobSchema = z.object({
  name: z.string(),
  schedule: z.string().optional(),
  agent: z.string(),
  action: z.string().optional(),
  enabled: z.boolean().default(true),
});

const ConventionsSchema = z.array(
  z.object({
    name: z.string(),
    category: z.string().default("general"),
    description: z.string(),
  }),
);

function idFromPath(filePath: string): string {
  return basename(filePath, ".toml");
}

function tableExists(db: Database, table: string): boolean {
  const row = db
    .query<{ n: number }, [string]>(
      `SELECT COUNT(1) as n FROM sqlite_master WHERE type = 'table' AND name = ?`,
    )
    .get(table);
  return (row?.n ?? 0) > 0;
}

export async function syncConfigFile(
  db: Database,
  filePath: string,
): Promise<void> {
  if (
    filePath.includes("/agents/") &&
    tableExists(db, "agent_defs")
  ) {
    const raw = await Bun.file(filePath).text();
    const parsed = Bun.TOML.parse(raw);
    const def = AgentDefSchema.parse(parsed);
    const id = idFromPath(filePath);
    db.query(
      `INSERT OR REPLACE INTO agent_defs (id, name, model, system_prompt, tools, trigger_on, source_path, updated_at)
       VALUES (?, ?, ?, ?, ?, ?, ?, unixepoch())`,
    ).run(
      id,
      def.name,
      def.model ?? null,
      def.system_prompt ?? null,
      def.tools ? JSON.stringify(def.tools) : null,
      def.trigger_on ?? null,
      filePath,
    );
  } else if (filePath.includes("/policies/")) {
    const raw = await Bun.file(filePath).text();
    const parsed = Bun.TOML.parse(raw);
    const policy = PolicySchema.parse(parsed);
    const id = idFromPath(filePath);
    db.query(
      `INSERT OR REPLACE INTO policies (id, name, effect, description, conditions, enabled, source_path, updated_at)
       VALUES (?, ?, ?, ?, ?, 1, ?, unixepoch())`,
    ).run(
      id,
      policy.name,
      policy.effect,
      policy.description,
      policy.conditions ? JSON.stringify(policy.conditions) : null,
      filePath,
    );
  } else if (filePath.includes("/jobs/") && tableExists(db, "jobs")) {
    const raw = await Bun.file(filePath).text();
    const parsed = Bun.TOML.parse(raw);
    const job = JobSchema.parse(parsed);
    const id = idFromPath(filePath);
    db.query(
      `INSERT OR REPLACE INTO jobs (id, name, schedule, agent_name, action, enabled, source_path, updated_at)
       VALUES (?, ?, ?, ?, ?, ?, ?, unixepoch())`,
    ).run(
      id,
      job.name,
      job.schedule ?? null,
      job.agent,
      job.action ?? null,
      job.enabled ? 1 : 0,
      filePath,
    );
  } else if (basename(filePath) === "conventions.toml") {
    const raw = await Bun.file(filePath).text();
    const parsed = Bun.TOML.parse(raw);
    const items = Array.isArray(parsed) ? parsed : (parsed as any).convention;
    const conventions = ConventionsSchema.parse(items);
    const baseId = idFromPath(filePath);
    db.transaction(() => {
      db.query("DELETE FROM conventions WHERE id LIKE ?").run(`${baseId}:%`);
      for (let i = 0; i < conventions.length; i++) {
        const c = conventions[i];
        db.query(
          `INSERT INTO conventions (id, name, category, description, confidence, created_at)
           VALUES (?, ?, ?, ?, 1.0, unixepoch())`,
        ).run(`${baseId}:${i}`, c.name, c.category, c.description);
      }
    })();
  }
}

export async function removeConfig(
  db: Database,
  filePath: string,
): Promise<void> {
  if (filePath.includes("/agents/") && tableExists(db, "agent_defs")) {
    db.query("DELETE FROM agent_defs WHERE source_path = ?").run(filePath);
  } else if (filePath.includes("/policies/")) {
    db.query("DELETE FROM policies WHERE source_path = ?").run(filePath);
  } else if (filePath.includes("/jobs/") && tableExists(db, "jobs")) {
    db.query("DELETE FROM jobs WHERE source_path = ?").run(filePath);
  } else if (basename(filePath) === "conventions.toml") {
    db.query("DELETE FROM conventions WHERE id LIKE ?").run(
      `${idFromPath(filePath)}:%`,
    );
  }
}

async function findTomlFiles(dir: string): Promise<string[]> {
  const results: string[] = [];
  let entries: string[];
  try {
    entries = await readdir(dir);
  } catch {
    return results;
  }
  for (const entry of entries) {
    const full = join(dir, entry);
    const s = await stat(full);
    if (s.isDirectory()) {
      results.push(...(await findTomlFiles(full)));
    } else if (entry.endsWith(".toml")) {
      results.push(full);
    }
  }
  return results;
}

export async function syncAllConfigs(
  db: Database,
  configDir: string,
): Promise<number> {
  const files = await findTomlFiles(configDir);
  let count = 0;
  for (const file of files) {
    try {
      await syncConfigFile(db, file);
      count++;
    } catch (err) {
      console.error(`Failed to sync ${file}:`, err);
    }
  }
  return count;
}
