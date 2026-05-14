import { existsSync, readdirSync } from "node:fs";
import * as path from "node:path";
import {
  createAgentDB,
  closeAgentDB,
  listSessions,
  labelSession,
  type AgentDB,
  type LocalGen,
} from "@swe/db";
import { summariseSession, type SummariseConfig } from "./summarise.js";

export interface AutoLabelConfig extends SummariseConfig {
  repoPath: string;
  localGen?: LocalGen;
}

function discoverAgentDbPaths(repoPath: string): string[] {
  const agentsDir = path.join(repoPath, ".swe", "agents");
  const paths: string[] = [];

  if (existsSync(agentsDir)) {
    try {
      for (const f of readdirSync(agentsDir)) {
        if (f.endsWith(".db")) {
          paths.push(path.join(agentsDir, f));
        }
      }
    } catch { /* skip */ }
  }

  return paths;
}

interface EarlyMessages {
  userText: string | null;
  assistantText: string | null;
}

function getFirstMessages(db: AgentDB, sessionId: string): EarlyMessages {
  const rows = db.db
    .prepare(
      `SELECT role, content FROM messages
       WHERE session_id = ? AND role IN ('user', 'assistant')
       ORDER BY turn ASC, created_at ASC
       LIMIT 10`,
    )
    .all(sessionId) as { role: string; content: string | null }[];

  let userText: string | null = null;
  let assistantText: string | null = null;

  for (const row of rows) {
    if (row.role === "user" && userText === null && row.content) {
      userText = row.content.slice(0, 400);
    }
    if (row.role === "assistant" && assistantText === null && row.content) {
      assistantText = row.content.slice(0, 400);
    }
    if (userText && assistantText) break;
  }

  return { userText, assistantText };
}

/**
 * Scans all agent DBs for the repo, finds eligible unlabelled sessions,
 * and labels them. Resolves when done. Never rejects.
 */
export async function runAutoLabel(config: AutoLabelConfig): Promise<void> {
  try {
    const dbPaths = discoverAgentDbPaths(config.repoPath);

    for (const dbPath of dbPaths) {
      let db: AgentDB | undefined;
      try {
        db = createAgentDB(dbPath);
        const sessions = listSessions(db);

        for (const session of sessions) {
          if (session.label !== null) continue;
          if (session.id === db.activeSessionId) continue;

          try {
            const { userText, assistantText } = getFirstMessages(db, session.id);
            if (!userText || !assistantText) continue;

            const label = await summariseSession(
              { userText, assistantText },
              { ...config, localGen: config.localGen },
            );
            if (label) {
              labelSession(db, session.id, label);
            }
          } catch { /* skip this session */ }
        }
      } catch { /* skip this DB */ } finally {
        if (db) try { closeAgentDB(db); } catch { /* best effort */ }
      }
    }
  } catch { /* never reject */ }
}
