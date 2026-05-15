import {
  getConversation,
  getSessionMetrics,
  listPolicies,
  listSessions,
  listSkills,
  type AgentDB,
  type IndexStatus,
  type Policy,
  type SessionSummary,
  type Skill,
} from "@moneypenny/db";
import { getIndexStatus } from "@moneypenny/search";

export type SessionRow = SessionSummary;

export interface BlueprintDetail {
  id: string;
  dirPath: string;
  agentMdPath: string;
  checksum: string;
  name: string;
  description: string | null;
  schedule: string | null;
  timezone: string | null;
  enabled: number;
  status: string;
  validationErrors: string | null;
  configJson: string;
  prompt: string;
  jobId: string | null;
  lastLoadedAt: number;
  createdAt: number;
  updatedAt: number;
}

export interface JobRow {
  id: string;
  name: string;
  description: string | null;
  schedule: string;
  operation: string;
  payload: string | null;
  nextRunAt: number | null;
  lastRunAt: number | null;
  overlapPolicy: string;
  maxRetries: number;
  timeoutMs: number;
  status: string;
  enabled: number;
  createdAt: number;
  updatedAt: number;
}

export interface JobRunRow {
  id: string;
  jobId: string;
  startedAt: number;
  endedAt: number | null;
  status: string;
  result: string | null;
  error: string | null;
  retryCount: number;
  createdAt: number;
}

export interface MemoryRow {
  id: string;
  sessionId: string | null;
  turn: number;
  role: string;
  content: string;
  createdAt: number;
}

export type SkillRow = Skill;

export type IndexHealthStats = IndexStatus;

export interface GovEventRow {
  id: string;
  operation: string;
  actor: string;
  sessionId: string | null;
  input: string;
  output: string | null;
  error: string | null;
  durationMs: number | null;
  createdAt: number;
}

export type PolicyRow = Policy;

export interface CostSummary {
  totalCostUsd: number;
  totalTurns: number;
  totalInputTokens: number;
  totalOutputTokens: number;
  totalToolCalls: number;
}

const AGENT_COLS = `id, dir_path as dirPath, agent_md_path as agentMdPath, checksum,
  name, description, schedule, timezone, enabled, status,
  validation_errors as validationErrors, config_json as configJson, prompt,
  job_id as jobId, last_loaded_at as lastLoadedAt,
  created_at as createdAt, updated_at as updatedAt`;

function listAgentRows(db: AgentDB["db"]): BlueprintDetail[] {
  try {
    return db.prepare(`SELECT ${AGENT_COLS} FROM agents ORDER BY id ASC`).all() as BlueprintDetail[];
  } catch {
    return [];
  }
}

export type BlueprintRow = Pick<BlueprintDetail, "id" | "name" | "description" | "status" | "enabled" | "schedule">;

const JOB_SELECT = `SELECT id, name, description, schedule, operation, payload,
  next_run_at AS nextRunAt, last_run_at AS lastRunAt,
  overlap_policy AS overlapPolicy, max_retries AS maxRetries, timeout_ms AS timeoutMs,
  status, enabled, created_at AS createdAt, updated_at AS updatedAt
  FROM jobs`;

export class DataStore {
  constructor(private readonly db: AgentDB) {}

  listSessions(opts?: { limit?: number; offset?: number; search?: string }): SessionRow[] {
    let rows = listSessions(this.db);
    const q = opts?.search?.trim().toLowerCase();
    if (q) {
      rows = rows.filter((r) => r.id.toLowerCase().includes(q) || (r.label?.toLowerCase().includes(q) ?? false));
    }
    const offset = Math.max(0, opts?.offset ?? 0);
    const limit = opts?.limit;
    if (limit != null) {
      return rows.slice(offset, offset + Math.max(0, limit));
    }
    return rows.slice(offset);
  }

  getSession(id: string): SessionRow | null {
    return listSessions(this.db).find((s) => s.id === id) ?? null;
  }

  deleteSession(id: string): void {
    this.db.writer.exclusive((raw) => {
      raw.exec("BEGIN IMMEDIATE");
      try {
        raw.prepare(`DELETE FROM messages WHERE session_id = ?`).run(id);
        raw.prepare(`DELETE FROM events WHERE session_id = ?`).run(id);
        raw.prepare(`DELETE FROM metrics WHERE session_id = ?`).run(id);
        raw.prepare(`DELETE FROM compaction_markers WHERE session_id = ?`).run(id);
        raw.prepare(`DELETE FROM gov_events WHERE session_id = ?`).run(id);
        raw.prepare(`DELETE FROM sessions WHERE id = ?`).run(id);
        raw.exec("COMMIT");
      } catch (e) {
        try {
          raw.exec("ROLLBACK");
        } catch {
          /* best effort */
        }
        throw e;
      }
    });
    if (this.db.activeSessionId === id) {
      this.db.activeSessionId = undefined;
    }
  }

