use rusqlite::{Connection, params};
use uuid::Uuid;

const CHUNK_MAX_CHARS: usize = 2000;
const DOC_MAX_CHARS: usize = 120_000;

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
    let normalized_content = normalize_ingest_content(content);
    if normalized_content.trim().is_empty() {
        anyhow::bail!("ingest content is empty after normalization");
    }

    let now = chrono::Utc::now().timestamp();
    let doc_id = Uuid::new_v4().to_string();
    let hash = simple_hash(&normalized_content);

    conn.execute(
        "INSERT INTO documents (id, path, title, content_hash, metadata, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![doc_id, path, title, hash, metadata, now, now],
    )?;

    let chunks = chunk_markdown(&normalized_content);
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

/// Normalize content for knowledge ingestion with balanced cleanup:
/// - strip HTML when detected,
/// - remove obvious boilerplate lines,
/// - collapse whitespace while keeping paragraph breaks,
/// - enforce a document-size budget before chunking.
pub fn normalize_ingest_content(content: &str) -> String {
    let base = if is_probably_html_document(content) {
        strip_html_tags(content)
    } else {
        content.to_string()
    };
    let collapsed = collapse_whitespace_preserve_paragraphs(&base);
    let pruned = prune_boilerplate_lines(&collapsed);
    apply_content_budget(&pruned, DOC_MAX_CHARS)
}

pub fn extract_html_title(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let start = lower.find("<title")?.checked_add(6)?;
    let after_tag = lower[start..].find('>')?.checked_add(1)?;
    let content_start = start + after_tag;
    let end = lower[content_start..].find("</title")?;
    let title = html[content_start..content_start + end].trim().to_string();
    if title.is_empty() { None } else { Some(title) }
}

pub fn is_probably_html_document(content: &str) -> bool {
    let lower = content.to_ascii_lowercase();
    lower.contains("<!doctype html")
        || lower.contains("<html")
        || lower.contains("<body")
        || lower.contains("<article")
        || lower.contains("<main")
        || (lower.contains("<p") && lower.contains("</p>"))
}

pub fn strip_html_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len() / 2);
    let mut in_tag = false;
    let mut skip_depth: usize = 0;
    let mut tag_buf = String::new();
    let mut chars = html.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '<' {
            in_tag = true;
            tag_buf.clear();
            continue;
        }
        if in_tag {
            if ch == '>' {
                in_tag = false;
                let raw = tag_buf.trim().to_ascii_lowercase();
                let tag_name = raw
                    .trim_start_matches('/')
                    .split_whitespace()
                    .next()
                    .unwrap_or("");
                let is_closing = raw.starts_with('/');
                let skip_tag = matches!(
                    tag_name,
                    "script"
                        | "style"
                        | "noscript"
                        | "svg"
                        | "nav"
                        | "footer"
                        | "header"
                        | "aside"
                        | "form"
                );
                if skip_tag {
                    if is_closing {
                        skip_depth = skip_depth.saturating_sub(1);
                    } else {
                        skip_depth = skip_depth.saturating_add(1);
                    }
                }
                if matches!(
                    tag_name,
                    "br" | "p" | "div" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "li" | "tr"
                        | "blockquote" | "section" | "article" | "main"
                ) {
                    out.push('\n');
                }
            } else {
                tag_buf.push(ch);
            }
            continue;
        }
        if skip_depth > 0 {
            continue;
        }
        if ch == '&' {
            let mut entity = String::new();
            while let Some(&next) = chars.peek() {
                if next == ';' || entity.len() > 8 {
                    chars.next();
                    break;
                }
                entity.push(next);
                chars.next();
            }
            match entity.as_str() {
                "amp" => out.push('&'),
                "lt" => out.push('<'),
                "gt" => out.push('>'),
                "quot" => out.push('"'),
                "apos" => out.push('\''),
                "nbsp" => out.push(' '),
                _ => out.push(' '),
            }
            continue;
        }
        out.push(ch);
    }

    collapse_whitespace_preserve_paragraphs(&out)
}

