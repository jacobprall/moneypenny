import {
  listConventions as dbListConventions,
  listPointers as dbListPointers,
  listSkills as dbListSkills,
  insertPointer,
} from "@moneypenny/db";
import type { ActionContext } from "./context.js";

export function listSkills(ctx: ActionContext) {
  return dbListSkills(ctx.readDb);
}

export function listConventions(ctx: ActionContext) {
  return dbListConventions(ctx.readDb);
}

export function listPointers(
  ctx: ActionContext,
  q?: { sessionId?: string; pinnedOnly?: boolean },
) {
  return dbListPointers(ctx.readDb, q);
}

export function recordPointer(
  ctx: ActionContext,
  input: { sessionId: string; key: string; phrase: string; pinned?: boolean },
) {
  return insertPointer(ctx.writeDb, {
    sessionId: input.sessionId,
    key: input.key,
    phrase: input.phrase,
    pinned: input.pinned ? 1 : 0,
  });
}
