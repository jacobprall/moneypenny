use rusqlite::{Connection, params};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct Fact {
    pub id: String,
    pub agent_id: String,
    pub content: String,
    pub summary: String,
    pub pointer: String,
    pub content_embedding: Option<Vec<u8>>,
    pub summary_embedding: Option<Vec<u8>>,
    pub pointer_embedding: Option<Vec<u8>>,
    pub keywords: Option<String>,
    pub source_message_id: Option<String>,
    pub confidence: f64,
    pub created_at: i64,
    pub updated_at: i64,
    pub superseded_at: Option<i64>,
    pub version: i64,
}

#[derive(Debug, Clone)]
pub struct FactLink {
    pub source_id: String,
    pub target_id: String,
    pub relation: Option<String>,
    pub strength: f64,
}

#[derive(Debug, Clone)]
pub struct FactAuditEntry {
    pub id: String,
    pub fact_id: String,
    pub operation: String,
    pub old_content: Option<String>,
    pub new_content: Option<String>,
    pub reason: Option<String>,
    pub source_message_id: Option<String>,
    pub created_at: i64,
}

pub struct NewFact {
    pub agent_id: String,
    pub content: String,
    pub summary: String,
    pub pointer: String,
    pub keywords: Option<String>,
    pub source_message_id: Option<String>,
    pub confidence: f64,
}

/// Insert a new fact and log the addition in fact_audit.
pub fn add(conn: &Connection, fact: &NewFact, reason: Option<&str>) -> anyhow::Result<String> {
    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp();

    conn.execute(
        "INSERT INTO facts (id, agent_id, content, summary, pointer, keywords, source_message_id, confidence, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![id, fact.agent_id, fact.content, fact.summary, fact.pointer, fact.keywords, fact.source_message_id, fact.confidence, now, now],
    )?;

    let audit_id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO fact_audit (id, fact_id, operation, new_content, reason, source_message_id, created_at)
         VALUES (?1, ?2, 'add', ?3, ?4, ?5, ?6)",
        params![audit_id, id, fact.content, reason, fact.source_message_id, now],
    )?;

    Ok(id)
}

/// Update a fact's content, summary, pointer, and bump version. Logs to fact_audit.
pub fn update(
    conn: &Connection,
    fact_id: &str,
    new_content: &str,
    new_summary: &str,
    new_pointer: &str,
    reason: Option<&str>,
    source_message_id: Option<&str>,
) -> anyhow::Result<()> {
    let now = chrono::Utc::now().timestamp();

    let old_content: String = conn.query_row(
        "SELECT content FROM facts WHERE id = ?1",
        [fact_id],
        |r| r.get(0),
    )?;

    conn.execute(
        "UPDATE facts SET content = ?1, summary = ?2, pointer = ?3, updated_at = ?4, version = version + 1
         WHERE id = ?5",
        params![new_content, new_summary, new_pointer, now, fact_id],
    )?;

    let audit_id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO fact_audit (id, fact_id, operation, old_content, new_content, reason, source_message_id, created_at)
         VALUES (?1, ?2, 'update', ?3, ?4, ?5, ?6, ?7)",
        params![audit_id, fact_id, old_content, new_content, reason, source_message_id, now],
    )?;

    Ok(())
}

/// Soft-delete a fact by setting superseded_at. Logs to fact_audit.
pub fn delete(conn: &Connection, fact_id: &str, reason: Option<&str>) -> anyhow::Result<()> {
    let now = chrono::Utc::now().timestamp();

    let old_content: String = conn.query_row(
        "SELECT content FROM facts WHERE id = ?1",
        [fact_id],
        |r| r.get(0),
    )?;

    conn.execute(
        "UPDATE facts SET superseded_at = ?1 WHERE id = ?2",
        params![now, fact_id],
    )?;

    let audit_id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO fact_audit (id, fact_id, operation, old_content, reason, created_at)
         VALUES (?1, ?2, 'delete', ?3, ?4, ?5)",
        params![audit_id, fact_id, old_content, reason, now],
    )?;

    Ok(())
}

