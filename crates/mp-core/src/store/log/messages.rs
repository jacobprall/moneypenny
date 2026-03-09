use rusqlite::{Connection, params};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct Session {
    pub id: String,
    pub agent_id: String,
    pub channel: Option<String>,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub summary: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Message {
    pub id: String,
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub created_at: i64,
}

/// Create a new session. Returns the session ID.
pub fn create_session(
    conn: &Connection,
    agent_id: &str,
    channel: Option<&str>,
) -> anyhow::Result<String> {
    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp();
    conn.execute(
        "INSERT INTO sessions (id, agent_id, channel, started_at) VALUES (?1, ?2, ?3, ?4)",
        params![id, agent_id, channel, now],
    )?;
    Ok(id)
}

/// End a session.
pub fn end_session(conn: &Connection, session_id: &str) -> anyhow::Result<()> {
    let now = chrono::Utc::now().timestamp();
    conn.execute(
        "UPDATE sessions SET ended_at = ?1 WHERE id = ?2",
        params![now, session_id],
    )?;
    Ok(())
}

/// Update rolling summary for a session.
pub fn update_summary(conn: &Connection, session_id: &str, summary: &str) -> anyhow::Result<()> {
    conn.execute(
        "UPDATE sessions SET summary = ?1 WHERE id = ?2",
        params![summary, session_id],
    )?;
    Ok(())
}

/// Get a session by ID.
pub fn get_session(conn: &Connection, session_id: &str) -> anyhow::Result<Option<Session>> {
    let session = conn
        .query_row(
            "SELECT id, agent_id, channel, started_at, ended_at, summary FROM sessions WHERE id = ?1",
            [session_id],
            |r| {
                Ok(Session {
                    id: r.get(0)?,
                    agent_id: r.get(1)?,
                    channel: r.get(2)?,
                    started_at: r.get(3)?,
                    ended_at: r.get(4)?,
                    summary: r.get(5)?,
                })
            },
        )
        .ok();
    Ok(session)
}

/// Append a message to the log. Returns the message ID.
pub fn append_message(
    conn: &Connection,
    session_id: &str,
    role: &str,
    content: &str,
) -> anyhow::Result<String> {
    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp();
    conn.execute(
        "INSERT INTO messages (id, session_id, role, content, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, session_id, role, content, now],
    )?;
    Ok(id)
}

/// Get messages for a session, ordered by time.
pub fn get_messages(conn: &Connection, session_id: &str) -> anyhow::Result<Vec<Message>> {
    let mut stmt = conn.prepare(
        "SELECT id, session_id, role, content, created_at FROM messages
         WHERE session_id = ?1 ORDER BY created_at ASC",
    )?;
    let msgs = stmt
        .query_map([session_id], |r| {
            Ok(Message {
                id: r.get(0)?,
                session_id: r.get(1)?,
                role: r.get(2)?,
                content: r.get(3)?,
                created_at: r.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(msgs)
}

/// Get the last N messages for a session.
pub fn get_recent_messages(
    conn: &Connection,
    session_id: &str,
    limit: usize,
) -> anyhow::Result<Vec<Message>> {
    let mut stmt = conn.prepare(
        "SELECT id, session_id, role, content, created_at FROM messages
         WHERE session_id = ?1 ORDER BY rowid DESC LIMIT ?2",
    )?;
    let mut msgs = stmt
        .query_map(params![session_id, limit], |r| {
            Ok(Message {
                id: r.get(0)?,
                session_id: r.get(1)?,
                role: r.get(2)?,
                content: r.get(3)?,
                created_at: r.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    msgs.reverse();
    Ok(msgs)
}

/// Return (id, content) for agent-scoped messages that are missing embeddings.
pub fn messages_without_embedding(
    conn: &Connection,
    agent_id: &str,
) -> anyhow::Result<Vec<(String, String)>> {
    let mut stmt = conn.prepare(
        "SELECT m.id, m.content
         FROM messages m
         JOIN sessions s ON s.id = m.session_id
         WHERE s.agent_id = ?1
           AND m.content_embedding IS NULL
         ORDER BY m.created_at ASC",
    )?;
    let rows = stmt
        .query_map(params![agent_id], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Write or overwrite the FLOAT32 content embedding for a message.
pub fn set_message_embedding(
    conn: &Connection,
    message_id: &str,
    blob: &[u8],
) -> anyhow::Result<()> {
    conn.execute(
        "UPDATE messages SET content_embedding = ?1 WHERE id = ?2",
        params![blob, message_id],
    )?;
    Ok(())
}

