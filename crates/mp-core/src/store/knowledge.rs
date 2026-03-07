use rusqlite::{Connection, params};
use uuid::Uuid;

const CHUNK_MAX_CHARS: usize = 2000;

#[derive(Debug, Clone)]
pub struct Document {
    pub id: String,
    pub path: Option<String>,
    pub title: Option<String>,
    pub content_hash: String,
    pub metadata: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone)]
pub struct Chunk {
    pub id: String,
    pub document_id: String,
    pub content: String,
    pub summary: Option<String>,
    pub position: i64,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
pub struct Skill {
    pub id: String,
    pub name: String,
    pub description: String,
    pub content: String,
    pub tool_id: Option<String>,
    pub usage_count: i64,
    pub success_rate: Option<f64>,
    pub promoted: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Ingest a document: store metadata and chunk the content.
/// Returns (document_id, chunks_created).
pub fn ingest(
    conn: &Connection,
    path: Option<&str>,
    title: Option<&str>,
    content: &str,
    metadata: Option<&str>,
) -> anyhow::Result<(String, usize)> {
    let now = chrono::Utc::now().timestamp();
    let doc_id = Uuid::new_v4().to_string();
    let hash = simple_hash(content);

    conn.execute(
        "INSERT INTO documents (id, path, title, content_hash, metadata, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![doc_id, path, title, hash, metadata, now, now],
    )?;

    let chunks = chunk_markdown(content);
    for (i, chunk_text) in chunks.iter().enumerate() {
        let chunk_id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO chunks (id, document_id, content, position, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![chunk_id, doc_id, chunk_text, i as i64, now],
        )?;
    }

    Ok((doc_id, chunks.len()))
}

/// Get a document by ID.
pub fn get_document(conn: &Connection, doc_id: &str) -> anyhow::Result<Option<Document>> {
    let doc = conn.query_row(
        "SELECT id, path, title, content_hash, metadata, created_at, updated_at
         FROM documents WHERE id = ?1",
        [doc_id],
        |r| Ok(Document {
            id: r.get(0)?,
            path: r.get(1)?,
            title: r.get(2)?,
            content_hash: r.get(3)?,
            metadata: r.get(4)?,
            created_at: r.get(5)?,
            updated_at: r.get(6)?,
        }),
    ).ok();
    Ok(doc)
}

/// List all documents.
pub fn list_documents(conn: &Connection) -> anyhow::Result<Vec<Document>> {
    let mut stmt = conn.prepare(
        "SELECT id, path, title, content_hash, metadata, created_at, updated_at
         FROM documents ORDER BY created_at DESC"
    )?;
    let docs = stmt.query_map([], |r| {
        Ok(Document {
            id: r.get(0)?,
            path: r.get(1)?,
            title: r.get(2)?,
            content_hash: r.get(3)?,
            metadata: r.get(4)?,
            created_at: r.get(5)?,
            updated_at: r.get(6)?,
        })
    })?.collect::<Result<Vec<_>, _>>()?;
    Ok(docs)
}

/// Get chunks for a document, ordered by position.
pub fn get_chunks(conn: &Connection, doc_id: &str) -> anyhow::Result<Vec<Chunk>> {
    let mut stmt = conn.prepare(
        "SELECT id, document_id, content, summary, position, created_at
         FROM chunks WHERE document_id = ?1 ORDER BY position ASC"
    )?;
    let chunks = stmt.query_map([doc_id], |r| {
        Ok(Chunk {
            id: r.get(0)?,
            document_id: r.get(1)?,
            content: r.get(2)?,
            summary: r.get(3)?,
            position: r.get(4)?,
            created_at: r.get(5)?,
        })
    })?.collect::<Result<Vec<_>, _>>()?;
    Ok(chunks)
}

/// Add a knowledge graph edge.
pub fn add_edge(conn: &Connection, source: &str, target: &str, relation: &str) -> anyhow::Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO edges (source_id, target_id, relation) VALUES (?1, ?2, ?3)",
        params![source, target, relation],
    )?;
    Ok(())
}

// -- Skills --

/// Add a skill.
pub fn add_skill(
    conn: &Connection,
    name: &str,
    description: &str,
    content: &str,
    tool_id: Option<&str>,
) -> anyhow::Result<String> {
    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp();
    conn.execute(
        "INSERT INTO skills (id, name, description, content, tool_id, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![id, name, description, content, tool_id, now, now],
    )?;
    Ok(id)
}

/// Get a skill by ID.
pub fn get_skill(conn: &Connection, skill_id: &str) -> anyhow::Result<Option<Skill>> {
    let skill = conn.query_row(
        "SELECT id, name, description, content, tool_id, usage_count, success_rate, promoted, created_at, updated_at
         FROM skills WHERE id = ?1",
        [skill_id],
        |r| Ok(Skill {
            id: r.get(0)?,
            name: r.get(1)?,
            description: r.get(2)?,
            content: r.get(3)?,
            tool_id: r.get(4)?,
            usage_count: r.get(5)?,
            success_rate: r.get(6)?,
            promoted: r.get::<_, i64>(7)? != 0,
            created_at: r.get(8)?,
            updated_at: r.get(9)?,
        }),
    ).ok();
    Ok(skill)
}

