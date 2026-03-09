use rusqlite::{Connection, params};

pub fn policy_audit_projection_expr(alias: &str) -> String {
    format!(
        "'[policy_audit] actor=' || {a}.actor || \
         ' action=' || {a}.action || \
         ' resource=' || {a}.resource || \
         ' effect=' || {a}.effect || \
         ' reason=' || COALESCE({a}.reason, '')",
        a = alias
    )
}

/// Return (id, composed_text) for policy audit rows missing embeddings.
pub fn policy_audit_without_embedding(
    conn: &Connection,
    agent_id: &str,
) -> anyhow::Result<Vec<(String, String)>> {
    let projection = policy_audit_projection_expr("pa");
    let sql = format!(
        "SELECT pa.id, ({projection}) AS content
         FROM policy_audit pa
         WHERE (
                pa.actor = ?1 OR
                pa.session_id IN (SELECT id FROM sessions WHERE agent_id = ?1)
               )
           AND pa.content_embedding IS NULL
         ORDER BY pa.created_at ASC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(params![agent_id], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Write or overwrite the FLOAT32 content embedding for a policy audit row.
pub fn set_policy_audit_embedding(
    conn: &Connection,
    audit_id: &str,
    blob: &[u8],
) -> anyhow::Result<()> {
    set_policy_audit_embedding_with_meta(conn, audit_id, blob, None, None)
}

/// Write or overwrite the FLOAT32 content embedding for a policy audit row and
/// persist embedding provenance metadata.
pub fn set_policy_audit_embedding_with_meta(
    conn: &Connection,
    audit_id: &str,
    blob: &[u8],
    embedding_model: Option<&str>,
    embedding_content_hash: Option<&str>,
) -> anyhow::Result<()> {
    conn.execute(
        "UPDATE policy_audit
         SET content_embedding = ?1,
             embedding_model = ?2,
             embedding_content_hash = ?3
         WHERE id = ?4",
        params![blob, embedding_model, embedding_content_hash, audit_id],
    )?;
    Ok(())
}
