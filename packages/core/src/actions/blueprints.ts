import { ErrorCodes, MoneypennyError } from "../errors.js";
import type { ActionContext } from "./context.js";
import type { BlueprintDirs } from "./context.js";

export function listBlueprints(ctx: ActionContext) {
  return ctx.registry.list();
}

export function getBlueprint(ctx: ActionContext, name: string) {
  const b = ctx.registry.resolve(name);
  if (!b) throw new MoneypennyError(ErrorCodes.BLUEPRINT_NOT_FOUND, name);
  return b;
}

export function reloadBlueprints(
  ctx: ActionContext,
  dirs: BlueprintDirs,
): void {
  ctx.registry.start(dirs.global, dirs.repo);
}
