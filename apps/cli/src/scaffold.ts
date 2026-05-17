import { join } from "node:path";
import { mkdir } from "node:fs/promises";

const DEFAULT_AGENT = `name = "Moneypenny"
model = "claude-sonnet-4-20250514"
trigger_on = "manual"

# tools = ["expand_previous_session", "get_full_session", "search_code", "search_messages"]
`;

const BUDGET_POLICY = `name = "Budget Guard"
effect = "deny"
description = "Enforce daily and per-session cost limits"

[conditions]
maxDailyUsd = 10.0
maxSessionUsd = 2.0
`;

const EXAMPLE_CONVENTIONS = `[[convention]]
name = "TypeScript strict mode"
category = "language"
description = "All TypeScript files use strict mode and explicit return types on exported functions"

[[convention]]
name = "SQL migrations numbered"
category = "database"
description = "SQL migrations use numbered prefix format: 001_name.sql, 002_name.sql"
`;

export async function scaffoldConfig(repoRoot: string): Promise<boolean> {
  const mpDir = join(repoRoot, ".moneypenny");
  const agentsDir = join(mpDir, "agents");
  const policiesDir = join(mpDir, "policies");

  const defaultAgentPath = join(agentsDir, "default.toml");
  const exists = await Bun.file(defaultAgentPath).exists();
  if (exists) return false;

  await mkdir(agentsDir, { recursive: true });
  await mkdir(policiesDir, { recursive: true });

  await Bun.write(defaultAgentPath, DEFAULT_AGENT);
  await Bun.write(join(policiesDir, "budget.toml"), BUDGET_POLICY);
  await Bun.write(join(mpDir, "conventions.toml"), EXAMPLE_CONVENTIONS);

  return true;
}