/// Get a fact by ID. Returns None if not found.
pub fn get(conn: &Connection, fact_id: &str) -> anyhow::Result<Option<Fact>> {
    let mut stmt = conn.prepare(
        "SELECT id, agent_id, content, summary, pointer, content_embedding, summary_embedding,
                pointer_embedding, keywords, source_message_id, confidence,
                created_at, updated_at, superseded_at, version
         FROM facts WHERE id = ?1"
    )?;

    let fact = stmt.query_row([fact_id], |r| {
        Ok(Fact {
            id: r.get(0)?,
            agent_id: r.get(1)?,
            content: r.get(2)?,
            summary: r.get(3)?,
            pointer: r.get(4)?,
            content_embedding: r.get(5)?,
            summary_embedding: r.get(6)?,
            pointer_embedding: r.get(7)?,
            keywords: r.get(8)?,
            source_message_id: r.get(9)?,
            confidence: r.get(10)?,
            created_at: r.get(11)?,
            updated_at: r.get(12)?,
            superseded_at: r.get(13)?,
            version: r.get(14)?,
        })
    }).ok();

    Ok(fact)
}

/// Get all active (non-superseded) facts for an agent.
pub fn list_active(conn: &Connection, agent_id: &str) -> anyhow::Result<Vec<Fact>> {
    let mut stmt = conn.prepare(
        "SELECT id, agent_id, content, summary, pointer, content_embedding, summary_embedding,
                pointer_embedding, keywords, source_message_id, confidence,
                created_at, updated_at, superseded_at, version
         FROM facts WHERE agent_id = ?1 AND superseded_at IS NULL
         ORDER BY updated_at DESC"
    )?;

    let facts = stmt.query_map([agent_id], |r| {
        Ok(Fact {
            id: r.get(0)?,
            agent_id: r.get(1)?,
            content: r.get(2)?,
            summary: r.get(3)?,
            pointer: r.get(4)?,
            content_embedding: r.get(5)?,
            summary_embedding: r.get(6)?,
            pointer_embedding: r.get(7)?,
            keywords: r.get(8)?,
            source_message_id: r.get(9)?,
            confidence: r.get(10)?,
            created_at: r.get(11)?,
            updated_at: r.get(12)?,
            superseded_at: r.get(13)?,
            version: r.get(14)?,
        })
    })?.collect::<Result<Vec<_>, _>>()?;

    Ok(facts)
}

/// Get all Level 2 pointers for an agent (for context loading).
pub fn all_pointers(conn: &Connection, agent_id: &str) -> anyhow::Result<Vec<(String, String)>> {
    let mut stmt = conn.prepare(
        "SELECT id, pointer FROM facts WHERE agent_id = ?1 AND superseded_at IS NULL ORDER BY updated_at DESC"
    )?;
    let rows = stmt.query_map([agent_id], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Link two facts.
pub fn link(conn: &Connection, source_id: &str, target_id: &str, relation: Option<&str>, strength: f64) -> anyhow::Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO fact_links (source_id, target_id, relation, strength)
         VALUES (?1, ?2, ?3, ?4)",
        params![source_id, target_id, relation, strength],
    )?;
    Ok(())
}

/// Get links for a fact.
pub fn get_links(conn: &Connection, fact_id: &str) -> anyhow::Result<Vec<FactLink>> {
    let mut stmt = conn.prepare(
        "SELECT source_id, target_id, relation, strength FROM fact_links
         WHERE source_id = ?1 OR target_id = ?1"
    )?;
    let links = stmt.query_map([fact_id], |r| {
        Ok(FactLink {
            source_id: r.get(0)?,
            target_id: r.get(1)?,
            relation: r.get(2)?,
            strength: r.get(3)?,
        })
    })?.collect::<Result<Vec<_>, _>>()?;
    Ok(links)
}