  exportSession(id: string, format: "markdown" | "json"): string {
    const messages = getConversation(this.db, { sessionId: id });
    if (format === "json") {
      return JSON.stringify(
        messages.map((m) => ({
          id: m.id,
          turn: m.turn,
          role: m.role,
          content: m.content,
          toolCalls: m.toolCalls,
          toolCallId: m.toolCallId,
          createdAt: m.createdAt,
        })),
        null,
        2,
      );
    }
    const lines: string[] = [`# Session ${id}`, ""];
    for (const m of messages) {
      lines.push(`## ${m.role} (turn ${m.turn})`, "");
      if (m.content) lines.push(m.content, "");
      if (m.toolCalls) lines.push("```json", m.toolCalls, "```", "");
    }
    return lines.join("\n");
  }

  listBlueprints(): BlueprintRow[] {
    return listAgentRows(this.db.db).map((a) => ({
      id: a.id,
      name: a.name,
      description: a.description,
      status: a.status,
      enabled: a.enabled,
      schedule: a.schedule,
    }));
  }

  getBlueprint(name: string): BlueprintDetail | null {
    const rows = listAgentRows(this.db.db);
    return rows.find((a) => a.name === name) ?? null;
  }

  listJobs(opts?: { type?: string }): JobRow[] {
    try {
      if (opts?.type) {
        return this.db.db.prepare(`${JOB_SELECT} WHERE operation = ? ORDER BY created_at DESC`).all(opts.type) as JobRow[];
      }
      return this.db.db.prepare(`${JOB_SELECT} ORDER BY created_at DESC`).all() as JobRow[];
    } catch {
      return [];
    }
  }

  listJobRuns(jobId: string, limit?: number): JobRunRow[] {
    try {
      const lim = Math.max(1, limit ?? 50);
      return this.db.db
        .prepare(
          `SELECT id, job_id AS jobId, started_at AS startedAt, ended_at AS endedAt,
                  status, result, error, retry_count AS retryCount, created_at AS createdAt
           FROM job_runs WHERE job_id = ? ORDER BY created_at DESC LIMIT ?`,
        )
        .all(jobId, lim) as JobRunRow[];
    } catch {
      return [];
    }
  }

  listMemories(opts?: { limit?: number; search?: string }): MemoryRow[] {
    try {
      const lim = Math.max(1, opts?.limit ?? 50);
      const term = opts?.search?.trim();
      const memoryClause = `(
        LOWER(COALESCE(content, '')) LIKE '%memory%'
        OR LOWER(COALESCE(content, '')) LIKE '%remember%'
        OR LOWER(COALESCE(content, '')) LIKE '%recall%'
        OR LOWER(COALESCE(content, '')) LIKE '%memories%'
      )`;
      if (term) {
        const rows = this.db.db
          .prepare(
            `SELECT id, session_id AS sessionId, turn, role, content, created_at AS createdAt
             FROM messages
             WHERE session_id IS NOT NULL AND content IS NOT NULL
               AND ${memoryClause}
               AND LOWER(content) LIKE ?
             ORDER BY created_at DESC
             LIMIT ?`,
          )
          .all(`%${term.toLowerCase()}%`, lim) as MemoryRow[];
        return rows;
      }
      return this.db.db
        .prepare(
          `SELECT id, session_id AS sessionId, turn, role, content, created_at AS createdAt
           FROM messages
           WHERE session_id IS NOT NULL AND content IS NOT NULL
             AND ${memoryClause}
           ORDER BY created_at DESC
           LIMIT ?`,
        )
        .all(lim) as MemoryRow[];
    } catch {
      return [];
    }
  }

  listSkills(): SkillRow[] {
    return listSkills(this.db);
  }

  indexHealth(): IndexHealthStats {
    return getIndexStatus(this.db);
  }

  listPolicyEvents(sessionId: string): GovEventRow[] {
    try {
      const rows = this.db.db
        .prepare(
          `SELECT id, operation, actor, session_id AS sessionId, input, output, error, duration_ms AS durationMs, created_at AS createdAt
           FROM gov_events WHERE session_id = ? ORDER BY created_at DESC LIMIT ?`,
        )
        .all(sessionId, 500) as GovEventRow[];
      return rows.slice().reverse();
    } catch {
      return [];
    }
  }

  listActivePolicies(): PolicyRow[] {
    return listPolicies(this.db).filter((p) => p.enabled !== 0);
  }

  costSummary(): CostSummary {
    const m = getSessionMetrics(this.db);
    return {
      totalCostUsd: m.totalCostUsd,
      totalTurns: m.totalTurns,
      totalInputTokens: m.totalInputTokens,
      totalOutputTokens: m.totalOutputTokens,
      totalToolCalls: m.totalToolCalls,
    };
  }
}
