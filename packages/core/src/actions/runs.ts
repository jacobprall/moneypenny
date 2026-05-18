import { getRun, listRuns } from "@moneypenny/db";
import type { Message } from "@moneypenny/db";
import { ErrorCodes, MoneypennyError } from "../errors.js";
import type { ActionContext } from "./context.js";

export function listRunsBySession(ctx: ActionContext, sessionId: string) {
  return listRuns(ctx.readDb, sessionId);
}

export function getRunDetail(ctx: ActionContext, id: string) {
  const run = getRun(ctx.readDb, id);
  if (!run) throw new MoneypennyError(ErrorCodes.RUN_NOT_FOUND, id);
  const messages = ctx.readDb
    .query<Message, [string]>(
      `SELECT * FROM messages WHERE run_id = ? ORDER BY seq ASC`,
    )
    .all(id);
  return { run, messages };
}
