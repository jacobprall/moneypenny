import { existsSync } from "node:fs";
import { join } from "node:path";
import {
  getConversation,
  getSessionMetrics,
  getSkill,
  listPolicies,
  listSessions,
  listSkillCatalog,
  upsertSkill,
} from "@moneypenny/db";
import { getWorkspaceHandle } from "@moneypenny/db/workspace";
import type { AgentDB, IndexStatus, SessionMetrics } from "@moneypenny/db/types";
import { getIndexStatus } from "@moneypenny/search";

export interface MemorySearchHit {
  id: string;
  turn: number;
  role: string;
  snippet: string | null;
}

export interface SkillSearchHit {
  name: string;
  description: string;
  source: string;
}

export interface SessionListRow {
  id: string;
  label: string | null;
  created_at: number;
  last_active_at: number;
}

export interface PolicyRow {
  name: string;
  effect: string;
  tool_pattern: string | null;
  priority: number;
}

export interface MetricsBySessionRow {
  session_id: string | null;
  turns: number;
  total_cost_usd: number;
  total_input_tokens: number;
  total_output_tokens: number;
  total_tool_calls: number;
}

export interface ForgetMemoryResult {
  deletedMessages: number;
  deletedSkills: number;
}

export interface IndexStatusWithStale extends IndexStatus {
  staleChunks: number;
}

export interface PruneStaleChunksResult {
  deletedChunkRows: number;
  prunedPaths: string[];
}

function normalizeSearchPattern(q: string): string {
  return q.trim().toLowerCase();
}

export interface ContextCurateService {
  searchMemory(query: string, limit?: number): { messages: MemorySearchHit[]; skills: SkillSearchHit[] };
  forgetMemory(params: { id?: string; query?: string }): ForgetMemoryResult;
  reviewCosts(): { aggregate: SessionMetrics; bySession: MetricsBySessionRow[] };
  listSkillsForCuration(): SkillSearchHit[];
  updateSkillInstructions(name: string, instructions: string): void;
  listSessionsForCuration(limit?: number): SessionListRow[];
  summarizeSession(sessionId: string): string;
  indexStatus(repoPath: string): IndexStatusWithStale;
  inspectPolicies(): PolicyRow[];
  pruneStaleChunks(repoPath: string): PruneStaleChunksResult;
}

