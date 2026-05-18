import { z } from "zod";
import type { ToolDef } from "../types.js";

export const expandPreviousSessionTool: ToolDef<
  { label: string; limit?: number },
  { messages: Array<{ role: string; content: string | null; seq: number }> }
> = {
  name: "expand_previous_session",
  description: "Last N messages from the most recent session with this label.",
  category: "session",
  permissions: {},
  inputSchema: z.object({
    label: z.string(),
    limit: z.number().optional().default(40),
  }),
  execute: async ({ label, limit }, ctx) => {
    const sess = ctx.readDb
      .query<{ id: string }, [string]>(
        `SELECT id FROM sessions WHERE label = ? ORDER BY created_at DESC LIMIT 1`,
      )
      .get(label);
    if (!sess) return { messages: [] };
    const messages = ctx.readDb
      .query<
        { role: string; content: string | null; seq: number },
        [string, number]
      >(
        `SELECT role, content, seq FROM messages
         WHERE session_id = ? ORDER BY seq DESC LIMIT ?`,
      )
      .all(sess.id, limit);
    return { messages: messages.reverse() };
  },
};
