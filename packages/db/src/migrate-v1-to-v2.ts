import { Database } from "bun:sqlite";
import { randomUUID } from "node:crypto";
import { readdir } from "node:fs/promises";
import { join } from "node:path";

const V2_MARKER = 200;
const V2_BASELINE = 100;

function qIdent(name: string): string {
  return name.replace(/"/g, '""');
}

function dropEverything(db: Database): void {
  db.exec("PRAGMA foreign_keys = OFF");
  const triggers = db
    .query<{ name: string }, []>(
      `SELECT name FROM sqlite_master WHERE type='trigger'`,
    )
    .all();
  for (const { name } of triggers) {
    db.exec(`DROP TRIGGER IF EXISTS "${qIdent(name)}"`);
  }
  const views = db
    .query<{ name: string }, []>(`SELECT name FROM sqlite_master WHERE type='view'`)
    .all();
  for (const { name } of views) {
    db.exec(`DROP VIEW IF EXISTS "${qIdent(name)}"`);
  }
  for (;;) {
    const tables = db
      .query<{ name: string }, []>(
        `SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'`,
      )
      .all();
    if (tables.length === 0) break;
    for (const { name } of tables) {
      db.exec(`DROP TABLE IF EXISTS "${qIdent(name)}"`);
    }
  }
  db.exec("PRAGMA foreign_keys = ON");
}

function tableExists(db: Database, name: string): boolean {
  const row = db
    .query<{ n: number }, [string]>(
      `SELECT COUNT(1) as n FROM sqlite_master WHERE type='table' AND name = ?`,
    )
    .get(name);
  return (row?.n ?? 0) > 0;
}

function columnExists(db: Database, table: string, col: string): boolean {
  const row = db
    .query<{ n: number }, [string, string]>(
      `SELECT COUNT(1) as n FROM pragma_table_info(?) WHERE name = ?`,
    )
    .get(table, col);
  return (row?.n ?? 0) > 0;
}

async function readSortedSqlFiles(dir: string): Promise<string[]> {
  const names = (await readdir(dir)).filter((f) => f.endsWith(".sql")).sort();
  const out: string[] = [];
  for (const n of names) {
    out.push(await Bun.file(join(dir, n)).text());
  }
  return out;
}

function getSchemaVersion(db: Database): number {
  if (!tableExists(db, "schema_version")) return 0;
  const row = db
    .query<{ version: number }, []>(
      "SELECT COALESCE(MAX(version), 0) as version FROM schema_version",
    )
    .get();
  return row?.version ?? 0;
}

function isV1SessionsShape(db: Database): boolean {
  return tableExists(db, "sessions") && columnExists(db, "sessions", "is_active");
}

type V1Session = {
  id: string;
  label: string | null;
  created_at: number;
  last_active_at: number;
  is_active: number;
};

type V1Message = {
  id: string;
  turn: number;
  role: string;
  content: string | null;
  tool_calls: string | null;
  tool_call_id: string | null;
  session_id: string | null;
  created_at: number;
};

type V1CodeChunk = Record<string, unknown>;

type V1Skill = Record<string, unknown>;

type V1Convention = Record<string, unknown>;

type V1Pointer = Record<string, unknown>;

type V1Work = Record<string, unknown>;

type V1Config = { key: string; value: string; updated_at: number };

type V1Event = Record<string, unknown>;

type V1Policy = Record<string, unknown>;

function safeAll<T>(db: Database, sql: string): T[] {
  try {
    return db.query<T, []>(sql).all();
  } catch {
    return [];
  }
}

function tryParseJson(s: string): Record<string, unknown> {
  try {
    const v = JSON.parse(s) as unknown;
    return v && typeof v === "object" && !Array.isArray(v)
      ? (v as Record<string, unknown>)
      : {};
  } catch {
    return {};
  }
}

function asBlob(v: unknown): Uint8Array | null {
  if (v == null) return null;
  if (v instanceof Uint8Array) return v;
  if (v instanceof ArrayBuffer) return new Uint8Array(v);
  if (typeof Buffer !== "undefined" && Buffer.isBuffer(v))
    return new Uint8Array(v.buffer, v.byteOffset, v.byteLength);
  return null;
}

export async function migrateV1ToV2(
  writeDb: Database,
  sqlV2Dir: string,
  dbPath: string,
): Promise<{
  migrated: boolean;
  messageCount: number;
  sessionCount: number;
  backupPath: string | null;
}> {
  const ver = getSchemaVersion(writeDb);
  if (ver >= V2_MARKER) {
    return {
      migrated: false,
      messageCount: 0,
      sessionCount: 0,
      backupPath: null,
    };
  }
  if (ver >= V2_BASELINE) {
    return {
      migrated: false,
      messageCount: 0,
      sessionCount: 0,
      backupPath: null,
    };
  }

  const shouldMigrate = ver > 0 || isV1SessionsShape(writeDb);
  if (!shouldMigrate) {
    return {
      migrated: false,
      messageCount: 0,
      sessionCount: 0,
      backupPath: null,
    };
  }

  const backupPath = `${dbPath}.v1.bak`;
  await Bun.write(backupPath, await Bun.file(dbPath).arrayBuffer());

  const v1Sessions = safeAll<V1Session>(
    writeDb,
    `SELECT id, label, created_at, last_active_at, is_active FROM sessions`,
  );
  const v1Messages = safeAll<V1Message>(
    writeDb,
    `SELECT id, turn, role, content, tool_calls, tool_call_id, session_id, created_at FROM messages`,
  );

  const chunkCols = columnExists(writeDb, "code_chunks", "embed_dims")
    ? `id, file_path, chunk_index, content, language, symbol_name, start_line, end_line, embedding, embed_dims AS embedding_dim, updated_at`
    : `id, file_path, chunk_index, content, language, symbol_name, start_line, end_line, embedding, NULL AS embedding_dim, updated_at`;

  const v1Chunks = safeAll<V1CodeChunk>(
    writeDb,
    `SELECT ${chunkCols} FROM code_chunks`,
  );

  const v1Skills = safeAll<V1Skill>(writeDb, `SELECT * FROM skills`);
  const v1Conventions = safeAll<V1Convention>(writeDb, `SELECT * FROM conventions`);
  const v1Pointers = safeAll<V1Pointer>(writeDb, `SELECT * FROM session_pointers`);
  const v1Work = safeAll<V1Work>(writeDb, `SELECT * FROM work_queue`);
  const v1Config = safeAll<V1Config>(writeDb, `SELECT key, value, updated_at FROM config`);
  const v1Events = safeAll<V1Event>(writeDb, `SELECT * FROM events`);
  const v1Policies = safeAll<V1Policy>(writeDb, `SELECT * FROM policies`);

  const ddl = await readSortedSqlFiles(sqlV2Dir);

  const runBySession = new Map<string, string>();
  for (const s of v1Sessions) {
    runBySession.set(s.id, randomUUID());
  }

  const messagesBySession = new Map<string, V1Message[]>();
  for (const m of v1Messages) {
    if (!m.session_id) continue;
    const list = messagesBySession.get(m.session_id) ?? [];
    list.push(m);
    messagesBySession.set(m.session_id, list);
  }
  for (const [, list] of messagesBySession) {
    list.sort((a, b) => {
      if (a.turn !== b.turn) return a.turn - b.turn;
      return a.created_at - b.created_at;
    });
  }

  let messageCount = 0;

  writeDb.transaction(() => {
    dropEverything(writeDb);
    writeDb.exec(`
      CREATE TABLE schema_version (
        version INTEGER NOT NULL,
        applied_at INTEGER NOT NULL DEFAULT (unixepoch())
      )
    `);
    for (const sql of ddl) {
      writeDb.exec(sql);
    }

    const insSession = writeDb.query<
      unknown,
      [string, string | null, string, number, number]
    >(
      `INSERT INTO sessions (id, label, status, config, config_version, created_at, last_active_at)
       VALUES (?, ?, ?, '{}', 0, ?, ?)`,
    );
    for (const s of v1Sessions) {
      const status = s.is_active === 1 ? "active" : "archived";
      insSession.run(s.id, s.label, status, s.created_at, s.last_active_at);
    }

    const insRun = writeDb.query<
      unknown,
      [string, string, number, number]
    >(
      `INSERT INTO runs (id, session_id, status, started_at, finished_at)
       VALUES (?, ?, 'complete', ?, ?)`,
    );
    for (const s of v1Sessions) {
      const rid = runBySession.get(s.id)!;
      insRun.run(rid, s.id, s.last_active_at, s.last_active_at);
    }

    const insMsg = writeDb.query<
      unknown,
      [string, string, string, number, string, string | null, string | null, string | null, number]
    >(
      `INSERT INTO messages (id, session_id, run_id, seq, role, content, tool_calls, tool_call_id, pending, created_at)
       VALUES (?, ?, ?, ?, ?, ?, ?, ?, 0, ?)`,
    );

    for (const s of v1Sessions) {
      const list = messagesBySession.get(s.id) ?? [];
      const rid = runBySession.get(s.id)!;
      let seq = 0;
      for (const m of list) {
        insMsg.run(
          m.id,
          m.session_id!,
          rid,
          seq,
          m.role,
          m.content,
          m.tool_calls,
          m.tool_call_id,
          m.created_at,
        );
        seq++;
        messageCount++;
      }
    }

    const insChunk = writeDb.query<
      unknown,
      [string, string, number, string, string | null, string | null, number | null, number | null, Uint8Array | null, number | null, number]
    >(
      `INSERT INTO code_chunks (id, file_path, chunk_index, content, language, symbol_name, start_line, end_line, embedding, embedding_dim, updated_at)
       VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
    );
    for (const ch of v1Chunks) {
      insChunk.run(
        String(ch.id),
        String(ch.file_path),
        Number(ch.chunk_index),
        String(ch.content),
        ch.language != null ? String(ch.language) : null,
        ch.symbol_name != null ? String(ch.symbol_name) : null,
        ch.start_line != null ? Number(ch.start_line) : null,
        ch.end_line != null ? Number(ch.end_line) : null,
        asBlob(ch.embedding),
        ch.embedding_dim != null ? Number(ch.embedding_dim) : null,
        Number(ch.updated_at ?? Math.floor(Date.now() / 1000)),
      );
    }

    const insSkill = writeDb.query<
      unknown,
      [string, string, string, string | null, number, string | null, number]
    >(
      `INSERT INTO skills (id, name, description, instructions, confidence, source_session_id, created_at)
       VALUES (?, ?, ?, ?, ?, ?, ?)`,
    );
    for (const sk of v1Skills) {
      const sid = sk.source_session_id != null ? String(sk.source_session_id) : null;
      insSkill.run(
        String(sk.id),
        String(sk.name),
        String(sk.description),
        sk.instructions != null ? String(sk.instructions) : null,
        Number(sk.confidence ?? 0.5),
        sid,
        Number(sk.created_at ?? Math.floor(Date.now() / 1000)),
      );
    }

    const insConv = writeDb.query<
      unknown,
      [string, string, string, string, number, string | null, number]
    >(
      `INSERT OR IGNORE INTO conventions (id, name, category, description, confidence, source_session_id, created_at)
       VALUES (?, ?, ?, ?, ?, ?, ?)`,
    );
    for (const c of v1Conventions) {
      insConv.run(
        String(c.id),
        String(c.name),
        String(c.category),
        String(c.description),
        Number(c.confidence ?? 0.5),
        null,
        Number(c.created_at ?? Math.floor(Date.now() / 1000)),
      );
    }

    const insPtr = writeDb.query<
      unknown,
      [string, string, string, string, number, number, number]
    >(
      `INSERT INTO session_pointers (id, session_id, key, phrase, pinned, archived, created_at)
       VALUES (?, ?, ?, ?, ?, ?, ?)`,
    );
    for (const p of v1Pointers) {
      insPtr.run(
        String(p.id),
        String(p.session_id),
        String(p.key),
        String(p.phrase),
        Number(p.pinned ?? 0),
        Number(p.archived ?? 0),
        Number(p.created_at ?? Math.floor(Date.now() / 1000)),
      );
    }

    const insWq = writeDb.query<
      unknown,
      [string, string | null, string | null, number, number | null, string | null]
    >(
      `INSERT INTO work_queue (type, session_id, payload, created_at, processed_at, error)
       VALUES (?, ?, ?, ?, ?, ?)`,
    );
    for (const w of v1Work) {
      insWq.run(
        String(w.type),
        w.session_id != null ? String(w.session_id) : null,
        w.payload != null ? String(w.payload) : null,
        Number(w.created_at ?? Math.floor(Date.now() / 1000)),
        w.processed_at != null ? Number(w.processed_at) : null,
        w.error != null ? String(w.error) : null,
      );
    }

    const insCfg = writeDb.query<unknown, [string, string, number]>(
      `INSERT INTO config (key, value, updated_at) VALUES (?, ?, ?)`,
    );
    for (const c of v1Config) {
      insCfg.run(c.key, c.value, c.updated_at);
    }

    const insEv = writeDb.query<
      unknown,
      [string, string | null, string | null, number]
    >(
      `INSERT INTO events (type, session_id, run_id, blueprint, detail, created_at)
       VALUES (?, ?, NULL, NULL, ?, ?)`,
    );
    for (const e of v1Events) {
      let detail: string | null =
        e.detail != null ? String(e.detail) : null;
      if (e.agent_name != null) {
        const base = detail ?? "{}";
        detail = JSON.stringify({
          ...tryParseJson(base),
          v1_agent_name: String(e.agent_name),
        });
      }
      insEv.run(
        String(e.type),
        e.session_id != null ? String(e.session_id) : null,
        detail,
        Number(e.created_at ?? Math.floor(Date.now() / 1000)),
      );
    }

    const insPol = writeDb.query<
      unknown,
      [string, string, string, string, string | null, number, string, number]
    >(
      `INSERT OR REPLACE INTO policies (id, name, effect, description, conditions, enabled, source_path, updated_at)
       VALUES (?, ?, ?, ?, ?, ?, ?, ?)`,
    );
    for (const p of v1Policies) {
      const effectRaw = String(p.effect ?? "warn");
      const effect =
        effectRaw === "deny" || effectRaw === "allow" || effectRaw === "warn"
          ? effectRaw
          : "warn";
      insPol.run(
        String(p.id),
        String(p.name),
        effect,
        String(p.description),
        p.conditions != null ? String(p.conditions) : null,
        Number(p.enabled ?? 1),
        p.source_path != null ? String(p.source_path) : "",
        Number(p.updated_at ?? p.created_at ?? Math.floor(Date.now() / 1000)),
      );
    }

    writeDb.exec("DELETE FROM schema_version");
    writeDb.exec(
      `INSERT INTO schema_version (version) VALUES (${V2_MARKER})`,
    );
  })();

  return {
    migrated: true,
    messageCount,
    sessionCount: v1Sessions.length,
    backupPath,
  };
}
