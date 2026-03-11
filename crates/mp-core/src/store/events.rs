//! Unified events store — append-only log for brain.memories.events.
//!
//! New writes go to the events table. Unified query searches events +
//! activity_log + policy_audit (legacy). Per D3 Option C.

use rusqlite::{Connection, params};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct Event {
    pub id: String,
    pub brain_id: String,
    pub event_type: String,
    pub action: String,
    pub resource: Option<String>,
    pub actor: Option<String>,
    pub session_id: Option<String>,
    pub correlation_id: Option<String>,
    pub detail: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
pub struct AppendInput {
    pub brain_id: String,
    pub event_type: String,
    pub action: String,
    pub resource: Option<String>,
    pub actor: Option<String>,
    pub session_id: Option<String>,
    pub correlation_id: Option<String>,
    pub detail: Option<String>,
}

/// Append a new event. Primary write path for brain.memories.events.
pub fn append(conn: &Connection, input: &AppendInput) -> anyhow::Result<String> {
    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp();

    conn.execute(
        "INSERT INTO events (id, brain_id, event_type, action, resource, actor, session_id, correlation_id, detail, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            id,
            input.brain_id,
            input.event_type,
            input.action,
            input.resource,
            input.actor,
            input.session_id,
            input.correlation_id,
            input.detail,
            now,
        ],
    )?;

    Ok(id)
}

