import type { SearchOptions, SearchResult, Skill, SubagentDef } from "@moneypenny/db/types";

/** Hybrid search plus exclude patterns for grep fallback. */
export interface SearchService {
  hybridSearch(query: string, opts?: SearchOptions): SearchResult[];
  validateAndRefreshResults(results: SearchResult[]): SearchResult[];
  getExcludePatterns(): string[];
}

/** Workspace index write-through and batch reindex. */
export interface WorkspaceService {
  reindexFile(relPath: string, opts?: { content?: string }): void;
  reindexFiles(relPaths: string[]): void;
}

export interface SkillService {
  getSkill(name: string): Skill | null;
  getSkillFile(name: string, path: string): string | undefined;
  listSkillFiles(name: string): string[];
}

export interface SubagentService {
  getSubagentDef(name: string): SubagentDef | null;
}

export interface ConversationService {
  compactConversation(upToTurn: number, summary: string): void;
}

/**
 * Read-only SELECT: validates SELECT/WITH, appends LIMIT when missing; runs on the agent
 * read pool (`AgentDB.reads` in `@moneypenny/db`).
 * Throws on disallowed SQL or execution errors.
 */
export interface QueryService {
  executeReadOnlyQuery(sql: string, params?: (string | number)[]): Record<string, unknown>[];
}

import type { ContextCurateService } from "./context-curate-service.js";

export interface ToolServices {
  search: SearchService;
  workspace: WorkspaceService;
  skills: SkillService;
  subagents: SubagentService;
  conversation: ConversationService;
  query: QueryService;
  contextCurate: ContextCurateService;
}
