import { createSession as insertSessionRecord } from "@moneypenny/db";
import { ErrorCodes, MoneypennyError } from "../errors.js";
import { snapshotConfig } from "./sessions.js";
import type { ActionContext } from "./context.js";

export async function launchAgent(
  ctx: ActionContext,
  input: {
    blueprint?: string;
    task?: string;
    cwd?: string;
    label?: string;
    idea_id?: string;
  },
) {
  const cwd = input.cwd ?? process.cwd();
  const bpName = input.blueprint ?? "default";
  const bp = ctx.registry.resolve(bpName, cwd) ?? ctx.registry.getDefault();
  if (!bp) throw new MoneypennyError(ErrorCodes.BLUEPRINT_NOT_FOUND, bpName);
  const cfg = snapshotConfig(bp, cwd);
  const session = insertSessionRecord(ctx.writeDb, {
    label: input.label ?? null,
    ideaId: input.idea_id ?? null,
    config: JSON.stringify(cfg),
  });
  ctx.events.emit({
    type: "session.created",
    session_id: session.id,
    detail: { blueprint: bp.name, cwd, parent_id: null, idea_id: input.idea_id },
  });
  if (input.task) await ctx.runner.launch(session.id, input.task);
  return session;
}

export function getAgentStatus(ctx: ActionContext) {
  const ids = ctx.runner.activeSessionIds();
  return { activeSessions: ids, count: ids.length };
}

export async function killAgent(ctx: ActionContext, sessionId: string): Promise<void> {
  await ctx.runner.kill(sessionId);
}
