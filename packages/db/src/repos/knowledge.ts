import { Database } from "bun:sqlite";
import { randomUUID } from "node:crypto";

export type Skill = {
  id: string;
  name: string;
  description: string;
  instructions: string | null;
  confidence: number;
  source_session_id: string | null;
  created_at: number;
};

export type Convention = {
  id: string;
  name: string;
  category: string;
  description: string;
  confidence: number;
  source_session_id: string | null;
  created_at: number;
};

export type SessionPointer = {
  id: string;
  session_id: string;
  key: string;
  phrase: string;
  pinned: number;
  archived: number;
  created_at: number;
};

export function listSkills(db: Database): Skill[] {
  return db.query<Skill, []>(`SELECT * FROM skills ORDER BY confidence DESC`).all();
}

export function insertSkill(
  db: Database,
  input: Omit<Skill, "id" | "created_at"> & { id?: string },
): Skill {
  const id = input.id ?? randomUUID();
  db.query<
    unknown,
    [string, string, string, string | null, number, string | null]
  >(
    `INSERT INTO skills (id, name, description, instructions, confidence, source_session_id)
     VALUES (?, ?, ?, ?, ?, ?)`,
  ).run(
    id,
    input.name,
    input.description,
    input.instructions ?? null,
    input.confidence,
    input.source_session_id ?? null,
  );
  return db.query<Skill, [string]>(`SELECT * FROM skills WHERE id = ?`).get(id)!;
}

export function listConventions(db: Database): Convention[] {
  return db
    .query<Convention, []>(`SELECT * FROM conventions ORDER BY name ASC`)
    .all();
}

export function upsertConvention(
  db: Database,
  input: Omit<Convention, "created_at"> & { created_at?: number },
): Convention {
  const createdAt = input.created_at ?? Math.floor(Date.now() / 1000);
  db.query<
    unknown,
    [string, string, string, string, number, string | null, number]
  >(
    `INSERT INTO conventions (id, name, category, description, confidence, source_session_id, created_at)
     VALUES (?, ?, ?, ?, ?, ?, ?)
     ON CONFLICT(name) DO UPDATE SET
       category = excluded.category,
       description = excluded.description,
       confidence = excluded.confidence,
       source_session_id = excluded.source_session_id`,
  ).run(
    input.id,
    input.name,
    input.category,
    input.description,
    input.confidence,
    input.source_session_id ?? null,
    createdAt,
  );
  return db
    .query<Convention, [string]>(`SELECT * FROM conventions WHERE name = ?`)
    .get(input.name)!;
}

export function listPointers(
  db: Database,
  filter?: { sessionId?: string; pinnedOnly?: boolean },
): SessionPointer[] {
  const sid = filter?.sessionId;
  const pin = filter?.pinnedOnly;
  if (sid != null && pin) {
    return db
      .query<SessionPointer, [string]>(
        `SELECT * FROM session_pointers WHERE archived = 0 AND session_id = ? AND pinned = 1
         ORDER BY created_at DESC`,
      )
      .all(sid);
  }
  if (sid != null) {
    return db
      .query<SessionPointer, [string]>(
        `SELECT * FROM session_pointers WHERE archived = 0 AND session_id = ?
         ORDER BY pinned DESC, created_at DESC`,
      )
      .all(sid);
  }
  if (pin) {
    return db
      .query<SessionPointer, []>(
        `SELECT * FROM session_pointers WHERE archived = 0 AND pinned = 1
         ORDER BY created_at DESC`,
      )
      .all();
  }
  return db
    .query<SessionPointer, []>(
      `SELECT * FROM session_pointers WHERE archived = 0
       ORDER BY pinned DESC, created_at DESC`,
    )
    .all();
}

export function insertPointer(
  db: Database,
  input: {
    sessionId: string;
    key: string;
    phrase: string;
    pinned?: number;
  },
): SessionPointer {
  const id = randomUUID();
  const pinned = input.pinned ?? 0;
  db.query<unknown, [string, string, string, string, number]>(
    `INSERT INTO session_pointers (id, session_id, key, phrase, pinned)
     VALUES (?, ?, ?, ?, ?)`,
  ).run(id, input.sessionId, input.key, input.phrase, pinned);
  return db
    .query<SessionPointer, [string]>(`SELECT * FROM session_pointers WHERE id = ?`)
    .get(id)!;
}

export function archivePointer(db: Database, id: string): void {
  db.query<unknown, [string]>(
    `UPDATE session_pointers SET archived = 1 WHERE id = ?`,
  ).run(id);
}

export function pinPointer(db: Database, id: string, pinned: boolean): void {
  db.query<unknown, [number, string]>(
    `UPDATE session_pointers SET pinned = ? WHERE id = ?`,
  ).run(pinned ? 1 : 0, id);
}
