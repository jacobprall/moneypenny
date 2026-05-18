import {
  createTab,
  deleteTab,
  listTabs as dbListTabs,
  setActiveTab,
  updateTab,
} from "@moneypenny/db";
import { ErrorCodes, MoneypennyError } from "../errors.js";
import type { ActionContext } from "./context.js";

export function listTabs(ctx: ActionContext) {
  return dbListTabs(ctx.readDb);
}

export function openTab(
  ctx: ActionContext,
  input: {
    kind: string;
    sessionId?: string | null;
    label?: string | null;
    position?: number;
    active?: boolean;
  },
) {
  return createTab(ctx.writeDb, input);
}

export function patchTab(
  ctx: ActionContext,
  input: {
    id: string;
    position?: number;
    label?: string | null;
    active?: boolean;
  },
) {
  const tabs = dbListTabs(ctx.readDb);
  if (!tabs.find((t) => t.id === input.id))
    throw new MoneypennyError(ErrorCodes.TAB_NOT_FOUND, input.id);
  if (input.active) setActiveTab(ctx.writeDb, input.id);
  if (input.position != null || input.label !== undefined) {
    updateTab(ctx.writeDb, input.id, {
      position: input.position,
      label: input.label,
    });
  }
}

export function closeTab(ctx: ActionContext, id: string): void {
  const tabs = dbListTabs(ctx.readDb);
  if (!tabs.find((t) => t.id === id))
    throw new MoneypennyError(ErrorCodes.TAB_NOT_FOUND, id);
  deleteTab(ctx.writeDb, id);
}
