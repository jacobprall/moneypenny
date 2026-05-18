import type { ActionContext } from "./context.js";

export function getHealth(ctx: ActionContext) {
  const healthRow = ctx.readDb
    .query<{ health: string }, []>(`SELECT health FROM v_health`)
    .get();
  const costRow = ctx.readDb
    .query<
      {
        total: number;
        sessions: number;
        tokens_in: number;
        tokens_out: number;
      },
      []
    >(`SELECT * FROM v_cost_today`)
    .get();
  let health: Record<string, unknown> = {};
  try {
    health = JSON.parse(healthRow?.health ?? "{}") as Record<string, unknown>;
  } catch {
    health = {};
  }
  return {
    ...health,
    pool: { activeSessions: ctx.runner.activeSessionIds() },
    cost_today: costRow ?? {},
  };
}

export function getSystemConfig(ctx: ActionContext): Record<string, string> {
  const rows = ctx.readDb
    .query<{ key: string; value: string }, []>(`SELECT key, value FROM config`)
    .all();
  return Object.fromEntries(rows.map((r) => [r.key, r.value]));
}

export function setSystemConfig(
  ctx: ActionContext,
  kv: Record<string, string>,
): void {
  const stmt = ctx.writeDb.query<
    unknown,
    [string, string]
  >(`INSERT INTO config (key, value) VALUES (?, ?)
     ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = unixepoch()`);
  for (const [k, v] of Object.entries(kv)) stmt.run(k, v);
}
