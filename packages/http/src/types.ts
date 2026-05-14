import type { AgentDB } from "@moneypenny/db";

export type { AgentDB };

export interface CreateHttpAppOptions {
  /** Returns the active agent database (or null when not ready). */
  getDb: () => AgentDB | null;
  /** API key for on-demand agent runs from HTTP. */
  getApiKey?: () => string | undefined;
  /** Directory scanned for agent.md definitions (reload, optional). */
  blueprintsDir?: string;
  /** Directory scanned for policy YAML files (reload, optional). */
  policiesDir?: string;
  /** If set, serves static files when the directory exists. */
  uiDistPath?: string;
}
