import { z } from "zod";
import type { ToolDef } from "../types.js";

export const findSymbolTool: ToolDef<
  { pattern: string; limit?: number },
  { rows: Array<Record<string, unknown>> }
> = {
  name: "find_symbol",
  description: "Find code chunks by symbol name (SQL LIKE).",
  category: "code",
  permissions: { filesystem: "read" },
  inputSchema: z.object({
    pattern: z.string(),
    limit: z.number().optional().default(20),
  }),
  execute: async ({ pattern, limit }, ctx) => {
    const like = `%${pattern.replace(/%/g, "\\%")}%`;
    const rows = ctx.readDb
      .query<
        {
          id: string;
          file_path: string;
          symbol_name: string | null;
          start_line: number | null;
          content: string;
        },
        [string, number]
      >(
        `SELECT id, file_path, symbol_name, start_line, content FROM code_chunks
         WHERE symbol_name LIKE ? ESCAPE '\\' LIMIT ?`,
      )
      .all(like, limit);
    return { rows: rows as Array<Record<string, unknown>> };
  },
};
