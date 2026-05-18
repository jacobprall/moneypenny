import { z } from "zod";
import type { ToolDef } from "../types.js";

export const readSymbolTool: ToolDef<
  { symbol?: string; path?: string; line?: number },
  { content: string; id?: string }
> = {
  name: "read_symbol",
  description: "Load one code chunk by symbol name or file path + line.",
  category: "code",
  permissions: { filesystem: "read" },
  inputSchema: z.object({
    symbol: z.string().optional(),
    path: z.string().optional(),
    line: z.number().optional(),
  }),
  execute: async ({ symbol, path: filePath, line }, ctx) => {
    if (symbol) {
      const row = ctx.readDb
        .query<{ id: string; content: string }, [string]>(
          `SELECT id, content FROM code_chunks WHERE symbol_name = ? LIMIT 1`,
        )
        .get(symbol);
      return row ?? { content: "" };
    }
    if (filePath && line !== undefined) {
      const row = ctx.readDb
        .query<{ id: string; content: string }, [string, number, number]>(
          `SELECT id, content FROM code_chunks
           WHERE file_path = ? AND start_line <= ? AND end_line >= ? LIMIT 1`,
        )
        .get(filePath, line, line);
      return row ?? { content: "" };
    }
    throw new Error("provide symbol or path+line");
  },
};