/// Record a skill invocation and its success/failure.
pub fn record_skill_usage(conn: &Connection, skill_id: &str, success: bool) -> anyhow::Result<()> {
    let now = chrono::Utc::now().timestamp();

    let (count, rate): (i64, Option<f64>) = conn.query_row(
        "SELECT usage_count, success_rate FROM skills WHERE id = ?1",
        [skill_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;

    let old_rate = rate.unwrap_or(0.0);
    let new_count = count + 1;
    let successes = (old_rate * count as f64) + if success { 1.0 } else { 0.0 };
    let new_rate = successes / new_count as f64;

    conn.execute(
        "UPDATE skills SET usage_count = ?1, success_rate = ?2, updated_at = ?3 WHERE id = ?4",
        params![new_count, new_rate, now, skill_id],
    )?;
    Ok(())
}

/// Promote a skill.
pub fn promote_skill(conn: &Connection, skill_id: &str) -> anyhow::Result<()> {
    conn.execute("UPDATE skills SET promoted = 1 WHERE id = ?1", [skill_id])?;
    Ok(())
}

// -- Markdown-aware chunking --

/// Split markdown into chunks of roughly CHUNK_MAX_CHARS, breaking at heading boundaries.
pub fn chunk_markdown(content: &str) -> Vec<String> {
    let mut chunks: Vec<String> = Vec::new();
    let mut current = String::new();

    for line in content.lines() {
        let is_heading = line.starts_with('#');
        let would_overflow = current.len() + line.len() + 1 > CHUNK_MAX_CHARS;

        if (is_heading || would_overflow) && !current.trim().is_empty() {
            chunks.push(current.trim().to_string());
            current = String::new();
        }

        current.push_str(line);
        current.push('\n');
    }

    if !current.trim().is_empty() {
        chunks.push(current.trim().to_string());
    }

    chunks
}

fn simple_hash(content: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
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
    fn ingest_creates_document_and_chunks() {
        let conn = setup();
        let content = "# Title\n\nSome content here.\n\n# Section 2\n\nMore content.";
        let (doc_id, count) = ingest(&conn, Some("/docs/test.md"), Some("Test Doc"), content, None).unwrap();

        let doc = get_document(&conn, &doc_id).unwrap().unwrap();
        assert_eq!(doc.title.as_deref(), Some("Test Doc"));
        assert_eq!(doc.path.as_deref(), Some("/docs/test.md"));

        let chunks = get_chunks(&conn, &doc_id).unwrap();
        assert_eq!(chunks.len(), count);
        assert!(count >= 2, "should split on headings");
        assert_eq!(chunks[0].position, 0);
        assert_eq!(chunks[1].position, 1);
    }

    #[test]
    fn chunk_markdown_splits_on_headings() {
        let md = "# A\nfoo\n# B\nbar\n# C\nbaz";
        let chunks = chunk_markdown(md);
        assert_eq!(chunks.len(), 3);
        assert!(chunks[0].starts_with("# A"));
        assert!(chunks[1].starts_with("# B"));
        assert!(chunks[2].starts_with("# C"));
    }

    #[test]
    fn chunk_markdown_splits_large_blocks() {
        let big = "x".repeat(CHUNK_MAX_CHARS + 500);
        let md = format!("# Heading\n{big}");
        let chunks = chunk_markdown(&md);
        assert!(chunks.len() >= 2, "should split oversized content");
    }

    #[test]
    fn chunk_markdown_handles_empty() {
        assert!(chunk_markdown("").is_empty());
        assert!(chunk_markdown("   \n  \n  ").is_empty());
    }

    #[test]
    fn list_documents_returns_all() {
        let conn = setup();
        ingest(&conn, None, Some("A"), "content a", None).unwrap();
        ingest(&conn, None, Some("B"), "content b", None).unwrap();
        let docs = list_documents(&conn).unwrap();
        assert_eq!(docs.len(), 2);
    }

    #[test]
    fn content_hash_changes_with_content() {
        let conn = setup();
        let (id1, _) = ingest(&conn, None, None, "version 1", None).unwrap();
        let (id2, _) = ingest(&conn, None, None, "version 2", None).unwrap();
        let d1 = get_document(&conn, &id1).unwrap().unwrap();
        let d2 = get_document(&conn, &id2).unwrap().unwrap();
        assert_ne!(d1.content_hash, d2.content_hash);
    }

    #[test]
    fn add_and_get_skill() {
        let conn = setup();
        let id = add_skill(&conn, "sql-query", "Run SQL queries", "# SQL Skill\n...", None).unwrap();
        let skill = get_skill(&conn, &id).unwrap().unwrap();
        assert_eq!(skill.name, "sql-query");
        assert_eq!(skill.usage_count, 0);
        assert!(!skill.promoted);
    }

    #[test]
    fn record_skill_usage_tracks_rate() {
        let conn = setup();
        let id = add_skill(&conn, "test", "test", "body", None).unwrap();
        record_skill_usage(&conn, &id, true).unwrap();
        record_skill_usage(&conn, &id, true).unwrap();
        record_skill_usage(&conn, &id, false).unwrap();

        let skill = get_skill(&conn, &id).unwrap().unwrap();
        assert_eq!(skill.usage_count, 3);
        let rate = skill.success_rate.unwrap();
        assert!((rate - 2.0 / 3.0).abs() < 0.01);
    }

    #[test]
    fn promote_skill_sets_flag() {
        let conn = setup();
        let id = add_skill(&conn, "test", "test", "body", None).unwrap();
        promote_skill(&conn, &id).unwrap();
        let skill = get_skill(&conn, &id).unwrap().unwrap();
        assert!(skill.promoted);
    }

    #[test]
    fn add_edge_and_no_duplicate() {
        let conn = setup();
        add_edge(&conn, "a", "b", "references").unwrap();
        add_edge(&conn, "a", "b", "references").unwrap(); // should not fail (OR IGNORE)
    }
}
