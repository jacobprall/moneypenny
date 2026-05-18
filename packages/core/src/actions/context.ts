import type { Database } from "bun:sqlite";
import type {
  BlueprintRegistry,
  EngineSessionRunner,
  EventBus,
  ToolRegistry,
} from "@moneypenny/engine";

export interface CustodianHandle {
  queueExtract(sessionId: string): void;
}

export interface ActionContext {
  writeDb: Database;
  readDb: Database;
  events: EventBus;
  runner: EngineSessionRunner;
  registry: BlueprintRegistry;
  ideasDirs: { global: string; repo?: string };
  tools: ToolRegistry;
  custodian?: CustodianHandle;
}

export function createActionContext(
  input: ActionContext,
): ActionContext {
  return input;
}

export type BlueprintDirs = { global: string; repo?: string };