fn collapse_whitespace_preserve_paragraphs(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut previous_blank = false;
    for line in s.lines() {
        let trimmed = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if trimmed.is_empty() {
            if !previous_blank && !result.is_empty() {
                result.push('\n');
                previous_blank = true;
            }
        } else {
            result.push_str(&trimmed);
            result.push('\n');
            previous_blank = false;
        }
    }
    result.trim().to_string()
}

fn prune_boilerplate_lines(content: &str) -> String {
    let mut out = String::with_capacity(content.len());
    let mut previous_blank = false;
    for line in content.lines() {
        if is_boilerplate_line(line) {
            continue;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if !previous_blank && !out.is_empty() {
                out.push('\n');
                previous_blank = true;
            }
            continue;
        }
        out.push_str(trimmed);
        out.push('\n');
        previous_blank = false;
    }
    out.trim().to_string()
}

fn is_boilerplate_line(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }
    let lower = trimmed.to_ascii_lowercase();
    let is_short = trimmed.chars().count() <= 120;
    is_short
        && (lower.contains("cookie policy")
            || lower.contains("privacy policy")
            || lower.contains("terms of service")
            || lower.contains("all rights reserved")
            || lower.contains("sign in")
            || lower.contains("log in")
            || lower.contains("subscribe")
            || lower.contains("back to top")
            || lower == "menu"
            || lower == "search")
}

fn apply_content_budget(content: &str, max_chars: usize) -> String {
    if content.chars().count() <= max_chars {
        return content.to_string();
    }
    let mut hard_cutoff = content.len();
    for (idx, _) in content.char_indices().take(max_chars) {
        hard_cutoff = idx;
    }
    let candidate = &content[..hard_cutoff];
    let preferred = candidate
        .rfind("\n\n")
        .or_else(|| candidate.rfind('\n'))
        .unwrap_or(hard_cutoff);
    let mut truncated = content[..preferred].trim_end().to_string();
    truncated.push_str("\n\n[Content truncated during ingest due to size budget.]");
    truncated
}

/// Get a document by ID.
pub fn get_document(conn: &Connection, doc_id: &str) -> anyhow::Result<Option<Document>> {
    let doc = conn
        .query_row(
            "SELECT id, path, title, content_hash, metadata, created_at, updated_at
         FROM documents WHERE id = ?1",
            [doc_id],
            |r| {
                Ok(Document {
                    id: r.get(0)?,
                    path: r.get(1)?,
                    title: r.get(2)?,
                    content_hash: r.get(3)?,
                    metadata: r.get(4)?,
                    created_at: r.get(5)?,
                    updated_at: r.get(6)?,
                })
            },
        )
        .ok();
    Ok(doc)
}