/// Get audit trail for a fact.
pub fn get_audit(conn: &Connection, fact_id: &str) -> anyhow::Result<Vec<FactAuditEntry>> {
    let mut stmt = conn.prepare(
        "SELECT id, fact_id, operation, old_content, new_content, reason, source_message_id, created_at
         FROM fact_audit WHERE fact_id = ?1 ORDER BY created_at ASC"
    )?;
    let entries = stmt.query_map([fact_id], |r| {
        Ok(FactAuditEntry {
            id: r.get(0)?,
            fact_id: r.get(1)?,
            operation: r.get(2)?,
            old_content: r.get(3)?,
            new_content: r.get(4)?,
            reason: r.get(5)?,
            source_message_id: r.get(6)?,
            created_at: r.get(7)?,
        })
    })?.collect::<Result<Vec<_>, _>>()?;
    Ok(entries)
}

/// Bump confidence for a fact (re-extraction validation signal).
pub fn bump_confidence(conn: &Connection, fact_id: &str, amount: f64) -> anyhow::Result<()> {
    conn.execute(
        "UPDATE facts SET confidence = MIN(confidence + ?1, 10.0), updated_at = ?2 WHERE id = ?3",
        params![amount, chrono::Utc::now().timestamp(), fact_id],
    )?;
    Ok(())
}

/// Write or overwrite the FLOAT32 content embedding for a fact.
///
/// Called from the async agent layer after `embed()` returns so that the
/// synchronous store layer never has to touch async code.
pub fn set_content_embedding(conn: &Connection, fact_id: &str, blob: &[u8]) -> anyhow::Result<()> {
    conn.execute(
        "UPDATE facts SET content_embedding = ?1 WHERE id = ?2",
        params![blob, fact_id],
    )?;
    Ok(())
}