/// Unified query: searches events table, then activity_log and policy_audit.
/// For legacy tables we use brain_id as agent_id (1:1 in single-brain case).
pub fn query(
    conn: &Connection,
    brain_id: &str,
    event_type: Option<&str>,
    action: Option<&str>,
    resource: Option<&str>,
    session_id: Option<&str>,
    query_text: Option<&str>,
    limit: Option<usize>,
) -> anyhow::Result<Vec<serde_json::Value>> {
    let limit = limit.unwrap_or(50).min(500) as i64;
    let half = (limit / 3).max(1); // ~1/3 from each source

    let mut results = Vec::new();

    // 1. Events table (new writes)
    let mut sql = String::from(
        "SELECT id, brain_id, event_type, action, resource, actor, session_id, correlation_id, detail, created_at
         FROM events WHERE brain_id = ?1",
    );
    let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(brain_id.to_string())];

    if let Some(et) = event_type {
        sql.push_str(" AND event_type = ?");
        params_vec.push(Box::new(et.to_string()));
    }
    if let Some(a) = action {
        sql.push_str(" AND action = ?");
        params_vec.push(Box::new(a.to_string()));
    }
    if let Some(r) = resource {
        sql.push_str(" AND resource = ?");
        params_vec.push(Box::new(r.to_string()));
    }
    if let Some(s) = session_id {
        sql.push_str(" AND session_id = ?");
        params_vec.push(Box::new(s.to_string()));
    }
    if let Some(q) = query_text {
        sql.push_str(" AND (detail LIKE ? OR action LIKE ? OR resource LIKE ?)");
        let pattern = format!("%{q}%");
        params_vec.push(Box::new(pattern.clone()));
        params_vec.push(Box::new(pattern.clone()));
        params_vec.push(Box::new(pattern));
    }
    sql.push_str(" ORDER BY created_at DESC LIMIT ?");
    params_vec.push(Box::new(half));

    let param_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|b| b.as_ref()).collect();
    let mut stmt = conn.prepare(&sql)?;
    let rows: Vec<serde_json::Value> = stmt
        .query_map(rusqlite::params_from_iter(param_refs), |r| {
            Ok(serde_json::json!({
                "source": "events",
                "id": r.get::<_, String>(0)?,
                "brain_id": r.get::<_, String>(1)?,
                "event_type": r.get::<_, String>(2)?,
                "action": r.get::<_, String>(3)?,
                "resource": r.get::<_, Option<String>>(4)?,
                "actor": r.get::<_, Option<String>>(5)?,
                "session_id": r.get::<_, Option<String>>(6)?,
                "correlation_id": r.get::<_, Option<String>>(7)?,
                "detail": r.get::<_, Option<String>>(8)?,
                "created_at": r.get::<_, i64>(9)?,
            }))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    results.extend(rows);

    // 2. activity_log (legacy) — use brain_id as agent_id
    if table_has_column(conn, "activity_log", "agent_id") {
        let mut sql = String::from(
            "SELECT id, agent_id, event, action, resource, detail, conversation_id, duration_ms, created_at
             FROM activity_log WHERE agent_id = ?1",
        );
        let mut p: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(brain_id.to_string())];
        if let Some(et) = event_type {
            sql.push_str(" AND event = ?");
            p.push(Box::new(et.to_string()));
        }
        if let Some(a) = action {
            sql.push_str(" AND action = ?");
            p.push(Box::new(a.to_string()));
        }
        if let Some(r) = resource {
            sql.push_str(" AND resource = ?");
            p.push(Box::new(r.to_string()));
        }
        if let Some(q) = query_text {
            sql.push_str(" AND (detail LIKE ? OR event LIKE ? OR action LIKE ? OR resource LIKE ?)");
            let pattern = format!("%{q}%");
            for _ in 0..4 {
                p.push(Box::new(pattern.clone()));
            }
        }
        sql.push_str(" ORDER BY created_at DESC LIMIT ?");
        p.push(Box::new(half));

        let pref: Vec<&dyn rusqlite::ToSql> = p.iter().map(|b| b.as_ref()).collect();
        if let Ok(mut stmt) = conn.prepare(&sql) {
            let rows: Vec<serde_json::Value> = stmt
                .query_map(rusqlite::params_from_iter(pref), |r| {
                    Ok(serde_json::json!({
                        "source": "activity_log",
                        "id": r.get::<_, String>(0)?,
                        "agent_id": r.get::<_, String>(1)?,
                        "event_type": r.get::<_, String>(2)?,
                        "action": r.get::<_, String>(3)?,
                        "resource": r.get::<_, String>(4)?,
                        "detail": r.get::<_, String>(5)?,
                        "conversation_id": r.get::<_, String>(6)?,
                        "duration_ms": r.get::<_, Option<i64>>(7)?,
                        "created_at": r.get::<_, i64>(8)?,
                    }))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            results.extend(rows);
        }
    }

    // 3. policy_audit (legacy)
    let mut sql = String::from(
        "SELECT id, policy_id, actor, action, resource, effect, reason, session_id, created_at
         FROM policy_audit WHERE 1=1",
    );
    let mut p: Vec<Box<dyn rusqlite::ToSql>> = vec![];
    if let Some(a) = action {
        sql.push_str(" AND action = ?");
        p.push(Box::new(a.to_string()));
    }
    if let Some(r) = resource {
        sql.push_str(" AND resource = ?");
        p.push(Box::new(r.to_string()));
    }
    if let Some(s) = session_id {
        sql.push_str(" AND session_id = ?");
        p.push(Box::new(s.to_string()));
    }
    if let Some(q) = query_text {
        sql.push_str(" AND (reason LIKE ? OR actor LIKE ? OR action LIKE ? OR resource LIKE ?)");
        let pattern = format!("%{q}%");
        for _ in 0..4 {
            p.push(Box::new(pattern.clone()));
        }
    }
    sql.push_str(" ORDER BY created_at DESC LIMIT ?");
    p.push(Box::new(half));

    let pref: Vec<&dyn rusqlite::ToSql> = p.iter().map(|b| b.as_ref()).collect();
    if let Ok(mut stmt) = conn.prepare(&sql) {
        let rows: Vec<serde_json::Value> = stmt
            .query_map(rusqlite::params_from_iter(pref), |r| {
                Ok(serde_json::json!({
                    "source": "policy_audit",
                    "id": r.get::<_, String>(0)?,
                    "policy_id": r.get::<_, Option<String>>(1)?,
                    "actor": r.get::<_, String>(2)?,
                    "action": r.get::<_, String>(3)?,
                    "resource": r.get::<_, String>(4)?,
                    "effect": r.get::<_, String>(5)?,
                    "reason": r.get::<_, Option<String>>(6)?,
                    "session_id": r.get::<_, Option<String>>(7)?,
                    "created_at": r.get::<_, i64>(8)?,
                }))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        results.extend(rows);
    }

    // Sort by created_at desc and truncate to limit
    results.sort_by(|a, b| {
        let ca = a["created_at"].as_i64().unwrap_or(0);
        let cb = b["created_at"].as_i64().unwrap_or(0);
        cb.cmp(&ca)
    });
    results.truncate(limit as usize);

    Ok(results)
}

/// Compact: delete events older than N days.
pub fn compact(conn: &Connection, brain_id: &str, older_than_days: i64) -> anyhow::Result<usize> {
    let cutoff = chrono::Utc::now().timestamp() - older_than_days * 24 * 3600;
    let n = conn.execute(
        "DELETE FROM events WHERE brain_id = ?1 AND created_at < ?2",
        params![brain_id, cutoff],
    )?;
    Ok(n)
}

fn table_has_column(conn: &Connection, table: &str, column: &str) -> bool {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info(?1) WHERE name = ?2)",
        params![table, column],
        |r| r.get::<_, bool>(0),
    )
    .unwrap_or(false)
}
