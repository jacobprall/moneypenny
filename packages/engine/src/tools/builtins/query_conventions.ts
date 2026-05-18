import { z } from "zod";
import type { ToolDef } from "../types.js";

export const queryConventionsTool: ToolDef<
  Record<string, never>,
  { rows: Array<{ name: string; category: string; description: string }> }
> = {
  name: "query_conventions",
  description: "List detected project conventions (read-only).",
  category: "knowledge",
  permissions: {},
  inputSchema: z.object({}),
  execute: (_args, ctx) => {
    const rows = ctx.readDb
      .query<{ name: string; category: string; description: string }, []>(
        `SELECT name, category, description FROM conventions ORDER BY name`,
      )
      .all();
    return { rows };
  },
};
