import { Database } from "bun:sqlite";
import { randomUUID } from "node:crypto";

export type Message = {
  id: string;
  session_id: string;
  run_id: string | null;
  seq: number;
  role: string;
  content: string | null;
  tool_calls: string | null;
  tool_call_id: string | null;
  pending: number;
  created_at: number;
};

export function nextSeq(db: Database, sessionId: string): number {
  const row = db
    .query<{ m: number | null }, [string]>(
      `SELECT MAX(seq) AS m FROM messages WHERE session_id = ?`,
    )
    .get(sessionId);
  const m = row?.m;
  return (m == null ? -1 : m) + 1;
}

export function insertMessage(
  db: Database,
  input: {
    sessionId: string;
    runId?: string | null;
    seq: number;
    role: string;
    content?: string | null;
    toolCalls?: string | null;
    toolCallId?: string | null;
    pending?: number;
  },
): Message {
  const id = randomUUID();
  db.query<
    unknown,
    [
      string,
      string,
      string | null,
      number,
      string,
      string | null,
      string | null,
      string | null,
      number,
    ]
  >(
    `INSERT INTO messages (id, session_id, run_id, seq, role, content, tool_calls, tool_call_id, pending)
     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)`,
  ).run(
    id,
    input.sessionId,
    input.runId ?? null,
    input.seq,
    input.role,
    input.content ?? null,
    input.toolCalls ?? null,
    input.toolCallId ?? null,
    input.pending ?? 0,
  );
  return db
    .query<Message, [string]>(`SELECT * FROM messages WHERE id = ?`)
    .get(id)!;
}

export function insertPendingUserMessage(
  db: Database,
  sessionId: string,
  content: string,
): Message {
  return insertMessage(db, {
    sessionId,
    seq: nextSeq(db, sessionId),
    role: "user",
    content,
    pending: 1,
  });
}

export function appendAssistantMessage(
  db: Database,
  input: {
    sessionId: string;
    runId: string | null;
    content?: string | null;
    toolCalls?: string | null;
  },
): Message {
  return insertMessage(db, {
    sessionId: input.sessionId,
    runId: input.runId,
    seq: nextSeq(db, input.sessionId),
    role: "assistant",
    content: input.content ?? null,
    toolCalls: input.toolCalls ?? null,
  });
}

export function appendToolResultMessage(
  db: Database,
  input: {
    sessionId: string;
    runId: string | null;
    toolCallId: string;
    content: string;
  },
): Message {
  return insertMessage(db, {
    sessionId: input.sessionId,
    runId: input.runId,
    seq: nextSeq(db, input.sessionId),
    role: "tool",
    content: input.content,
    toolCallId: input.toolCallId,
  });
}

export function drainPending(db: Database, sessionId: string): void {
  db.query<unknown, [string]>(
    `UPDATE messages SET pending = 0 WHERE session_id = ? AND pending = 1`,
  ).run(sessionId);
}

export function listMessages(
  db: Database,
  input: {
    sessionId: string;
    cursorSeq?: number | null;
    direction: "before" | "after";
    limit: number;
  },
): Message[] {
  const lim = Math.min(Math.max(input.limit, 1), 500);
  if (input.cursorSeq == null) {
    return db
      .query<Message, [string, number]>(
        `SELECT * FROM messages WHERE session_id = ? ORDER BY seq DESC LIMIT ?`,
      )
      .all(input.sessionId, lim)
      .reverse();
  }
  if (input.direction === "after") {
    return db
      .query<Message, [string, number, number]>(
        `SELECT * FROM messages WHERE session_id = ? AND seq > ? ORDER BY seq ASC LIMIT ?`,
      )
      .all(input.sessionId, input.cursorSeq, lim);
  }
  return db
    .query<Message, [string, number, number]>(
      `SELECT * FROM messages WHERE session_id = ? AND seq < ? ORDER BY seq DESC LIMIT ?`,
    )
    .all(input.sessionId, input.cursorSeq, lim)
    .reverse();
}

export function searchMessagesFts(db: Database, ftsQuery: string): Message[] {
  return db
    .query<Message, [string]>(
      `SELECT m.* FROM messages m
       JOIN messages_fts ON messages_fts.rowid = m.rowid
       WHERE messages_fts MATCH ?
       ORDER BY m.created_at DESC
       LIMIT 200`,
    )
    .all(ftsQuery);
}
