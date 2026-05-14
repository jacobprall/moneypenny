import type { Database } from "bun:sqlite";

export interface SessionSummaryData {
  id: string;
  memberId?: string;
  title?: string;
  durationMs?: number;
  costUsd?: number;
  filesModified?: number;
  model?: string;
}

export function writeSummary(teamDb: Database, data: SessionSummaryData): void {
  teamDb.run(
    `INSERT OR REPLACE INTO session_summaries (id, member_id, title, duration_ms, cost_usd, files_modified, model, created_at)
     VALUES (?, ?, ?, ?, ?, ?, ?, ?)`,
    [
      data.id,
      data.memberId ?? null,
      data.title ?? null,
      data.durationMs ?? null,
      data.costUsd ?? null,
      data.filesModified ?? null,
      data.model ?? null,
      Date.now(),
    ],
  );
}
