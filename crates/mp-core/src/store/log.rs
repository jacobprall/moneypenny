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

#[derive(Debug, Clone)]
pub struct ToolCallRecord {
    pub id: String,
    pub message_id: String,
    pub session_id: String,
    pub tool_name: String,
    pub arguments: Option<String>,
    pub result: Option<String>,
    pub status: Option<String>,
    pub policy_decision: Option<String>,
    pub duration_ms: Option<i64>,
    pub created_at: i64,
}

/// Create a new session. Returns the session ID.
pub fn create_session(conn: &Connection, agent_id: &str, channel: Option<&str>) -> anyhow::Result<String> {
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
    let session = conn.query_row(
        "SELECT id, agent_id, channel, started_at, ended_at, summary FROM sessions WHERE id = ?1",
        [session_id],
        |r| Ok(Session {
            id: r.get(0)?,
            agent_id: r.get(1)?,
            channel: r.get(2)?,
            started_at: r.get(3)?,
            ended_at: r.get(4)?,
            summary: r.get(5)?,
        }),
    ).ok();
    Ok(session)
}

/// Append a message to the log. Returns the message ID.
pub fn append_message(conn: &Connection, session_id: &str, role: &str, content: &str) -> anyhow::Result<String> {
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
         WHERE session_id = ?1 ORDER BY created_at ASC"
    )?;
    let msgs = stmt.query_map([session_id], |r| {
        Ok(Message {
            id: r.get(0)?,
            session_id: r.get(1)?,
            role: r.get(2)?,
            content: r.get(3)?,
            created_at: r.get(4)?,
        })
    })?.collect::<Result<Vec<_>, _>>()?;
    Ok(msgs)
}

/// Get the last N messages for a session.
pub fn get_recent_messages(conn: &Connection, session_id: &str, limit: usize) -> anyhow::Result<Vec<Message>> {
    let mut stmt = conn.prepare(
        "SELECT id, session_id, role, content, created_at FROM messages
         WHERE session_id = ?1 ORDER BY rowid DESC LIMIT ?2"
    )?;
    let mut msgs = stmt.query_map(params![session_id, limit], |r| {
        Ok(Message {
            id: r.get(0)?,
            session_id: r.get(1)?,
            role: r.get(2)?,
            content: r.get(3)?,
            created_at: r.get(4)?,
        })
    })?.collect::<Result<Vec<_>, _>>()?;
    msgs.reverse();
    Ok(msgs)
}

/// Record a tool call. Returns the tool call ID.
pub fn record_tool_call(
    conn: &Connection,
    message_id: &str,
    session_id: &str,
    tool_name: &str,
    arguments: Option<&str>,
    result: Option<&str>,
    status: Option<&str>,
    policy_decision: Option<&str>,
    duration_ms: Option<i64>,
) -> anyhow::Result<String> {
    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp();
    conn.execute(
        "INSERT INTO tool_calls (id, message_id, session_id, tool_name, arguments, result, status, policy_decision, duration_ms, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![id, message_id, session_id, tool_name, arguments, result, status, policy_decision, duration_ms, now],
    )?;
    Ok(id)
}

/// Get tool calls for a session.
pub fn get_tool_calls(conn: &Connection, session_id: &str) -> anyhow::Result<Vec<ToolCallRecord>> {
    let mut stmt = conn.prepare(
        "SELECT id, message_id, session_id, tool_name, arguments, result, status, policy_decision, duration_ms, created_at
         FROM tool_calls WHERE session_id = ?1 ORDER BY created_at ASC"
    )?;
    let calls = stmt.query_map([session_id], |r| {
        Ok(ToolCallRecord {
            id: r.get(0)?,
            message_id: r.get(1)?,
            session_id: r.get(2)?,
            tool_name: r.get(3)?,
            arguments: r.get(4)?,
            result: r.get(5)?,
            status: r.get(6)?,
            policy_decision: r.get(7)?,
            duration_ms: r.get(8)?,
            created_at: r.get(9)?,
        })
    })?.collect::<Result<Vec<_>, _>>()?;
    Ok(calls)
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
    fn create_and_get_session() {
        let conn = setup();
        let sid = create_session(&conn, "agent-main", Some("cli")).unwrap();
        let s = get_session(&conn, &sid).unwrap().unwrap();
        assert_eq!(s.agent_id, "agent-main");
        assert_eq!(s.channel.as_deref(), Some("cli"));
        assert!(s.ended_at.is_none());
    }

    #[test]
    fn end_session_sets_ended_at() {
        let conn = setup();
        let sid = create_session(&conn, "agent-main", None).unwrap();
        end_session(&conn, &sid).unwrap();
        let s = get_session(&conn, &sid).unwrap().unwrap();
        assert!(s.ended_at.is_some());
    }

    #[test]
    fn update_session_summary() {
        let conn = setup();
        let sid = create_session(&conn, "agent-main", None).unwrap();
        update_summary(&conn, &sid, "User asked about Rust.").unwrap();
        let s = get_session(&conn, &sid).unwrap().unwrap();
        assert_eq!(s.summary.as_deref(), Some("User asked about Rust."));
    }

    #[test]
    fn append_and_get_messages() {
        let conn = setup();
        let sid = create_session(&conn, "a", None).unwrap();
        append_message(&conn, &sid, "user", "hello").unwrap();
        append_message(&conn, &sid, "assistant", "hi there").unwrap();

        let msgs = get_messages(&conn, &sid).unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].role, "user");
        assert_eq!(msgs[1].role, "assistant");
    }

    #[test]
    fn get_recent_messages_limits_and_orders() {
        let conn = setup();
        let sid = create_session(&conn, "a", None).unwrap();
        for i in 0..10 {
            append_message(&conn, &sid, "user", &format!("msg {i}")).unwrap();
        }
        let recent = get_recent_messages(&conn, &sid, 3).unwrap();
        assert_eq!(recent.len(), 3);
        assert_eq!(recent[0].content, "msg 7");
        assert_eq!(recent[2].content, "msg 9");
    }

    #[test]
    fn record_and_get_tool_calls() {
        let conn = setup();
        let sid = create_session(&conn, "a", None).unwrap();
        let mid = append_message(&conn, &sid, "assistant", "calling tool").unwrap();

        record_tool_call(
            &conn, &mid, &sid, "shell_exec",
            Some(r#"{"cmd":"ls"}"#), Some("file.txt"), Some("success"),
            Some("allowed"), Some(42),
        ).unwrap();

        let calls = get_tool_calls(&conn, &sid).unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].tool_name, "shell_exec");
        assert_eq!(calls[0].status.as_deref(), Some("success"));
        assert_eq!(calls[0].duration_ms, Some(42));
    }

    #[test]
    fn messages_are_append_only_ordered() {
        let conn = setup();
        let sid = create_session(&conn, "a", None).unwrap();
        let m1 = append_message(&conn, &sid, "user", "first").unwrap();
        let m2 = append_message(&conn, &sid, "assistant", "second").unwrap();

        let msgs = get_messages(&conn, &sid).unwrap();
        assert_eq!(msgs[0].id, m1);
        assert_eq!(msgs[1].id, m2);
    }

    #[test]
    fn get_nonexistent_session_returns_none() {
        let conn = setup();
        assert!(get_session(&conn, "nope").unwrap().is_none());
    }
}
