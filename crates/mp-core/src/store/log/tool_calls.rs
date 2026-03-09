use rusqlite::{Connection, params};
use uuid::Uuid;

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

pub fn tool_call_projection_expr(alias: &str) -> String {
    format!(
        "'[tool_call] tool=' || {a}.tool_name || \
         ' status=' || COALESCE({a}.status, '') || \
         ' policy=' || COALESCE({a}.policy_decision, '') || \
         ' args=' || COALESCE({a}.arguments, '') || \
         ' result=' || COALESCE({a}.result, '')",
        a = alias
    )
}

/// Return (id, composed_text) for tool calls missing embeddings.
pub fn tool_calls_without_embedding(
    conn: &Connection,
    agent_id: &str,
) -> anyhow::Result<Vec<(String, String)>> {
    let projection = tool_call_projection_expr("tc");
    let sql = format!(
        "SELECT tc.id, ({projection}) AS content
         FROM tool_calls tc
         JOIN sessions s ON s.id = tc.session_id
         WHERE s.agent_id = ?1
           AND tc.content_embedding IS NULL
         ORDER BY tc.created_at ASC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(params![agent_id], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Write or overwrite the FLOAT32 content embedding for a tool call.
pub fn set_tool_call_embedding(
    conn: &Connection,
    tool_call_id: &str,
    blob: &[u8],
) -> anyhow::Result<()> {
    set_tool_call_embedding_with_meta(conn, tool_call_id, blob, None, None)
}

/// Write or overwrite the FLOAT32 content embedding for a tool call and persist
/// embedding provenance metadata.
pub fn set_tool_call_embedding_with_meta(
    conn: &Connection,
    tool_call_id: &str,
    blob: &[u8],
    embedding_model: Option<&str>,
    embedding_content_hash: Option<&str>,
) -> anyhow::Result<()> {
    conn.execute(
        "UPDATE tool_calls
         SET content_embedding = ?1,
             embedding_model = ?2,
             embedding_content_hash = ?3
         WHERE id = ?4",
        params![blob, embedding_model, embedding_content_hash, tool_call_id],
    )?;
    Ok(())
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
        params![
            id,
            message_id,
            session_id,
            tool_name,
            arguments,
            result,
            status,
            policy_decision,
            duration_ms,
            now
        ],
    )?;
    Ok(id)
}

/// Get tool calls for a session.
pub fn get_tool_calls(conn: &Connection, session_id: &str) -> anyhow::Result<Vec<ToolCallRecord>> {
    let mut stmt = conn.prepare(
        "SELECT id, message_id, session_id, tool_name, arguments, result, status, policy_decision, duration_ms, created_at
         FROM tool_calls WHERE session_id = ?1 ORDER BY created_at ASC",
    )?;
    let calls = stmt
        .query_map([session_id], |r| {
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
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(calls)
}
