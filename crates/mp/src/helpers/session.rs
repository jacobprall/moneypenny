use anyhow::Result;

pub fn resolve_or_create_session(
    conn: &rusqlite::Connection,
    agent_name: &str,
    channel: Option<&str>,
    requested_session_id: Option<String>,
    force_new: bool,
) -> Result<(String, bool)> {
    if let Some(sid) = requested_session_id {
        let exists: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sessions WHERE id = ?1 AND agent_id = ?2",
            rusqlite::params![sid, agent_name],
            |r| r.get(0),
        )?;
        if exists > 0 {
            return Ok((sid, true));
        }

        let recent: Vec<String> = conn
            .prepare(
                "SELECT id FROM sessions WHERE agent_id = ?1 ORDER BY started_at DESC LIMIT 3",
            )?
            .query_map([agent_name], |r| r.get(0))?
            .collect::<Result<Vec<_>, _>>()?;

        let hint = if recent.is_empty() {
            "No sessions exist yet. Omit --session-id to create one.".to_string()
        } else {
            format!(
                "Recent sessions: {}\nFix: use one of the IDs above, or omit --session-id.",
                recent.join(", ")
            )
        };
        anyhow::bail!("Session '{sid}' not found for agent '{agent_name}'.\n{hint}");
    }

    if !force_new {
        if let Ok(sid) = find_recent_resumable_session(conn, agent_name, channel) {
            return Ok((sid, true));
        }
    }

    let sid = mp_core::store::log::create_session(conn, agent_name, channel)?;
    Ok((sid, false))
}

fn find_recent_resumable_session(
    conn: &rusqlite::Connection,
    agent_name: &str,
    channel: Option<&str>,
) -> Result<String> {
    let cutoff = chrono::Utc::now().timestamp() - 24 * 3600;
    let sid: String = if let Some(ch) = channel {
        conn.query_row(
            "SELECT s.id
             FROM sessions s
             LEFT JOIN messages m ON m.session_id = s.id
             WHERE s.agent_id = ?1 AND s.channel = ?2
               AND s.ended_at IS NULL
             GROUP BY s.id
             HAVING COALESCE(MAX(m.created_at), s.started_at) >= ?3
             ORDER BY COALESCE(MAX(m.created_at), s.started_at) DESC
             LIMIT 1",
            rusqlite::params![agent_name, ch, cutoff],
            |r| r.get(0),
        )?
    } else {
        conn.query_row(
            "SELECT s.id
             FROM sessions s
             LEFT JOIN messages m ON m.session_id = s.id
             WHERE s.agent_id = ?1
               AND s.ended_at IS NULL
             GROUP BY s.id
             HAVING COALESCE(MAX(m.created_at), s.started_at) >= ?2
             ORDER BY COALESCE(MAX(m.created_at), s.started_at) DESC
             LIMIT 1",
            rusqlite::params![agent_name, cutoff],
            |r| r.get(0),
        )?
    };
    Ok(sid)
}