/// List all documents.
pub fn list_documents(conn: &Connection) -> anyhow::Result<Vec<Document>> {
    let mut stmt = conn.prepare(
        "SELECT id, path, title, content_hash, metadata, created_at, updated_at
         FROM documents ORDER BY created_at DESC",
    )?;
    let docs = stmt
        .query_map([], |r| {
            Ok(Document {
                id: r.get(0)?,
                path: r.get(1)?,
                title: r.get(2)?,
                content_hash: r.get(3)?,
                metadata: r.get(4)?,
                created_at: r.get(5)?,
                updated_at: r.get(6)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(docs)
}

/// Get chunks for a document, ordered by position.
pub fn get_chunks(conn: &Connection, doc_id: &str) -> anyhow::Result<Vec<Chunk>> {
    let mut stmt = conn.prepare(
        "SELECT id, document_id, content, summary, position, created_at
         FROM chunks WHERE document_id = ?1 ORDER BY position ASC",
    )?;
    let chunks = stmt
        .query_map([doc_id], |r| {
            Ok(Chunk {
                id: r.get(0)?,
                document_id: r.get(1)?,
                content: r.get(2)?,
                summary: r.get(3)?,
                position: r.get(4)?,
                created_at: r.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(chunks)
}

/// Add a knowledge graph edge.
pub fn add_edge(
    conn: &Connection,
    source: &str,
    target: &str,
    relation: &str,
) -> anyhow::Result<()> {
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

/// Write or overwrite the FLOAT32 content embedding for a knowledge chunk.
pub fn set_chunk_embedding(conn: &Connection, chunk_id: &str, blob: &[u8]) -> anyhow::Result<()> {
    set_chunk_embedding_with_meta(conn, chunk_id, blob, None, None)
}

/// Write or overwrite the FLOAT32 content embedding for a knowledge chunk and
/// persist embedding provenance metadata.
pub fn set_chunk_embedding_with_meta(
    conn: &Connection,
    chunk_id: &str,
    blob: &[u8],
    embedding_model: Option<&str>,
    embedding_content_hash: Option<&str>,
) -> anyhow::Result<()> {
    conn.execute(
        "UPDATE chunks
         SET content_embedding = ?1,
             embedding_model = ?2,
             embedding_content_hash = ?3
         WHERE id = ?4",
        rusqlite::params![blob, embedding_model, embedding_content_hash, chunk_id],
    )?;
    Ok(())
}

/// Return (id, content) for all chunks that have no content_embedding yet.
pub fn chunks_without_embedding(conn: &Connection) -> anyhow::Result<Vec<(String, String)>> {
    let mut stmt =
        conn.prepare("SELECT id, content FROM chunks WHERE content_embedding IS NULL")?;
    let rows = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

// -- Markdown-aware chunking --

/// Split markdown into chunks of roughly CHUNK_MAX_CHARS, breaking at heading boundaries.
pub fn chunk_markdown(content: &str) -> Vec<String> {
    let mut chunks: Vec<String> = Vec::new();
    let mut current = String::new();

    for raw_line in content.lines() {
        for line in split_long_line(raw_line, CHUNK_MAX_CHARS) {
            let is_heading = line.starts_with('#');
            let would_overflow = current.len() + line.len() + 1 > CHUNK_MAX_CHARS;

            if (is_heading || would_overflow) && !current.trim().is_empty() {
                chunks.push(current.trim().to_string());
                current = String::new();
            }

            current.push_str(line);
            current.push('\n');
        }
    }

    if !current.trim().is_empty() {
        chunks.push(current.trim().to_string());
    }

    chunks
}

fn split_long_line(line: &str, max_chars: usize) -> Vec<&str> {
    if line.chars().count() <= max_chars {
        return vec![line];
    }

    let mut parts = Vec::new();
    let mut start_char = 0usize;
    let byte_indices: Vec<usize> = line
        .char_indices()
        .map(|(idx, _)| idx)
        .chain(std::iter::once(line.len()))
        .collect();

    while start_char < byte_indices.len() - 1 {
        let end_char = (start_char + max_chars).min(byte_indices.len() - 1);
        let start_byte = byte_indices[start_char];
        let end_byte = byte_indices[end_char];
        parts.push(&line[start_byte..end_byte]);
        start_char = end_char;
    }
    parts
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
        let (doc_id, count) = ingest(
            &conn,
            Some("/docs/test.md"),
            Some("Test Doc"),
            content,
            None,
        )
        .unwrap();

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
    fn normalize_ingest_content_strips_html_and_boilerplate() {
        let html = r#"
            <html><head><title>Demo</title></head>
            <body>
                <nav>Menu</nav>
                <article><h1>Hello</h1><p>Useful body text.</p></article>
                <footer>Privacy Policy</footer>
            </body></html>
        "#;
        let normalized = normalize_ingest_content(html);
        assert!(normalized.contains("Hello"));
        assert!(normalized.contains("Useful body text."));
        assert!(!normalized.to_ascii_lowercase().contains("privacy policy"));
        assert!(!normalized.to_ascii_lowercase().contains("menu"));
    }

    #[test]
    fn normalize_ingest_content_applies_budget() {
        let long = "A paragraph.\n\n".repeat(20_000);
        let normalized = normalize_ingest_content(&long);
        assert!(normalized.chars().count() <= DOC_MAX_CHARS + 64);
        assert!(normalized.contains("Content truncated during ingest"));
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
        let id = add_skill(
            &conn,
            "sql-query",
            "Run SQL queries",
            "# SQL Skill\n...",
            None,
        )
        .unwrap();
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
