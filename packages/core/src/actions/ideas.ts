import {
  listIdeas as readerList,
  getIdea as readerGet,
  writeIdea as readerWrite,
  deleteIdea as readerDelete,
} from "@moneypenny/engine";
import { ErrorCodes, MoneypennyError } from "../errors.js";
import type { ActionContext } from "./context.js";

export function listIdeas(
  ctx: ActionContext,
  _q?: { status?: string; tags?: string; cwd?: string },
) {
  let rows = readerList(ctx.ideasDirs.global, ctx.ideasDirs.repo);
  if (_q?.status) rows = rows.filter((i) => i.status === _q.status);
  if (_q?.tags) rows = rows.filter((i) => i.tags?.includes(_q.tags!));
  return rows;
}

export function getIdea(ctx: ActionContext, filename: string) {
  const i = readerGet(ctx.ideasDirs.global, ctx.ideasDirs.repo, filename);
  if (!i) throw new MoneypennyError(ErrorCodes.IDEA_NOT_FOUND, filename);
  return i;
}

export function createIdea(
  ctx: ActionContext,
  input: { filename: string; body: string; frontmatter: Record<string, unknown> },
) {
  return readerWrite(ctx.ideasDirs.global, input.filename, input.body, input.frontmatter);
}

export function updateIdea(
  ctx: ActionContext,
  filename: string,
  input: { body?: string; frontmatter: Record<string, unknown> },
) {
  const cur = getIdea(ctx, filename);
  return readerWrite(
    ctx.ideasDirs.global,
    filename,
    input.body ?? cur.body,
    input.frontmatter,
  );
}

export function deleteIdea(ctx: ActionContext, filename: string) {
  readerDelete(ctx.ideasDirs.global, filename);
}
