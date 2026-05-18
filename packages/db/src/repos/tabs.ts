import { Database } from "bun:sqlite";
import { randomUUID } from "node:crypto";

export type Tab = {
  id: string;
  kind: string;
  session_id: string | null;
  label: string | null;
  position: number;
  is_active: number;
  opened_at: number;
};

export function listTabs(db: Database): Tab[] {
  return db
    .query<Tab, []>(`SELECT * FROM tabs ORDER BY position ASC, opened_at ASC`)
    .all();
}

export function createTab(
  db: Database,
  input: {
    kind: string;
    sessionId?: string | null;
    label?: string | null;
    position?: number;
    active?: boolean;
  },
): Tab {
  const id = randomUUID();
  let position = input.position;
  if (position == null) {
    const row = db
      .query<{ m: number | null }, []>(`SELECT MAX(position) AS m FROM tabs`)
      .get();
    position = (row?.m == null ? 0 : row.m + 1);
  }
  const isActive = input.active ? 1 : 0;
  db.transaction(() => {
    if (isActive) {
      db.exec(`UPDATE tabs SET is_active = 0`);
    }
    db.query<
      unknown,
      [string, string, string | null, string | null, number, number]
    >(
      `INSERT INTO tabs (id, kind, session_id, label, position, is_active)
       VALUES (?, ?, ?, ?, ?, ?)`,
    ).run(
      id,
      input.kind,
      input.sessionId ?? null,
      input.label ?? null,
      position,
      isActive,
    );
  })();
  return db.query<Tab, [string]>(`SELECT * FROM tabs WHERE id = ?`).get(id)!;
}

export function updateTab(
  db: Database,
  id: string,
  patch: { position?: number; label?: string | null },
): void {
  if (patch.position != null && patch.label !== undefined) {
    db.query<unknown, [number, string | null, string]>(
      `UPDATE tabs SET position = ?, label = ? WHERE id = ?`,
    ).run(patch.position, patch.label, id);
  } else if (patch.position != null) {
    db.query<unknown, [number, string]>(
      `UPDATE tabs SET position = ? WHERE id = ?`,
    ).run(patch.position, id);
  } else if (patch.label !== undefined) {
    db.query<unknown, [string | null, string]>(
      `UPDATE tabs SET label = ? WHERE id = ?`,
    ).run(patch.label, id);
  }
}

export function deleteTab(db: Database, id: string): void {
  db.query<unknown, [string]>(`DELETE FROM tabs WHERE id = ?`).run(id);
}

export function setActiveTab(db: Database, tabId: string): void {
  db.transaction(() => {
    db.exec(`UPDATE tabs SET is_active = 0`);
    db.query<unknown, [string]>(`UPDATE tabs SET is_active = 1 WHERE id = ?`).run(
      tabId,
    );
  })();
}
