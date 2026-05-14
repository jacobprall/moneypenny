/**
 * Zod schema for agent.md frontmatter.
 *
 * `id` is intentionally NOT a valid field — the agent's id is the directory
 * name. Specifying `id` in frontmatter is rejected at validation time.
 */

import { z } from "zod";
import cronParser from "cron-parser";

const AGENT_ID_RE = /^[a-z][a-z0-9-]{1,63}$/;
const CRON_FIELD_COUNTS = new Set([5, 6]);

const permissionsSchema = z
  .object({
    allow: z.array(z.string()).default([]),
    deny: z.array(z.string()).default([]),
  })
  .default({ allow: [], deny: [] });

export const frontmatterSchema = z
  .object({
    name: z.string().min(1, "name is required"),
    description: z.string().optional(),
    enabled: z.boolean().default(true),

    schedule: z.string().optional(),
    timezone: z.string().optional(),
    catch_up: z.boolean().default(false),

    on_complete: z.array(z.string()).default([]),
    on_failure: z.array(z.string()).default([]),

    model: z.string().optional(),
    max_turns: z.number().int().positive().max(500).default(30),
    max_cost_per_session: z.number().positive().optional(),
    max_cost_per_turn: z.number().positive().optional(),
    timeout_ms: z.number().int().positive().default(15 * 60 * 1000),

    tools: z.array(z.string()).default([]),
    permissions: permissionsSchema,
    policies: z.array(z.string()).optional(),
    skills: z.array(z.string()).default([]),
  })
  .passthrough();

export type AgentFrontmatter = z.infer<typeof frontmatterSchema>;

export interface ValidationError {
  field: string;
  message: string;
}

export interface ValidateResult {
  ok: boolean;
  errors: ValidationError[];
  config?: AgentFrontmatter;
}

export function validateFrontmatter(raw: unknown): ValidateResult {
  if (raw && typeof raw === "object" && "id" in (raw as object)) {
    return {
      ok: false,
      errors: [
        {
          field: "id",
          message: "id is inferred from the directory name; remove it from frontmatter",
        },
      ],
    };
  }

  const parsed = frontmatterSchema.safeParse(raw ?? {});
  if (!parsed.success) {
    return {
      ok: false,
      errors: parsed.error.issues.map((e) => ({
        field: e.path.join(".") || "(root)",
        message: e.message,
      })),
    };
  }

  const config = parsed.data;
  const errors: ValidationError[] = [];

  if (config.schedule) {
    const fields = config.schedule.trim().split(/\s+/);
    if (!CRON_FIELD_COUNTS.has(fields.length)) {
      errors.push({
        field: "schedule",
        message: `expected 5 or 6 cron fields, got ${fields.length}`,
      });
    } else {
      try {
        cronParser.parse(config.schedule, {
          tz: config.timezone,
        });
      } catch (e) {
        errors.push({
          field: "schedule",
          message: e instanceof Error ? e.message : String(e),
        });
      }
    }
    if (!config.timezone) {
      errors.push({
        field: "timezone",
        message: "timezone is required when schedule is set",
      });
    }
  }

  if (errors.length > 0) {
    return { ok: false, errors, config };
  }
  return { ok: true, errors: [], config };
}

export function validateAgentId(id: string): ValidationError | null {
  if (!AGENT_ID_RE.test(id)) {
    return {
      field: "id",
      message: `invalid id "${id}"; must match ${AGENT_ID_RE}`,
    };
  }
  return null;
}
