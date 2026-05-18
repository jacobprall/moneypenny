import { randomUUID } from "node:crypto";
import { z } from "zod";
import type { ToolDef } from "../types.js";

export const recordPointerTool: ToolDef<
  { key: string; phrase: string; pinned?: boolean },
  { id: string }
> = {
  name: "record_pointer",
  description: "Record a session pointer for later recall.",
  category: "knowledge",
  permissions: {},
  inputSchema: z.object({
    key: z.string(),
    phrase: z.string(),
    pinned: z.boolean().optional().default(false),
  }),
  execute: async ({ key, phrase, pinned }, ctx) => {
    const id = randomUUID();
    ctx.writeDb
      .prepare(
        `INSERT INTO session_pointers (id, session_id, key, phrase, pinned, archived, created_at)
         VALUES (?, ?, ?, ?, ?, 0, unixepoch())`,
      )
      .run(id, ctx.sessionId, key, phrase, pinned ? 1 : 0);
    ctx.events.emit({
      type: "knowledge.pointer_created",
      session_id: ctx.sessionId,
      run_id: ctx.runId,
      detail: { key, session_id: ctx.sessionId },
    });
    return { id };
  },
};
