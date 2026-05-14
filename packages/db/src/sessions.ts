import { sqlError } from "./errors";
import type { AgentDB, Session, SessionSummary } from "./types";
import { generateUUIDv7 } from "./uuid";

function hasSessionsTable(db: AgentDB): boolean {
  try {
    const row = db.db
      .prepare(`SELECT 1 AS ok FROM sqlite_master WHERE type = 'table' AND name = 'sessions' LIMIT 1`)
      .get() as { ok: number } | undefined;
    return !!row;
  } catch {
    return false;
  }
}

export function createSession(db: AgentDB, label?: string, agentName?: string): Session {
  const id = generateUUIDv7();
  const now = Date.now();
  const agent = agentName ?? "default";
  try {
    db.db
      .prepare(`INSERT INTO sessions (id, label, agent_name, created_at, last_active_at, is_active) VALUES (?,?,?,?,?,1)`)
      .run(id, label ?? null, agent, now, now);
  } catch (e) {
    throw sqlError("createSession", e);
  }
  return { id, label: label ?? null, createdAt: now, lastActiveAt: now, isActive: true };
}

export function listSessions(db: AgentDB, opts?: { agentName?: string }): SessionSummary[] {
  if (!hasSessionsTable(db)) return [];

  const hasAgentName = (() => {
    try {
      db.db.prepare(`SELECT agent_name FROM sessions LIMIT 0`).get();
      return true;
    } catch { return false; }
  })();

  const agentFilter = opts?.agentName && hasAgentName;
  const whereClause = agentFilter ? `WHERE s.agent_name = ?` : "";

  try {
    const stmt = db.db.prepare(
      `SELECT
         s.id,
         s.label,
         ${hasAgentName ? "s.agent_name," : "'default' AS agent_name,"}
         s.created_at,
         s.last_active_at,
         COALESCE(m.turns, 0) AS turns,
         COALESCE(mt.cost_usd, 0) AS cost_usd
       FROM sessions s
       LEFT JOIN (SELECT session_id, COUNT(DISTINCT turn) AS turns FROM messages GROUP BY session_id) m
         ON m.session_id = s.id
       LEFT JOIN (SELECT session_id, SUM(cost_usd) AS cost_usd FROM metrics GROUP BY session_id) mt
         ON mt.session_id = s.id
       ${whereClause}
       ORDER BY s.last_active_at DESC`,
    );

    const rows = (agentFilter ? stmt.all(opts!.agentName!) : stmt.all()) as {
      id: string;
      label: string | null;
      agent_name: string;
      created_at: number;
      last_active_at: number;
      turns: number;
      cost_usd: number;
    }[];

    return rows.map((r) => ({
      id: r.id,
      label: r.label,
      agentName: r.agent_name ?? "default",
      turns: Number(r.turns),
      costUsd: Number(r.cost_usd),
      lastActiveAt: r.last_active_at,
      createdAt: r.created_at,
    }));
  } catch (e) {
    throw sqlError("listSessions", e);
  }
}

export function getActiveSession(db: AgentDB): Session | null {
  if (!hasSessionsTable(db)) return null;
  try {
    const row = db.db
      .prepare(
        `SELECT id, label, created_at, last_active_at, is_active
         FROM sessions WHERE is_active = 1 ORDER BY last_active_at DESC LIMIT 1`,
      )
      .get() as {
      id: string;
      label: string | null;
      created_at: number;
      last_active_at: number;
      is_active: number;
    } | undefined;
    if (!row) return null;
    return {
      id: row.id,
      label: row.label,
      createdAt: row.created_at,
      lastActiveAt: row.last_active_at,
      isActive: row.is_active === 1,
    };
  } catch (e) {
    throw sqlError("getActiveSession", e);
  }
}

export function labelSession(db: AgentDB, sessionId: string, label: string): void {
  try {
    db.db
      .prepare(`UPDATE sessions SET label = ? WHERE id = ? AND label IS NULL`)
      .run(label, sessionId);
  } catch (e) {
    throw sqlError("labelSession", e);
  }
}

export function setActiveSession(db: AgentDB, sessionId: string): void {
  db.activeSessionId = sessionId;
  try {
    const now = Date.now();
    db.db.exec("BEGIN IMMEDIATE");
    try {
      db.db.prepare(`UPDATE sessions SET is_active = 0 WHERE is_active = 1 AND id != ?`).run(sessionId);
      db.db.prepare(`UPDATE sessions SET is_active = 1, last_active_at = ? WHERE id = ?`).run(now, sessionId);
      db.db.exec("COMMIT");
    } catch (inner) {
      try { db.db.exec("ROLLBACK"); } catch { /* best effort */ }
      throw inner;
    }
  } catch (e) {
    throw sqlError("setActiveSession", e);
  }
}
