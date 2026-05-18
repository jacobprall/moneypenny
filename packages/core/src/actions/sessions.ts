import type { Blueprint } from "@moneypenny/engine";
import {
  createSession as insertSession,
  deleteSession as repoDelete,
  getSession,
  listSessions as repoList,
  listRuns,
  updateSessionConfigOptimistic,
  updateSessionStatus,
} from "@moneypenny/db";
import type { StoredSessionConfig } from "@moneypenny/engine";
import { ErrorCodes, MoneypennyError } from "../errors.js";
import type { ActionContext } from "./context.js";

export function snapshotConfig(bp: Blueprint, cwd: string): StoredSessionConfig {
  return {
    cwd,
    blueprint: bp.name,
    model: bp.model,
    strategy: bp.strategy,
    permissions: { ...bp.permissions },
    tools: bp.tools,
    pause_after: [...bp.pause_after],
    max_turns: bp.max_turns,
    context: { ...bp.context, skills: [...bp.context.skills] },
    instructions: bp.body,
  };
}

export function getSessionRecord(ctx: ActionContext, id: string) {
  const s = getSession(ctx.readDb, id);
  if (!s) throw new MoneypennyError(ErrorCodes.SESSION_NOT_FOUND, id);
  return s;
}

export async function createSession(
  ctx: ActionContext,
  input: {
    blueprint?: string;
    cwd: string;
    label?: string | null;
    parentId?: string | null;
    ideaId?: string | null;
    task?: string;
  },
): Promise<import("@moneypenny/db").Session> {
  const bp = input.blueprint
    ? ctx.registry.resolve(input.blueprint, input.cwd)
    : ctx.registry.getDefault();
  if (input.blueprint && !bp)
    throw new MoneypennyError(ErrorCodes.BLUEPRINT_NOT_FOUND, input.blueprint);
  const use = bp ?? ctx.registry.getDefault();
  const cfg = snapshotConfig(use, input.cwd);
  const session = insertSession(ctx.writeDb, {
    label: input.label,
    parentId: input.parentId,
    ideaId: input.ideaId,
    config: JSON.stringify(cfg),
  });
  ctx.events.emit({
    type: "session.created",
    session_id: session.id,
    detail: { blueprint: use.name, cwd: input.cwd, parent_id: input.parentId },
  });
  if (input.task) await ctx.runner.launch(session.id, input.task);
  return session;
}

export function getSessionDetail(ctx: ActionContext, id: string) {
  const s = getSessionRecord(ctx, id);
  const recentRuns = listRuns(ctx.readDb, id).slice(0, 12);
  return { session: s, recentRuns };
}

export function listSessions(
  ctx: ActionContext,
  q: {
    status?: string;
    blueprint?: string;
    label?: string;
    cursor?: number | null;
    limit?: number;
  },
) {
  const lim = Math.min(q.limit ?? 50, 200);
  let rows = repoList(ctx.readDb);
  if (q.status) rows = rows.filter((s) => s.status === q.status);
  if (q.blueprint)
    rows = rows.filter((s) => {
      try {
        const c = JSON.parse(s.config) as { blueprint?: string };
        return c.blueprint === q.blueprint;
      } catch {
        return false;
      }
    });
  if (q.label) rows = rows.filter((s) => s.label?.includes(q.label!));
  rows.sort((a, b) => b.last_active_at - a.last_active_at);
  const start = q.cursor ?? 0;
  const slice = rows.slice(start, start + lim);
  return {
    items: slice,
    nextCursor: start + slice.length < rows.length ? start + lim : null,
    hasMore: start + lim < rows.length,
  };
}

export async function injectMessage(
  ctx: ActionContext,
  sessionId: string,
  content: string,
): Promise<void> {
  getSessionRecord(ctx, sessionId);
  await ctx.runner.inject(sessionId, content);
}

export function updateSessionConfig(
  ctx: ActionContext,
  sessionId: string,
  nextJson: string,
  expectedVersion: number,
): { newVersion: number } {
  getSessionRecord(ctx, sessionId);
  const r = updateSessionConfigOptimistic(
    ctx.writeDb,
    sessionId,
    nextJson,
    expectedVersion,
  );
  if (!r.ok)
    throw new MoneypennyError(
      ErrorCodes.CONFIG_VERSION_MISMATCH,
      "config version mismatch",
    );
  ctx.events.emit({
    type: "session.config_changed",
    session_id: sessionId,
    detail: { keys: ["config"] },
  });
  return { newVersion: r.newVersion! };
}

export async function pauseSession(ctx: ActionContext, id: string): Promise<void> {
  getSessionRecord(ctx, id);
  await ctx.runner.pause(id);
}

export async function resumeSession(ctx: ActionContext, id: string): Promise<void> {
  getSessionRecord(ctx, id);
  await ctx.runner.resume(id);
}

function fireSessionCloseTriggers(ctx: ActionContext, closedSessionId: string, reason: string): void {
  const closedSession = getSession(ctx.readDb, closedSessionId);
  let cwd = process.cwd();
  if (closedSession?.config) {
    try {
      const cfg = JSON.parse(closedSession.config) as { cwd?: string };
      if (cfg.cwd) cwd = cfg.cwd;
    } catch { /* use default cwd */ }
  }

  for (const bp of ctx.registry.list()) {
    if (bp.trigger_on !== "session_close") continue;
    const cfg = snapshotConfig(bp, cwd);
    const session = insertSession(ctx.writeDb, {
      label: `${bp.name} (session_close)`,
      config: JSON.stringify(cfg),
    });
    ctx.events.emit({
      type: "session.created",
      session_id: session.id,
      detail: { blueprint: bp.name, cwd, parent_id: null },
    });
    void ctx.runner.launch(session.id, `Follow-up: session ${closedSessionId} just ${reason}.`);
  }
}

export function completeSession(ctx: ActionContext, id: string): void {
  getSessionRecord(ctx, id);
  updateSessionStatus(ctx.writeDb, id, "completed");
  ctx.writeDb
    .query(`UPDATE sessions SET completed_at = unixepoch() WHERE id = ?`)
    .run(id);
  ctx.events.emit({
    type: "session.status_changed",
    session_id: id,
    detail: { status: "completed", reason: "user" },
  });

  fireSessionCloseTriggers(ctx, id, "completed");
}

export function archiveSession(ctx: ActionContext, id: string): void {
  getSessionRecord(ctx, id);
  updateSessionStatus(ctx.writeDb, id, "archived");
  ctx.writeDb
    .query(`UPDATE sessions SET archived_at = unixepoch() WHERE id = ?`)
    .run(id);
  ctx.events.emit({
    type: "session.status_changed",
    session_id: id,
    detail: { status: "archived", reason: "user" },
  });
  ctx.custodian?.queueExtract(id);
  fireSessionCloseTriggers(ctx, id, "archived");
}

export function deleteSession(ctx: ActionContext, id: string): void {
  getSessionRecord(ctx, id);
  ctx.events.emit({ type: "session.deleted", session_id: id, detail: {} });
  repoDelete(ctx.writeDb, id);
}
