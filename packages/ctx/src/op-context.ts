import type { AgentDB } from "@moneypenny/db";

export interface OperationContext {
  db: AgentDB;
  actor: string;
  sessionId?: string;
}
