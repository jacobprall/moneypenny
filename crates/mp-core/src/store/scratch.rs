use rusqlite::{Connection, params};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct ScratchEntry {
    pub id: String,
    pub session_id: String,
    pub key: String,
    pub content: String,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Set a scratch entry (insert or update by session_id + key).
pub fn set(
    conn: &Connection,
    session_id: &str,
    key: &str,
    content: &str,
) -> anyhow::Result<String> {
    let now = chrono::Utc::now().timestamp();

    let existing_id: Option<String> = conn
        .query_row(
            "SELECT id FROM scratch WHERE session_id = ?1 AND key = ?2",
            params![session_id, key],
            |r| r.get(0),
        )
        .ok();

    match existing_id {
        Some(id) => {
            conn.execute(
                "UPDATE scratch SET content = ?1, updated_at = ?2 WHERE id = ?3",
                params![content, now, id],
            )?;
            Ok(id)
        }
        None => {
            let id = Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO scratch (id, session_id, key, content, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![id, session_id, key, content, now, now],
            )?;
            Ok(id)
        }
    }
}

/// Get a scratch entry by session and key.
pub fn get(conn: &Connection, session_id: &str, key: &str) -> anyhow::Result<Option<ScratchEntry>> {
    let entry = conn
        .query_row(
            "SELECT id, session_id, key, content, created_at, updated_at
         FROM scratch WHERE session_id = ?1 AND key = ?2",
            params![session_id, key],
            |r| {
                Ok(ScratchEntry {
                    id: r.get(0)?,
                    session_id: r.get(1)?,
                    key: r.get(2)?,
                    content: r.get(3)?,
                    created_at: r.get(4)?,
                    updated_at: r.get(5)?,
                })
            },
        )
        .ok();
    Ok(entry)
}

/// Get all scratch entries for a session.
pub fn list(conn: &Connection, session_id: &str) -> anyhow::Result<Vec<ScratchEntry>> {
    let mut stmt = conn.prepare(
        "SELECT id, session_id, key, content, created_at, updated_at
         FROM scratch WHERE session_id = ?1 ORDER BY created_at ASC",
    )?;
    let entries = stmt
        .query_map([session_id], |r| {
            Ok(ScratchEntry {
                id: r.get(0)?,
                session_id: r.get(1)?,
                key: r.get(2)?,
                content: r.get(3)?,
                created_at: r.get(4)?,
                updated_at: r.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(entries)
}

/// Delete a scratch entry by ID.
pub fn remove(conn: &Connection, entry_id: &str) -> anyhow::Result<()> {
    conn.execute("DELETE FROM scratch WHERE id = ?1", [entry_id])?;
    Ok(())
}

/// Clear all scratch entries for a session (end-of-session cleanup).
pub fn clear_session(conn: &Connection, session_id: &str) -> anyhow::Result<usize> {
    let deleted = conn.execute("DELETE FROM scratch WHERE session_id = ?1", [session_id])?;
    Ok(deleted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db, schema};

    fn setup() -> Connection {
        let conn = db::open_memory().unwrap();
        schema::init_agent_db(&conn).unwrap();
        conn
    }

    #[test]
    fn set_and_get() {
        let conn = setup();
        set(&conn, "session-1", "plan", "Step 1: do the thing").unwrap();
        let entry = get(&conn, "session-1", "plan").unwrap().unwrap();
        assert_eq!(entry.content, "Step 1: do the thing");
        assert_eq!(entry.key, "plan");
    }

    #[test]
    fn set_upserts_on_same_key() {
        let conn = setup();
        let id1 = set(&conn, "session-1", "plan", "v1").unwrap();
        let id2 = set(&conn, "session-1", "plan", "v2").unwrap();
        assert_eq!(id1, id2, "should update same row");

        let entry = get(&conn, "session-1", "plan").unwrap().unwrap();
        assert_eq!(entry.content, "v2");
    }

    #[test]
    fn different_keys_are_independent() {
        let conn = setup();
        set(&conn, "session-1", "plan", "the plan").unwrap();
        set(&conn, "session-1", "findings", "found something").unwrap();

        let entries = list(&conn, "session-1").unwrap();
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn different_sessions_are_independent() {
        let conn = setup();
        set(&conn, "s1", "key", "val1").unwrap();
        set(&conn, "s2", "key", "val2").unwrap();

        assert_eq!(get(&conn, "s1", "key").unwrap().unwrap().content, "val1");
        assert_eq!(get(&conn, "s2", "key").unwrap().unwrap().content, "val2");
    }

    #[test]
    fn remove_deletes_entry() {
        let conn = setup();
        let id = set(&conn, "s1", "plan", "stuff").unwrap();
        remove(&conn, &id).unwrap();
        assert!(get(&conn, "s1", "plan").unwrap().is_none());
    }

    #[test]
    fn clear_session_removes_all() {
        let conn = setup();
        set(&conn, "s1", "a", "1").unwrap();
        set(&conn, "s1", "b", "2").unwrap();
        set(&conn, "s2", "a", "3").unwrap();

        let deleted = clear_session(&conn, "s1").unwrap();
        assert_eq!(deleted, 2);
        assert!(list(&conn, "s1").unwrap().is_empty());
        assert_eq!(list(&conn, "s2").unwrap().len(), 1);
    }

    #[test]
    fn get_nonexistent_returns_none() {
        let conn = setup();
        assert!(get(&conn, "nope", "nope").unwrap().is_none());
    }
}
