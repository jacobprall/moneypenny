//! Brain store — CRUD for the brain registry.
//!
//! A brain is the aggregate root for knowledge, behaviors, memories, and focus.
//! One brain per agent DB (D6: one DB per brain).

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Brain {
    pub id: String,
    pub name: String,
    pub mission: Option<String>,
    pub config: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone)]
pub struct NewBrain {
    pub id: Option<String>,
    pub name: String,
    pub mission: Option<String>,
    pub config: Option<String>,
}

/// Create a new brain. If id is None, uses a new UUID.
pub fn create(conn: &Connection, brain: &NewBrain) -> anyhow::Result<String> {
    let id = brain
        .id
        .clone()
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let now = chrono::Utc::now().timestamp();

    conn.execute(
        "INSERT INTO brains (id, name, mission, config, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            id,
            brain.name,
            brain.mission,
            brain.config,
            now,
            now,
        ],
    )?;

    Ok(id)
}

/// Get a brain by id.
pub fn get(conn: &Connection, brain_id: &str) -> anyhow::Result<Option<Brain>> {
    let row = conn.query_row(
        "SELECT id, name, mission, config, created_at, updated_at
         FROM brains WHERE id = ?1",
        [brain_id],
        |r| {
            Ok(Brain {
                id: r.get(0)?,
                name: r.get(1)?,
                mission: r.get(2)?,
                config: r.get(3)?,
                created_at: r.get(4)?,
                updated_at: r.get(5)?,
            })
        },
    );

    match row {
        Ok(b) => Ok(Some(b)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// List all brains (typically one per agent DB).
pub fn list(conn: &Connection, limit: Option<usize>) -> anyhow::Result<Vec<Brain>> {
    let limit = limit.unwrap_or(100);
    let mut stmt = conn.prepare(
        "SELECT id, name, mission, config, created_at, updated_at
         FROM brains ORDER BY created_at DESC LIMIT ?1",
    )?;
    let brains = stmt
        .query_map([limit], |r| {
            Ok(Brain {
                id: r.get(0)?,
                name: r.get(1)?,
                mission: r.get(2)?,
                config: r.get(3)?,
                created_at: r.get(4)?,
                updated_at: r.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(brains)
}

/// Update a brain's name, mission, or config.
pub fn update(
    conn: &Connection,
    brain_id: &str,
    name: Option<&str>,
    mission: Option<&str>,
    config: Option<&str>,
) -> anyhow::Result<()> {
    let now = chrono::Utc::now().timestamp();

    let (cur_name, cur_mission, cur_config): (String, Option<String>, Option<String>) = conn
        .query_row(
            "SELECT name, mission, config FROM brains WHERE id = ?1",
            [brain_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(|e| anyhow::anyhow!("brain not found: {e}"))?;

    let new_name = name.unwrap_or(&cur_name);
    let new_mission = mission.or(cur_mission.as_deref());
    let new_config = config.or(cur_config.as_deref());

    conn.execute(
        "UPDATE brains SET name = ?1, mission = ?2, config = ?3, updated_at = ?4 WHERE id = ?5",
        params![new_name, new_mission, new_config, now, brain_id],
    )?;

    Ok(())
}

/// Delete a brain. Caller must ensure no orphaned artifacts.
pub fn delete(conn: &Connection, brain_id: &str) -> anyhow::Result<()> {
    conn.execute("DELETE FROM brains WHERE id = ?1", [brain_id])?;
    Ok(())
}

/// Ensure a default brain exists for the given agent_id. Creates one if missing.
/// Returns the brain_id for use as default.
pub fn ensure_default(conn: &Connection, agent_id: &str, agent_name: &str) -> anyhow::Result<String> {
    if let Some(b) = get(conn, agent_id)? {
        return Ok(b.id);
    }

    let brain = NewBrain {
        id: Some(agent_id.to_string()),
        name: agent_name.to_string(),
        mission: None,
        config: None,
    };
    create(conn, &brain)
}