/// Return IDs of all active facts that have no content_embedding yet.
pub fn ids_without_embedding(conn: &Connection, agent_id: &str) -> anyhow::Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT id FROM facts WHERE agent_id = ?1 AND superseded_at IS NULL AND content_embedding IS NULL",
    )?;
    let ids = stmt
        .query_map(params![agent_id], |r| r.get(0))?
        .collect::<Result<Vec<String>, _>>()?;
    Ok(ids)
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

    fn sample() -> NewFact {
        NewFact {
            agent_id: "agent-main".into(),
            content: "The ORDERS table uses soft deletes via deleted_at".into(),
            summary: "ORDERS uses soft deletes; filter WHERE deleted_at IS NULL".into(),
            pointer: "ORDERS: soft-delete filter".into(),
            keywords: Some("orders soft-delete deleted_at".into()),
            source_message_id: Some("msg_001".into()),
            confidence: 1.0,
        }
    }

    #[test]
    fn add_and_get() {
        let conn = setup();
        let id = add(&conn, &sample(), Some("extracted from conversation")).unwrap();
        let fact = get(&conn, &id).unwrap().unwrap();

        assert_eq!(fact.content, "The ORDERS table uses soft deletes via deleted_at");
        assert_eq!(fact.summary, "ORDERS uses soft deletes; filter WHERE deleted_at IS NULL");
        assert_eq!(fact.pointer, "ORDERS: soft-delete filter");
        assert_eq!(fact.confidence, 1.0);
        assert_eq!(fact.version, 1);
        assert!(fact.superseded_at.is_none());
    }

    #[test]
    fn add_creates_audit_entry() {
        let conn = setup();
        let id = add(&conn, &sample(), Some("test reason")).unwrap();
        let audit = get_audit(&conn, &id).unwrap();

        assert_eq!(audit.len(), 1);
        assert_eq!(audit[0].operation, "add");
        assert_eq!(audit[0].new_content.as_deref(), Some("The ORDERS table uses soft deletes via deleted_at"));
        assert_eq!(audit[0].reason.as_deref(), Some("test reason"));
        assert!(audit[0].old_content.is_none());
    }

    #[test]
    fn update_changes_content_and_bumps_version() {
        let conn = setup();
        let id = add(&conn, &sample(), None).unwrap();

        update(&conn, &id, "Updated content", "Updated summary", "Updated ptr", Some("refined"), None).unwrap();

        let fact = get(&conn, &id).unwrap().unwrap();
        assert_eq!(fact.content, "Updated content");
        assert_eq!(fact.summary, "Updated summary");
        assert_eq!(fact.pointer, "Updated ptr");
        assert_eq!(fact.version, 2);
    }

    #[test]
    fn update_creates_audit_with_old_and_new() {
        let conn = setup();
        let id = add(&conn, &sample(), None).unwrap();
        update(&conn, &id, "New content", "New summary", "New ptr", Some("refined"), None).unwrap();

        let audit = get_audit(&conn, &id).unwrap();
        assert_eq!(audit.len(), 2);
        assert_eq!(audit[1].operation, "update");
        assert_eq!(audit[1].old_content.as_deref(), Some("The ORDERS table uses soft deletes via deleted_at"));
        assert_eq!(audit[1].new_content.as_deref(), Some("New content"));
    }

    #[test]
    fn delete_sets_superseded_at() {
        let conn = setup();
        let id = add(&conn, &sample(), None).unwrap();
        delete(&conn, &id, Some("contradicted")).unwrap();

        let fact = get(&conn, &id).unwrap().unwrap();
        assert!(fact.superseded_at.is_some());
    }

    #[test]
    fn delete_creates_audit_entry() {
        let conn = setup();
        let id = add(&conn, &sample(), None).unwrap();
        delete(&conn, &id, Some("wrong")).unwrap();

        let audit = get_audit(&conn, &id).unwrap();
        assert_eq!(audit.len(), 2);
        assert_eq!(audit[1].operation, "delete");
        assert!(audit[1].old_content.is_some());
    }

    #[test]
    fn list_active_excludes_superseded() {
        let conn = setup();
        let id1 = add(&conn, &sample(), None).unwrap();
        let _id2 = add(&conn, &NewFact {
            agent_id: "agent-main".into(),
            content: "Second fact".into(),
            summary: "Second".into(),
            pointer: "second".into(),
            keywords: None,
            source_message_id: None,
            confidence: 1.0,
        }, None).unwrap();
        delete(&conn, &id1, None).unwrap();

        let active = list_active(&conn, "agent-main").unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].content, "Second fact");
    }

    #[test]
    fn all_pointers_returns_id_and_pointer() {
        let conn = setup();
        add(&conn, &sample(), None).unwrap();
        let pointers = all_pointers(&conn, "agent-main").unwrap();
        assert_eq!(pointers.len(), 1);
        assert_eq!(pointers[0].1, "ORDERS: soft-delete filter");
    }

    #[test]
    fn link_and_get_links() {
        let conn = setup();
        let id1 = add(&conn, &sample(), None).unwrap();
        let id2 = add(&conn, &NewFact {
            agent_id: "agent-main".into(),
            content: "Second".into(),
            summary: "s".into(),
            pointer: "p".into(),
            keywords: None,
            source_message_id: None,
            confidence: 1.0,
        }, None).unwrap();

        link(&conn, &id1, &id2, Some("relates_to"), 0.9).unwrap();
        let links = get_links(&conn, &id1).unwrap();
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].relation.as_deref(), Some("relates_to"));
        assert_eq!(links[0].strength, 0.9);
    }

    #[test]
    fn bump_confidence_increments() {
        let conn = setup();
        let id = add(&conn, &sample(), None).unwrap();
        bump_confidence(&conn, &id, 0.5).unwrap();

        let fact = get(&conn, &id).unwrap().unwrap();
        assert_eq!(fact.confidence, 1.5);
    }

    #[test]
    fn bump_confidence_caps_at_ten() {
        let conn = setup();
        let id = add(&conn, &sample(), None).unwrap();
        bump_confidence(&conn, &id, 20.0).unwrap();

        let fact = get(&conn, &id).unwrap().unwrap();
        assert_eq!(fact.confidence, 10.0);
    }

    #[test]
    fn get_nonexistent_returns_none() {
        let conn = setup();
        assert!(get(&conn, "nope").unwrap().is_none());
    }
}