export function createContextCurateService(db: AgentDB): ContextCurateService {
  return {
    searchMemory(query: string, limit = 50) {
      const lim = Math.min(Math.max(1, limit), 200);
      const pat = normalizeSearchPattern(query);
      if (!pat) {
        return { messages: [] as MemorySearchHit[], skills: [] as SkillSearchHit[] };
      }

      const messages = db.reads.read((raw) => {
        const rows = raw
          .prepare(
            `SELECT id, turn, role, content
             FROM messages
             WHERE instr(lower(coalesce(content, '')), ?) > 0
             ORDER BY created_at DESC
             LIMIT ?`,
          )
          .all(pat, lim) as { id: string; turn: number; role: string; content: string | null }[];
        return rows.map((r) => ({
          id: r.id,
          turn: r.turn,
          role: r.role,
          snippet: r.content != null ? r.content.slice(0, 500) : null,
        }));
      });

      const skills = db.reads.read((raw) => {
        const rows = raw
          .prepare(
            `SELECT name, description, source
             FROM skills
             WHERE instr(lower(name), ?) > 0
                OR instr(lower(description), ?) > 0
                OR instr(lower(instructions), ?) > 0
             ORDER BY name
             LIMIT ?`,
          )
          .all(pat, pat, pat, lim) as SkillSearchHit[];
        return rows;
      });

      return { messages, skills };
    },

    forgetMemory(params: { id?: string; query?: string }): ForgetMemoryResult {
      let deletedMessages = 0;
      let deletedSkills = 0;

      const id = params.id?.trim();
      const query = params.query?.trim();

      db.writer.exclusive((raw) => {
        if (id) {
          deletedMessages += raw.prepare(`DELETE FROM messages WHERE id = ?`).run(id).changes;
          deletedSkills += raw.prepare(`DELETE FROM skills WHERE name = ?`).run(id).changes;
        }
        if (query) {
          const pat = normalizeSearchPattern(query);
          if (pat) {
            const msgIds = raw
              .prepare(
                `SELECT id FROM messages WHERE instr(lower(coalesce(content, '')), ?) > 0 LIMIT 500`,
              )
              .all(pat) as { id: string }[];
            const delMsg = raw.prepare(`DELETE FROM messages WHERE id = ?`);
            for (const row of msgIds) {
              deletedMessages += delMsg.run(row.id).changes;
            }

            const skillNames = raw
              .prepare(
                `SELECT name FROM skills
                 WHERE instr(lower(name), ?) > 0
                    OR instr(lower(description), ?) > 0
                    OR instr(lower(instructions), ?) > 0
                 LIMIT 100`,
              )
              .all(pat, pat, pat) as { name: string }[];
            const delSkill = raw.prepare(`DELETE FROM skills WHERE name = ?`);
            for (const row of skillNames) {
              deletedSkills += delSkill.run(row.name).changes;
            }
          }
        }
      });

      return { deletedMessages, deletedSkills };
    },

    reviewCosts(): { aggregate: SessionMetrics; bySession: MetricsBySessionRow[] } {
      const aggregate = getSessionMetrics(db);
      const bySession = db.reads.read((raw) => {
        const rows = raw
          .prepare(
            `SELECT
               session_id,
               COUNT(*) AS turns,
               COALESCE(SUM(cost_usd), 0) AS total_cost_usd,
               COALESCE(SUM(input_tokens), 0) AS total_input_tokens,
               COALESCE(SUM(output_tokens), 0) AS total_output_tokens,
               COALESCE(SUM(tool_calls), 0) AS total_tool_calls
             FROM metrics
             GROUP BY session_id
             ORDER BY total_cost_usd DESC
             LIMIT 200`,
          )
          .all() as {
          session_id: string | null;
          turns: number;
          total_cost_usd: number;
          total_input_tokens: number;
          total_output_tokens: number;
          total_tool_calls: number;
        }[];
        return rows.map((r) => ({
          session_id: r.session_id,
          turns: Number(r.turns),
          total_cost_usd: Number(r.total_cost_usd),
          total_input_tokens: Number(r.total_input_tokens),
          total_output_tokens: Number(r.total_output_tokens),
          total_tool_calls: Number(r.total_tool_calls),
        }));
      });
      return { aggregate, bySession };
    },

    listSkillsForCuration(): SkillSearchHit[] {
      return listSkillCatalog(db);
    },

    updateSkillInstructions(name: string, instructions: string): void {
      const skill = getSkill(db, name);
      if (!skill) {
        throw new Error(`Skill not found: ${name}`);
      }
      upsertSkill(db, { ...skill, instructions });
    },

    listSessionsForCuration(limit?: number): SessionListRow[] {
      const rows = listSessions(db);
      const lim = limit != null ? Math.min(Math.max(1, limit), 500) : rows.length;
      return rows.slice(0, lim).map((s) => ({
        id: s.id,
        label: s.label,
        created_at: s.createdAt,
        last_active_at: s.lastActiveAt,
      }));
    },

    summarizeSession(sessionId: string): string {
      const messages = getConversation(db, { sessionId });
      if (messages.length === 0) {
        return `No messages found for session ${sessionId}.`;
      }
      const lines: string[] = [];
      lines.push(`Session ${sessionId}: ${messages.length} message(s).`);
      const byRole: Record<string, number> = {};
      for (const m of messages) {
        byRole[m.role] = (byRole[m.role] ?? 0) + 1;
      }
      lines.push(`Roles: ${JSON.stringify(byRole)}`);
      const maxChars = 12_000;
      let used = 0;
      lines.push("");
      lines.push("Transcript excerpt:");
      for (const m of messages) {
        const prefix = `[turn ${m.turn} ${m.role}]`;
        const body = (m.content ?? "").trim();
        const piece = body ? `${prefix} ${body}` : prefix;
        if (used + piece.length + 1 > maxChars) {
          lines.push(`… truncated after ~${maxChars} characters`);
          break;
        }
        lines.push(piece);
        used += piece.length + 1;
      }
      return lines.join("\n");
    },

    indexStatus(repoPath: string): IndexStatusWithStale {
      const status = getIndexStatus(db);
      const ws = getWorkspaceHandle(db);
      const paths = ws.prepare(`SELECT DISTINCT path FROM code_chunks`).all() as { path: string }[];
      let staleChunks = 0;
      const countStmt = ws.prepare(`SELECT COUNT(*) AS c FROM code_chunks WHERE path = ?`);
      for (const { path: rel } of paths) {
        const full = join(repoPath, rel);
        if (!existsSync(full)) {
          const row = countStmt.get(rel) as { c: number };
          staleChunks += Number(row.c);
        }
      }
      return { ...status, staleChunks };
    },

    inspectPolicies(): PolicyRow[] {
      return listPolicies(db)
        .filter((p) => p.enabled !== 0)
        .map((p) => ({
          name: p.name,
          effect: p.effect,
          tool_pattern: p.toolPattern,
          priority: p.priority,
        }));
    },

    pruneStaleChunks(repoPath: string): PruneStaleChunksResult {
      const ws = getWorkspaceHandle(db);
      const paths = ws.prepare(`SELECT DISTINCT path FROM code_chunks`).all() as { path: string }[];
      const prunedPaths: string[] = [];
      let deletedChunkRows = 0;

      for (const { path: rel } of paths) {
        const full = join(repoPath, rel);
        if (!existsSync(full)) {
          const info = ws.prepare(`DELETE FROM code_chunks WHERE path = ?`).run(rel);
          deletedChunkRows += info.changes;
          prunedPaths.push(rel);
        }
      }

      return { deletedChunkRows, prunedPaths };
    },
  };
}
