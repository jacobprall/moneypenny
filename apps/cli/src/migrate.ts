import { existsSync, mkdirSync, readdirSync, renameSync, writeFileSync } from "node:fs";
import * as path from "node:path";
import { Database } from "bun:sqlite";
import {
  createAgentDB,
  closeAgentDB,
  DEFAULT_BLUEPRINT,
  type AgentDB,
} from "@moneypenny/db";
import { getMpDir } from "./session.js";

/**
 * Migrate from per-agent `.db` files to single `mp.db`.
 * Detects old layout (`.mp/agents/*.db`), merges data into the unified
 * `mp.db`, creates agent definition `.md` files from blueprint config,
 * and backs up old DBs. Idempotent — skips if already migrated.
 */
export function migrateToSingleDb(repoPath: string): { migrated: boolean; agents: string[] } {
  const mpDir = getMpDir(repoPath);
  const agentsDir = path.join(mpDir, "agents");
  const mpDbPath = path.join(mpDir, "mp.db");

  if (!existsSync(agentsDir)) return { migrated: false, agents: [] };

  let oldDbFiles: string[];
  try {
    oldDbFiles = readdirSync(agentsDir).filter((f) => f.endsWith(".db"));
  } catch {
    return { migrated: false, agents: [] };
  }

  if (oldDbFiles.length === 0) return { migrated: false, agents: [] };

  const migratedAgents: string[] = [];
  let targetDb: AgentDB | undefined;

  try {
    targetDb = createAgentDB(mpDbPath, { repoPath, blueprint: DEFAULT_BLUEPRINT });
    targetDb.db.exec("BEGIN IMMEDIATE");

    for (const dbFile of oldDbFiles) {
      const agentName = dbFile.replace(".db", "");
      const oldDbPath = path.join(agentsDir, dbFile);
      let oldDb: Database | undefined;

      try {
        oldDb = new Database(oldDbPath, { readonly: true });

        const hasSessions = oldDb
          .prepare(`SELECT 1 FROM sqlite_master WHERE type='table' AND name='sessions' LIMIT 1`)
          .get();

        if (hasSessions) {
          const sessions = oldDb.prepare(`SELECT id, label, created_at, last_active_at, is_active FROM sessions`).all() as {
            id: string; label: string | null; created_at: number; last_active_at: number; is_active: number;
          }[];

          for (const s of sessions) {
            try {
              targetDb.db
                .prepare(`INSERT OR IGNORE INTO sessions (id, label, agent_name, created_at, last_active_at, is_active) VALUES (?,?,?,?,?,?)`)
                .run(s.id, s.label, agentName, s.created_at, s.last_active_at, s.is_active);
            } catch { /* skip duplicate */ }
          }

          const messages = oldDb.prepare(
            `SELECT id, turn, role, content, tool_calls, tool_call_id, tokens_in, tokens_out, cost_usd, session_id, created_at FROM messages`,
          ).all() as { id: string; turn: number; role: string; content: string | null; tool_calls: string | null; tool_call_id: string | null; tokens_in: number | null; tokens_out: number | null; cost_usd: number | null; session_id: string | null; created_at: number }[];

          for (const m of messages) {
            try {
              targetDb.db
                .prepare(`INSERT OR IGNORE INTO messages (id, turn, role, content, tool_calls, tool_call_id, tokens_in, tokens_out, cost_usd, session_id, created_at) VALUES (?,?,?,?,?,?,?,?,?,?,?)`)
                .run(m.id, m.turn, m.role, m.content, m.tool_calls, m.tool_call_id, m.tokens_in, m.tokens_out, m.cost_usd, m.session_id, m.created_at);
            } catch { /* skip duplicate */ }
          }

          const hasMetrics = oldDb
            .prepare(`SELECT 1 FROM sqlite_master WHERE type='table' AND name='metrics' LIMIT 1`)
            .get();

          if (hasMetrics) {
            const metrics = oldDb.prepare(
              `SELECT turn, model, input_tokens, output_tokens, cached_input_tokens, cost_usd, tool_calls, elapsed_ms, session_id, created_at FROM metrics`,
            ).all() as { turn: number; model: string | null; input_tokens: number; output_tokens: number; cached_input_tokens: number; cost_usd: number; tool_calls: number; elapsed_ms: number | null; session_id: string | null; created_at: number | null }[];

            for (const m of metrics) {
              try {
                targetDb.db
                  .prepare(`INSERT OR IGNORE INTO metrics (turn, model, input_tokens, output_tokens, cached_input_tokens, cost_usd, tool_calls, elapsed_ms, session_id, created_at) VALUES (?,?,?,?,?,?,?,?,?,?)`)
                  .run(m.turn, m.model, m.input_tokens, m.output_tokens, m.cached_input_tokens, m.cost_usd, m.tool_calls, m.elapsed_ms, m.session_id, m.created_at);
              } catch { /* skip duplicate */ }
            }
          }
        }

        const hasSkills = oldDb
          .prepare(`SELECT 1 FROM sqlite_master WHERE type='table' AND name='skills' LIMIT 1`)
          .get();

        if (hasSkills) {
          const skills = oldDb.prepare(`SELECT name, description, instructions, source, created_at FROM skills`).all() as { name: string; description: string; instructions: string; source: string; created_at: number }[];
          for (const sk of skills) {
            try {
              targetDb.db
                .prepare(`INSERT OR IGNORE INTO skills (name, description, instructions, source, created_at) VALUES (?,?,?,?,?)`)
                .run(sk.name, sk.description, sk.instructions, sk.source, sk.created_at);
            } catch { /* skip duplicate */ }
          }
        }

        const bpName = (() => {
          try {
            const row = oldDb.prepare(`SELECT value FROM config WHERE key = 'blueprint_name'`).get() as { value: string } | undefined;
            return row?.value;
          } catch { return undefined; }
        })();

        const bpDesc = (() => {
          try {
            const row = oldDb.prepare(`SELECT value FROM config WHERE key = 'blueprint_description'`).get() as { value: string } | undefined;
            return row?.value;
          } catch { return undefined; }
        })();

        const sysInstructions = (() => {
          try {
            const row = oldDb.prepare(`SELECT value FROM config WHERE key = 'system_instructions'`).get() as { value: string } | undefined;
            return row?.value;
          } catch { return undefined; }
        })();

        if (agentName !== "default") {
          const mdPath = path.join(agentsDir, `${agentName}.md`);
          if (!existsSync(mdPath)) {
            const lines = ["---"];
            lines.push(`name: ${bpName ?? agentName}`);
            if (bpDesc) lines.push(`description: ${bpDesc}`);
            lines.push("---");
            lines.push("");
            if (sysInstructions) lines.push(sysInstructions);
            else lines.push(`Agent migrated from ${dbFile}.`);
            lines.push("");
            writeFileSync(mdPath, lines.join("\n"), "utf8");
          }
        }

        migratedAgents.push(agentName);
      } catch {
        /* skip unreadable DB */
      } finally {
        if (oldDb) try { oldDb.close(); } catch { /* best effort */ }
      }
    }

    targetDb.db.exec("COMMIT");

    const backupDir = path.join(mpDir, "agents.backup");
    if (!existsSync(backupDir)) {
      mkdirSync(backupDir, { recursive: true });
    }

    for (const dbFile of oldDbFiles) {
      try {
        renameSync(
          path.join(agentsDir, dbFile),
          path.join(backupDir, dbFile),
        );
      } catch { /* best effort */ }
      for (const suffix of ["-wal", "-shm"]) {
        const walFile = path.join(agentsDir, dbFile + suffix);
        if (existsSync(walFile)) {
          try {
            renameSync(walFile, path.join(backupDir, dbFile + suffix));
          } catch { /* best effort */ }
        }
      }
    }
  } catch (e) {
    if (targetDb) try { targetDb.db.exec("ROLLBACK"); } catch { /* may not be in txn */ }
    throw e;
  } finally {
    if (targetDb) try { closeAgentDB(targetDb); } catch { /* best effort */ }
  }

  return { migrated: true, agents: migratedAgents };
}