import { randomUUID } from "node:crypto";
import { z } from "zod";
import type { ToolDef } from "../types.js";

export const learnSkillTool: ToolDef<
  {
    name: string;
    description: string;
    instructions?: string;
    confidence?: number;
  },
  { id: string }
> = {
  name: "learn_skill",
  description: "Persist a reusable skill for future sessions.",
  category: "knowledge",
  permissions: {},
  inputSchema: z.object({
    name: z.string(),
    description: z.string(),
    instructions: z.string().optional(),
    confidence: z.number().min(0).max(1).optional().default(0.6),
  }),
  execute: async ({ name, description, instructions, confidence }, ctx) => {
    const id = randomUUID();
    ctx.writeDb
      .prepare(
        `INSERT INTO skills (id, name, description, instructions, confidence, source_session_id, created_at)
         VALUES (?, ?, ?, ?, ?, ?, unixepoch())`,
      )
      .run(id, name, description, instructions ?? null, confidence, ctx.sessionId);
    ctx.events.emit({
      type: "knowledge.skill_extracted",
      session_id: ctx.sessionId,
      run_id: ctx.runId,
      detail: { skill_name: name, session_id: ctx.sessionId },
    });
    return { id };
  },
};
