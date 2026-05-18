import { z } from "zod";
import type { ToolDef } from "../types.js";

function ftsSafe(q: string): string {
  return q.replace(/[^\w\s]/g, " ").replace(/\s+/g, " ").trim();
}

export const searchMessagesTool: ToolDef<
  { query: string; limit?: number },
  { rows: Array<{ session_id: string; snippet: string }> }
> = {
  name: "search_messages",
  description: "FTS over stored messages.",
  category: "session",
  permissions: {},
  inputSchema: z.object({
    query: z.string(),
    limit: z.number().optional().default(15),
  }),
  execute: async ({ query, limit }, ctx) => {
    const ft = ftsSafe(query);
    if (!ft) return { rows: [] };
    const rows = ctx.readDb
      .query<{ session_id: string; content: string | null }, [string, number]>(
        `SELECT m.session_id, m.content FROM messages_fts fts
         JOIN messages m ON m.rowid = fts.rowid
         WHERE messages_fts MATCH ? LIMIT ?`,
      )
      .all(ft, limit);
    return {
      rows: rows.map((r) => ({
        session_id: r.session_id,
        snippet: (r.content ?? "").slice(0, 400),
      })),
    };
  },
};
