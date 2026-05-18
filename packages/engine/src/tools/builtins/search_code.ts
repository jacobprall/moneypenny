import { z } from "zod";
import { hybridSearch } from "../../embeddings.js";
import type { ToolDef } from "../types.js";

function ftsSafe(q: string): string {
  return q.replace(/[^\w\s]/g, " ").replace(/\s+/g, " ").trim();
}

export const searchCodeTool: ToolDef<
  { query: string; limit?: number },
  { results: Array<Record<string, unknown>> }
> = {
  name: "search_code",
  description: "Hybrid search over indexed code (semantic when embeddings exist).",
  category: "code",
  permissions: { filesystem: "read" },
  inputSchema: z.object({
    query: z.string(),
    limit: z.number().optional().default(10),
  }),
  execute: async ({ query, limit }, ctx) => {
    try {
      const rows = await hybridSearch(ctx.readDb, query, limit);
      return {
        results: rows.map((r) => ({
          path: r.file_path,
          symbol: r.symbol_name,
          line: r.start_line,
          content: r.content.slice(0, 2000),
        })),
      };
    } catch {
      const ft = ftsSafe(query);
      if (!ft) return { results: [] };
      const rows = ctx.readDb
        .query<
          { file_path: string; symbol_name: string | null; content: string; start_line: number | null },
          [string, number]
        >(
          `SELECT c.file_path, c.symbol_name, c.content, c.start_line
           FROM code_chunks_fts fts JOIN code_chunks c ON c.rowid = fts.rowid
           WHERE code_chunks_fts MATCH ? ORDER BY rank LIMIT ?`,
        )
        .all(ft, limit);
      return {
        results: rows.map((r) => ({
          path: r.file_path,
          symbol: r.symbol_name,
          line: r.start_line,
          content: r.content.slice(0, 2000),
        })),
      };
    }
  },
};
