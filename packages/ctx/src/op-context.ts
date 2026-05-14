import type { Database } from "bun:sqlite";

export interface OperationContext {
  db: Database;
  actor: string;
  sessionId?: string;
}
