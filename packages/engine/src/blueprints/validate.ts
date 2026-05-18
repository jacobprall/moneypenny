import { basename } from "node:path";
import type { Blueprint } from "./types.js";

export const KNOWN_TOOL_NAMES = new Set([
  "read_file",
  "write_file",
  "edit_file",
  "list_directory",
  "search_code",
  "find_symbol",
  "read_symbol",
  "search_messages",
  "expand_previous_session",
  "spawn_agent",
  "request_human_input",
  "change_directory",
  "run_command",
  "learn_skill",
  "record_pointer",
  "query_conventions",
]);

const MODEL_HINTS = new Set([
  "gpt-4o",
  "gpt-4o-mini",
  "gpt-4-turbo",
  "claude-sonnet-4-20250514",
  "claude-3-5-sonnet-20241022",
]);

function validCron(expr: string): boolean {
  const parts = expr.trim().split(/\s+/);
  return parts.length >= 5 && parts.length <= 6;
}

export type ValidateBlueprintResult =
  | { ok: true; blueprint: Blueprint; warnings: string[] }
  | { ok: false; errors: string[] };

export function validateBlueprint(bp: Blueprint): ValidateBlueprintResult {
  const errors: string[] = [];
  const warnings: string[] = [];

  if (!bp.name.trim()) errors.push("name is required");

  const stem = basename(bp.path, ".md");
  if (stem !== bp.name) {
    warnings.push(`filename ${stem}.md does not match name ${bp.name}; using frontmatter name`);
  }

  if (bp.tools !== null) {
    const next: string[] = [];
    for (const t of bp.tools) {
      if (KNOWN_TOOL_NAMES.has(t)) next.push(t);
      else warnings.push(`unknown tool dropped: ${t}`);
    }
    bp.tools = next;
  }

  if (bp.model && !MODEL_HINTS.has(bp.model)) {
    warnings.push(`model may be unsupported at runtime: ${bp.model}`);
  }

  if (bp.trigger_on === "schedule") {
    if (!bp.schedule?.trim()) errors.push("schedule required when trigger_on=schedule");
    else if (!validCron(bp.schedule)) errors.push("schedule is not a valid cron expression");
  }

  if (bp.trigger_on === "file_change") {
    if (!bp.file_glob?.length) errors.push("file_glob required when trigger_on=file_change");
  }

  if (errors.length) return { ok: false, errors };

  return { ok: true, blueprint: bp, warnings };
}
