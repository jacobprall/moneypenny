import { existsSync } from "node:fs";
import * as path from "node:path";
import {
  createAgentDB,
  closeAgentDB,
  listSessions,
  labelSession,
  type AgentDB,
  type LocalGen,
} from "@moneypenny/db";
import { summariseSession, type SummariseConfig } from "./summarise.js";

export interface AutoLabelConfig extends SummariseConfig {
  repoPath: string;
  localGen?: LocalGen;
}

/** Returns the single mp.db path for a repo. */
function getMpDbPath(repoPath: string): string | null {
  const dbPath = path.join(repoPath, ".mp", "mp.db");
  return existsSync(dbPath) ? dbPath : null;
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
 * Opens the single mp.db, finds eligible unlabelled sessions,
 * and labels them. Resolves when done. Never rejects.
 */
export async function runAutoLabel(config: AutoLabelConfig): Promise<void> {
  try {
    const dbPath = getMpDbPath(config.repoPath);
    if (!dbPath) return;

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
    } catch { /* skip */ } finally {
      if (db) try { closeAgentDB(db); } catch { /* best effort */ }
    }
  } catch { /* never reject */ }
}
