import type { AgentDB } from "@moneypenny/db";
import {
  compactConversation,
  getSkill,
  getSkillFile,
  getSubagentDef,
  listSkillFiles,
} from "@moneypenny/db";
import { hybridSearch, getExcludePatterns } from "@moneypenny/search";
import { reindexFile, reindexFiles, validateAndRefreshResults } from "@moneypenny/db/workspace";
import type { ToolServices } from "./services.js";
import { createContextCurateService } from "./context-curate-service.js";

const QUERY_MAX_ROWS = 200;
const ALLOWED_SQL_PREFIX = /^\s*(?:--[^\n]*\n\s*)*(SELECT|WITH)\b/i;

function ensureQueryLimit(sql: string): string {
  if (/\bLIMIT\b/i.test(sql)) return sql;
  return `${sql.replace(/;\s*$/, "")} LIMIT ${QUERY_MAX_ROWS}`;
}

function logWorkspaceReindexFailure(op: string, relPath: string | undefined, e: unknown): void {
  const msg = e instanceof Error ? e.message : String(e);
  const pathPart = relPath != null ? ` path=${relPath}` : "";
  console.warn(`[mp] workspace ${op} failed:${pathPart} ${msg}`);
}

export function createToolServices(db: AgentDB): ToolServices {
  return {
    search: {
      hybridSearch(query, opts) {
        return hybridSearch(db, query, opts);
      },
      validateAndRefreshResults(results) {
        if (!db.workspace) return results;
        return validateAndRefreshResults(db.workspace, results);
      },
      getExcludePatterns() {
        try {
          return getExcludePatterns(db);
        } catch {
          return [];
        }
      },
    },

    workspace: {
      reindexFile(relPath, opts) {
        if (!db.workspace) return;
        try {
          reindexFile(
            db.workspace,
            relPath,
            opts?.content != null ? { content: opts.content } : undefined,
          );
        } catch (e) {
          logWorkspaceReindexFailure("reindexFile", relPath, e);
        }
      },
      reindexFiles(relPaths) {
        if (!db.workspace) return;
        try {
          reindexFiles(db.workspace, relPaths);
        } catch (e) {
          logWorkspaceReindexFailure("reindexFiles", relPaths.join(","), e);
        }
      },
    },

    skills: {
      getSkill: (name) => getSkill(db, name) ?? null,
      getSkillFile: (name, path) => getSkillFile(db, name, path),
      listSkillFiles: (name) => listSkillFiles(db, name),
    },

    subagents: {
      getSubagentDef: (name) => getSubagentDef(db, name) ?? null,
    },

    conversation: {
      compactConversation: (upToTurn, summary) => compactConversation(db, upToTurn, summary),
    },

    query: {
      executeReadOnlyQuery(query, params) {
        if (!ALLOWED_SQL_PREFIX.test(query)) {
          throw new Error("only SELECT statements are permitted");
        }
        const bounded = ensureQueryLimit(query);
        return db.reads.read((readDb) => {
          const stmt = readDb.prepare(bounded);
          return stmt.all(...(params ?? [])) as Record<string, unknown>[];
        });
      },
    },

    contextCurate: createContextCurateService(db),
  };
}
