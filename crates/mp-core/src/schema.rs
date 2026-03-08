/// Schema definitions and migrations for Moneypenny databases.
///
/// Two database types:
/// - **Agent DB**: one per agent, contains all memory stores + policies + jobs
/// - **Metadata DB**: one per gateway, contains agent registry + routing

use rusqlite::Connection;

const AGENT_SCHEMA_VERSION: i64 = 2;
const METADATA_SCHEMA_VERSION: i64 = 1;

pub fn init_agent_db(conn: &Connection) -> anyhow::Result<()> {
    let current = get_schema_version(conn);

    if current < 1 {
        conn.execute_batch(AGENT_SCHEMA_V1)?;
        set_schema_version(conn, 1)?;
    }

    if current < 2 {
        conn.execute_batch(AGENT_SCHEMA_V2)?;
        set_schema_version(conn, 2)?;
    }

    Ok(())
}

/// Initialize CRDT sync tracking on all default sync tables.
///
/// Must be called after `init_agent_db` and `mp_ext::init_all_extensions` so
/// the cloudsync functions are available. Idempotent — tables that already
/// have tracking enabled are silently skipped.
pub fn init_sync_tables(conn: &Connection) -> anyhow::Result<()> {
    crate::sync::init_sync_tables(conn, crate::sync::DEFAULT_SYNC_TABLES)?;
    Ok(())
}

/// Register sqlite-vector indexes for all embedding columns.
///
/// Must be called after both `init_agent_db` and `mp_ext::init_all_extensions`
/// so the sqlite-vector functions are available. Safe to call on every connection
/// open — `vector_init` is idempotent for the same table/column/dimension.
///
/// `dims` should match the configured embedding model (e.g. 768 for nomic-embed-text-v1.5).
pub fn init_vector_indexes(conn: &Connection, dims: usize) -> anyhow::Result<()> {
    let opts = format!("type=FLOAT32,dimension={dims},distance=COSINE");
    for (table, col) in &[
        ("facts",  "content_embedding"),
        ("chunks", "content_embedding"),
    ] {
        conn.execute(
            "SELECT vector_init(?1, ?2, ?3)",
            rusqlite::params![table, col, opts],
        )?;
    }
    Ok(())
}

pub fn init_metadata_db(conn: &Connection) -> anyhow::Result<()> {
    let current = get_schema_version(conn);
    if current >= METADATA_SCHEMA_VERSION {
        return Ok(());
    }

    conn.execute_batch(METADATA_SCHEMA_V1)?;

    set_schema_version(conn, METADATA_SCHEMA_VERSION)?;
    Ok(())
}

fn get_schema_version(conn: &Connection) -> i64 {
    conn.query_row(
        "SELECT version FROM schema_version ORDER BY version DESC LIMIT 1",
        [],
        |r| r.get(0),
    )
    .unwrap_or(0)
}

fn set_schema_version(conn: &Connection, version: i64) -> anyhow::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_version (
            version INTEGER NOT NULL,
            applied_at INTEGER NOT NULL
        )",
    )?;
    conn.execute(
        "INSERT INTO schema_version (version, applied_at) VALUES (?1, strftime('%s', 'now'))",
        [version],
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Agent database — v1
// ---------------------------------------------------------------------------

const AGENT_SCHEMA_V1: &str = "
-- Facts store: distilled, curated knowledge
CREATE TABLE IF NOT EXISTS facts (
    id                  TEXT NOT NULL PRIMARY KEY,
    agent_id            TEXT NOT NULL DEFAULT '',
    content             TEXT NOT NULL DEFAULT '',
    summary             TEXT NOT NULL DEFAULT '',
    pointer             TEXT NOT NULL DEFAULT '',
    content_embedding   BLOB,
    summary_embedding   BLOB,
    pointer_embedding   BLOB,
    keywords            TEXT,
    source_message_id   TEXT,
    confidence          REAL DEFAULT 1.0,
    created_at          INTEGER NOT NULL DEFAULT 0,
    updated_at          INTEGER NOT NULL DEFAULT 0,
    superseded_at       INTEGER,
    version             INTEGER DEFAULT 1
);

CREATE TABLE IF NOT EXISTS fact_links (
    source_id   TEXT NOT NULL,
    target_id   TEXT NOT NULL,
    relation    TEXT,
    strength    REAL DEFAULT 1.0,
    PRIMARY KEY (source_id, target_id)
);

CREATE TABLE IF NOT EXISTS fact_audit (
    id                  TEXT PRIMARY KEY,
    fact_id             TEXT NOT NULL,
    operation           TEXT NOT NULL,
    old_content         TEXT,
    new_content         TEXT,
    reason              TEXT,
    source_message_id   TEXT,
    created_at          INTEGER NOT NULL
);

-- Log store: sessions, messages, tool calls
CREATE TABLE IF NOT EXISTS sessions (
    id                  TEXT PRIMARY KEY,
    agent_id            TEXT NOT NULL,
    channel             TEXT,
    started_at          INTEGER NOT NULL,
    ended_at            INTEGER,
    summary             TEXT,
    summary_embedding   BLOB
);

CREATE TABLE IF NOT EXISTS messages (
    id                  TEXT PRIMARY KEY,
    session_id          TEXT NOT NULL,
    role                TEXT NOT NULL,
    content             TEXT NOT NULL,
    content_embedding   BLOB,
    created_at          INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS tool_calls (
    id                  TEXT PRIMARY KEY,
    message_id          TEXT NOT NULL,
    session_id          TEXT NOT NULL,
    tool_name           TEXT NOT NULL,
    arguments           TEXT,
    result              TEXT,
    status              TEXT,
    policy_decision     TEXT,
    duration_ms         INTEGER,
    created_at          INTEGER NOT NULL
);

-- Knowledge store: documents, chunks, edges, skills
CREATE TABLE IF NOT EXISTS documents (
    id                  TEXT PRIMARY KEY,
    path                TEXT,
    title               TEXT,
    content_hash        TEXT NOT NULL,
    metadata            TEXT,
    created_at          INTEGER NOT NULL,
    updated_at          INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS chunks (
    id                  TEXT PRIMARY KEY,
    document_id         TEXT NOT NULL,
    content             TEXT NOT NULL,
    summary             TEXT,
    content_embedding   BLOB,
    summary_embedding   BLOB,
    position            INTEGER,
    created_at          INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS edges (
    source_id   TEXT NOT NULL,
    target_id   TEXT NOT NULL,
    relation    TEXT NOT NULL,
    PRIMARY KEY (source_id, target_id, relation)
);

CREATE TABLE IF NOT EXISTS skills (
    id                  TEXT NOT NULL PRIMARY KEY,
    name                TEXT NOT NULL DEFAULT '',
    description         TEXT NOT NULL DEFAULT '',
    content             TEXT NOT NULL DEFAULT '',
    tool_id             TEXT,
    content_embedding   BLOB,
    usage_count         INTEGER DEFAULT 0,
    success_rate        REAL,
    promoted            INTEGER DEFAULT 0,
    created_at          INTEGER NOT NULL DEFAULT 0,
    updated_at          INTEGER NOT NULL DEFAULT 0
);

-- Scratch: session-scoped working memory
CREATE TABLE IF NOT EXISTS scratch (
    id                  TEXT PRIMARY KEY,
    session_id          TEXT NOT NULL,
    key                 TEXT NOT NULL,
    content             TEXT NOT NULL,
    created_at          INTEGER NOT NULL,
    updated_at          INTEGER NOT NULL
);

-- Policies
CREATE TABLE IF NOT EXISTS policies (
    id                  TEXT NOT NULL PRIMARY KEY,
    name                TEXT NOT NULL DEFAULT '',
    priority            INTEGER NOT NULL DEFAULT 0,
    phase               TEXT NOT NULL DEFAULT 'pre',
    effect              TEXT NOT NULL DEFAULT 'deny',
    actor_pattern       TEXT,
    action_pattern      TEXT,
    resource_pattern    TEXT,
    sql_pattern         TEXT,
    argument_pattern    TEXT,
    agent_id            TEXT,
    channel_pattern     TEXT,
    schedule            TEXT,
    message             TEXT,
    enabled             INTEGER DEFAULT 1,
    created_at          INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS policy_audit (
    id                  TEXT PRIMARY KEY,
    policy_id           TEXT,
    actor               TEXT NOT NULL,
    action              TEXT NOT NULL,
    resource            TEXT NOT NULL,
    effect              TEXT NOT NULL,
    reason              TEXT,
    session_id          TEXT,
    created_at          INTEGER NOT NULL
);

-- Jobs
CREATE TABLE IF NOT EXISTS jobs (
    id                  TEXT PRIMARY KEY,
    agent_id            TEXT NOT NULL,
    name                TEXT NOT NULL,
    description         TEXT,
    schedule            TEXT NOT NULL,
    next_run_at         INTEGER NOT NULL,
    last_run_at         INTEGER,
    timezone            TEXT DEFAULT 'UTC',
    job_type            TEXT NOT NULL,
    payload             TEXT NOT NULL,
    max_retries         INTEGER DEFAULT 0,
    retry_delay_ms      INTEGER DEFAULT 5000,
    timeout_ms          INTEGER DEFAULT 30000,
    overlap_policy      TEXT DEFAULT 'skip',
    status              TEXT DEFAULT 'active',
    enabled             INTEGER DEFAULT 1,
    created_at          INTEGER NOT NULL,
    updated_at          INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS job_runs (
    id                  TEXT PRIMARY KEY,
    job_id              TEXT NOT NULL,
    agent_id            TEXT NOT NULL,
    started_at          INTEGER NOT NULL,
    ended_at            INTEGER,
    status              TEXT NOT NULL,
    result              TEXT,
    policy_decision     TEXT,
    retry_count         INTEGER DEFAULT 0,
    created_at          INTEGER NOT NULL
);
";

// ---------------------------------------------------------------------------
// Agent database — v2: behavioral policy rules
// ---------------------------------------------------------------------------

const AGENT_SCHEMA_V2: &str = "
ALTER TABLE policies ADD COLUMN rule_type TEXT;
ALTER TABLE policies ADD COLUMN rule_config TEXT;
";

// ---------------------------------------------------------------------------
// Metadata database — v1
// ---------------------------------------------------------------------------

const METADATA_SCHEMA_V1: &str = "
CREATE TABLE IF NOT EXISTS agents (
    id                  TEXT PRIMARY KEY,
    name                TEXT NOT NULL,
    persona             TEXT,
    trust_level         TEXT DEFAULT 'standard',
    llm_provider        TEXT DEFAULT 'local',
    llm_model           TEXT,
    db_path             TEXT NOT NULL,
    sync_enabled        INTEGER DEFAULT 1,
    created_at          INTEGER
);
";

#[cfg(test)]
mod tests {
    use crate::db;

    fn setup_agent_db() -> rusqlite::Connection {
        let conn = db::open_memory().expect("open in-memory db");
        super::init_agent_db(&conn).expect("init agent schema");
        conn
    }

    fn setup_metadata_db() -> rusqlite::Connection {
        let conn = db::open_memory().expect("open in-memory db");
        super::init_metadata_db(&conn).expect("init metadata schema");
        conn
    }

    /// Helper: assert a table exists and return its column names.
    fn table_columns(conn: &rusqlite::Connection, table: &str) -> Vec<String> {
        let mut stmt = conn
            .prepare(&format!("PRAGMA table_info({table})"))
            .unwrap();
        let cols: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert!(!cols.is_empty(), "table '{table}' should exist");
        cols
    }

    /// Helper: get column info as (name, type, notnull, default_value, pk).
    fn column_info(
        conn: &rusqlite::Connection,
        table: &str,
    ) -> Vec<(String, String, bool, Option<String>, bool)> {
        let mut stmt = conn
            .prepare(&format!("PRAGMA table_info({table})"))
            .unwrap();
        stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(1)?,  // name
                row.get::<_, String>(2)?,  // type
                row.get::<_, bool>(3)?,    // notnull
                row.get::<_, Option<String>>(4)?, // dflt_value
                row.get::<_, bool>(5)?,    // pk
            ))
        })
        .unwrap()
        .map(|r| r.unwrap())
        .collect()
    }

    fn has_column(conn: &rusqlite::Connection, table: &str, column: &str) -> bool {
        table_columns(conn, table).contains(&column.to_string())
    }

    // ========================================================================
    // FACTS STORE
    // ========================================================================

    #[test]
    fn facts_table_has_all_columns() {
        let conn = setup_agent_db();
        let expected = vec![
            "id", "agent_id", "content", "summary", "pointer",
            "content_embedding", "summary_embedding", "pointer_embedding",
            "keywords", "source_message_id", "confidence",
            "created_at", "updated_at", "superseded_at", "version",
        ];
        for col in &expected {
            assert!(has_column(&conn, "facts", col), "facts missing column: {col}");
        }
    }

    #[test]
    fn facts_id_is_primary_key() {
        let conn = setup_agent_db();
        let info = column_info(&conn, "facts");
        let id_col = info.iter().find(|c| c.0 == "id").unwrap();
        assert!(id_col.4, "facts.id should be primary key");
    }

    #[test]
    fn facts_not_null_constraints() {
        let conn = setup_agent_db();
        let info = column_info(&conn, "facts");
        let required = ["agent_id", "content", "summary", "pointer", "created_at", "updated_at"];
        for name in &required {
            let col = info.iter().find(|c| c.0 == *name).unwrap();
            assert!(col.2, "facts.{name} should be NOT NULL");
        }
    }

    #[test]
    fn facts_defaults() {
        let conn = setup_agent_db();
        let info = column_info(&conn, "facts");
        let confidence = info.iter().find(|c| c.0 == "confidence").unwrap();
        assert_eq!(confidence.3.as_deref(), Some("1.0"), "confidence default should be 1.0");
        let version = info.iter().find(|c| c.0 == "version").unwrap();
        assert_eq!(version.3.as_deref(), Some("1"), "version default should be 1");
    }

    #[test]
    fn facts_insert_and_read() {
        let conn = setup_agent_db();
        conn.execute(
            "INSERT INTO facts (id, agent_id, content, summary, pointer, created_at, updated_at)
             VALUES ('f1', 'agent-main', 'full content', 'short summary', 'pointer label', 1000, 1000)",
            [],
        ).expect("insert fact");

        let (content, confidence, version): (String, f64, i64) = conn
            .query_row(
                "SELECT content, confidence, version FROM facts WHERE id = 'f1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("read fact");

        assert_eq!(content, "full content");
        assert_eq!(confidence, 1.0);
        assert_eq!(version, 1);
    }

    // ========================================================================
    // FACT_LINKS
    // ========================================================================

    #[test]
    fn fact_links_table_has_all_columns() {
        let conn = setup_agent_db();
        for col in &["source_id", "target_id", "relation", "strength"] {
            assert!(has_column(&conn, "fact_links", col), "fact_links missing column: {col}");
        }
    }

    #[test]
    fn fact_links_composite_primary_key() {
        let conn = setup_agent_db();
        // Insert a fact first for FK if we add them, but spec doesn't mandate FK here
        conn.execute(
            "INSERT INTO facts (id, agent_id, content, summary, pointer, created_at, updated_at)
             VALUES ('f1', 'a', 'c', 's', 'p', 1, 1)",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO facts (id, agent_id, content, summary, pointer, created_at, updated_at)
             VALUES ('f2', 'a', 'c', 's', 'p', 1, 1)",
            [],
        ).unwrap();

        conn.execute(
            "INSERT INTO fact_links (source_id, target_id, relation) VALUES ('f1', 'f2', 'relates_to')",
            [],
        ).expect("first insert");

        let result = conn.execute(
            "INSERT INTO fact_links (source_id, target_id, relation) VALUES ('f1', 'f2', 'supersedes')",
            [],
        );
        assert!(result.is_err(), "duplicate (source_id, target_id) should violate PK");
    }

    #[test]
    fn fact_links_strength_default() {
        let conn = setup_agent_db();
        conn.execute(
            "INSERT INTO facts (id, agent_id, content, summary, pointer, created_at, updated_at)
             VALUES ('f1', 'a', 'c', 's', 'p', 1, 1)",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO facts (id, agent_id, content, summary, pointer, created_at, updated_at)
             VALUES ('f2', 'a', 'c', 's', 'p', 1, 1)",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO fact_links (source_id, target_id) VALUES ('f1', 'f2')",
            [],
        ).unwrap();

        let strength: f64 = conn
            .query_row("SELECT strength FROM fact_links WHERE source_id = 'f1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(strength, 1.0);
    }

    // ========================================================================
    // FACT_AUDIT
    // ========================================================================

    #[test]
    fn fact_audit_table_has_all_columns() {
        let conn = setup_agent_db();
        let expected = vec![
            "id", "fact_id", "operation", "old_content", "new_content",
            "reason", "source_message_id", "created_at",
        ];
        for col in &expected {
            assert!(has_column(&conn, "fact_audit", col), "fact_audit missing column: {col}");
        }
    }

    #[test]
    fn fact_audit_not_null_constraints() {
        let conn = setup_agent_db();
        let info = column_info(&conn, "fact_audit");
        for name in &["fact_id", "operation", "created_at"] {
            let col = info.iter().find(|c| c.0 == *name).unwrap();
            assert!(col.2, "fact_audit.{name} should be NOT NULL");
        }
    }

    // ========================================================================
    // SESSIONS
    // ========================================================================

    #[test]
    fn sessions_table_has_all_columns() {
        let conn = setup_agent_db();
        let expected = vec![
            "id", "agent_id", "channel", "started_at", "ended_at",
            "summary", "summary_embedding",
        ];
        for col in &expected {
            assert!(has_column(&conn, "sessions", col), "sessions missing column: {col}");
        }
    }

    #[test]
    fn sessions_not_null_constraints() {
        let conn = setup_agent_db();
        let info = column_info(&conn, "sessions");
        for name in &["agent_id", "started_at"] {
            let col = info.iter().find(|c| c.0 == *name).unwrap();
            assert!(col.2, "sessions.{name} should be NOT NULL");
        }
    }

    // ========================================================================
    // MESSAGES
    // ========================================================================

    #[test]
    fn messages_table_has_all_columns() {
        let conn = setup_agent_db();
        let expected = vec![
            "id", "session_id", "role", "content", "content_embedding", "created_at",
        ];
        for col in &expected {
            assert!(has_column(&conn, "messages", col), "messages missing column: {col}");
        }
    }

    #[test]
    fn messages_not_null_constraints() {
        let conn = setup_agent_db();
        let info = column_info(&conn, "messages");
        for name in &["session_id", "role", "content", "created_at"] {
            let col = info.iter().find(|c| c.0 == *name).unwrap();
            assert!(col.2, "messages.{name} should be NOT NULL");
        }
    }

    // ========================================================================
    // TOOL_CALLS
    // ========================================================================

    #[test]
    fn tool_calls_table_has_all_columns() {
        let conn = setup_agent_db();
        let expected = vec![
            "id", "message_id", "session_id", "tool_name", "arguments",
            "result", "status", "policy_decision", "duration_ms", "created_at",
        ];
        for col in &expected {
            assert!(has_column(&conn, "tool_calls", col), "tool_calls missing column: {col}");
        }
    }

    #[test]
    fn tool_calls_not_null_constraints() {
        let conn = setup_agent_db();
        let info = column_info(&conn, "tool_calls");
        for name in &["message_id", "session_id", "tool_name", "created_at"] {
            let col = info.iter().find(|c| c.0 == *name).unwrap();
            assert!(col.2, "tool_calls.{name} should be NOT NULL");
        }
    }

    // ========================================================================
    // DOCUMENTS
    // ========================================================================

    #[test]
    fn documents_table_has_all_columns() {
        let conn = setup_agent_db();
        let expected = vec![
            "id", "path", "title", "content_hash", "metadata", "created_at", "updated_at",
        ];
        for col in &expected {
            assert!(has_column(&conn, "documents", col), "documents missing column: {col}");
        }
    }

    #[test]
    fn documents_not_null_constraints() {
        let conn = setup_agent_db();
        let info = column_info(&conn, "documents");
        for name in &["content_hash", "created_at", "updated_at"] {
            let col = info.iter().find(|c| c.0 == *name).unwrap();
            assert!(col.2, "documents.{name} should be NOT NULL");
        }
    }

    // ========================================================================
    // CHUNKS
    // ========================================================================

    #[test]
    fn chunks_table_has_all_columns() {
        let conn = setup_agent_db();
        let expected = vec![
            "id", "document_id", "content", "summary",
            "content_embedding", "summary_embedding", "position", "created_at",
        ];
        for col in &expected {
            assert!(has_column(&conn, "chunks", col), "chunks missing column: {col}");
        }
    }

    #[test]
    fn chunks_not_null_constraints() {
        let conn = setup_agent_db();
        let info = column_info(&conn, "chunks");
        for name in &["document_id", "content", "created_at"] {
            let col = info.iter().find(|c| c.0 == *name).unwrap();
            assert!(col.2, "chunks.{name} should be NOT NULL");
        }
    }

    // ========================================================================
    // EDGES (knowledge graph)
    // ========================================================================

    #[test]
    fn edges_table_has_all_columns() {
        let conn = setup_agent_db();
        for col in &["source_id", "target_id", "relation"] {
            assert!(has_column(&conn, "edges", col), "edges missing column: {col}");
        }
    }

    #[test]
    fn edges_composite_primary_key() {
        let conn = setup_agent_db();
        conn.execute(
            "INSERT INTO edges (source_id, target_id, relation) VALUES ('a', 'b', 'references')",
            [],
        ).expect("first insert");

        // Same triple should fail
        let dup = conn.execute(
            "INSERT INTO edges (source_id, target_id, relation) VALUES ('a', 'b', 'references')",
            [],
        );
        assert!(dup.is_err(), "duplicate (source, target, relation) should violate PK");

        // Same pair, different relation should succeed
        conn.execute(
            "INSERT INTO edges (source_id, target_id, relation) VALUES ('a', 'b', 'depends_on')",
            [],
        ).expect("different relation should be allowed");
    }

    // ========================================================================
    // SKILLS
    // ========================================================================

    #[test]
    fn skills_table_has_all_columns() {
        let conn = setup_agent_db();
        let expected = vec![
            "id", "name", "description", "content", "tool_id",
            "content_embedding", "usage_count", "success_rate",
            "promoted", "created_at", "updated_at",
        ];
        for col in &expected {
            assert!(has_column(&conn, "skills", col), "skills missing column: {col}");
        }
    }

    #[test]
    fn skills_defaults() {
        let conn = setup_agent_db();
        conn.execute(
            "INSERT INTO skills (id, name, description, content, created_at, updated_at)
             VALUES ('s1', 'test', 'desc', 'body', 1, 1)",
            [],
        ).unwrap();

        let (usage_count, promoted): (i64, i64) = conn
            .query_row(
                "SELECT usage_count, promoted FROM skills WHERE id = 's1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();

        assert_eq!(usage_count, 0);
        assert_eq!(promoted, 0);
    }

    // ========================================================================
    // SCRATCH
    // ========================================================================

    #[test]
    fn scratch_table_has_all_columns() {
        let conn = setup_agent_db();
        let expected = vec!["id", "session_id", "key", "content", "created_at", "updated_at"];
        for col in &expected {
            assert!(has_column(&conn, "scratch", col), "scratch missing column: {col}");
        }
    }

    #[test]
    fn scratch_not_null_constraints() {
        let conn = setup_agent_db();
        let info = column_info(&conn, "scratch");
        for name in &["session_id", "key", "content", "created_at", "updated_at"] {
            let col = info.iter().find(|c| c.0 == *name).unwrap();
            assert!(col.2, "scratch.{name} should be NOT NULL");
        }
    }

    // ========================================================================
    // POLICIES
    // ========================================================================

    #[test]
    fn policies_table_has_all_columns() {
        let conn = setup_agent_db();
        let expected = vec![
            "id", "name", "priority", "phase", "effect",
            "actor_pattern", "action_pattern", "resource_pattern",
            "sql_pattern", "argument_pattern",
            "agent_id", "channel_pattern", "schedule",
            "message", "enabled", "created_at",
            "rule_type", "rule_config",
        ];
        for col in &expected {
            assert!(has_column(&conn, "policies", col), "policies missing column: {col}");
        }
    }

    #[test]
    fn policies_defaults() {
        let conn = setup_agent_db();
        conn.execute(
            "INSERT INTO policies (id, name, effect, created_at)
             VALUES ('p1', 'test', 'deny', 1)",
            [],
        ).unwrap();

        let (priority, phase, enabled): (i64, String, i64) = conn
            .query_row(
                "SELECT priority, phase, enabled FROM policies WHERE id = 'p1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();

        assert_eq!(priority, 0, "priority default should be 0");
        assert_eq!(phase, "pre", "phase default should be 'pre'");
        assert_eq!(enabled, 1, "enabled default should be 1");
    }

    // ========================================================================
    // POLICY_AUDIT
    // ========================================================================

    #[test]
    fn policy_audit_table_has_all_columns() {
        let conn = setup_agent_db();
        let expected = vec![
            "id", "policy_id", "actor", "action", "resource",
            "effect", "reason", "session_id", "created_at",
        ];
        for col in &expected {
            assert!(has_column(&conn, "policy_audit", col), "policy_audit missing: {col}");
        }
    }

    #[test]
    fn policy_audit_not_null_constraints() {
        let conn = setup_agent_db();
        let info = column_info(&conn, "policy_audit");
        for name in &["actor", "action", "resource", "effect", "created_at"] {
            let col = info.iter().find(|c| c.0 == *name).unwrap();
            assert!(col.2, "policy_audit.{name} should be NOT NULL");
        }
    }

    // ========================================================================
    // JOBS
    // ========================================================================

    #[test]
    fn jobs_table_has_all_columns() {
        let conn = setup_agent_db();
        let expected = vec![
            "id", "agent_id", "name", "description",
            "schedule", "next_run_at", "last_run_at", "timezone",
            "job_type", "payload",
            "max_retries", "retry_delay_ms", "timeout_ms", "overlap_policy",
            "status", "enabled", "created_at", "updated_at",
        ];
        for col in &expected {
            assert!(has_column(&conn, "jobs", col), "jobs missing column: {col}");
        }
    }

    #[test]
    fn jobs_defaults() {
        let conn = setup_agent_db();
        conn.execute(
            "INSERT INTO jobs (id, agent_id, name, schedule, next_run_at, job_type, payload, created_at, updated_at)
             VALUES ('j1', 'a', 'test', '* * * * *', 1000, 'prompt', '{}', 1, 1)",
            [],
        ).unwrap();

        let (tz, retries, delay, timeout, overlap, status, enabled): (String, i64, i64, i64, String, String, i64) = conn
            .query_row(
                "SELECT timezone, max_retries, retry_delay_ms, timeout_ms, overlap_policy, status, enabled FROM jobs WHERE id = 'j1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?)),
            )
            .unwrap();

        assert_eq!(tz, "UTC");
        assert_eq!(retries, 0);
        assert_eq!(delay, 5000);
        assert_eq!(timeout, 30000);
        assert_eq!(overlap, "skip");
        assert_eq!(status, "active");
        assert_eq!(enabled, 1);
    }

    // ========================================================================
    // JOB_RUNS
    // ========================================================================

    #[test]
    fn job_runs_table_has_all_columns() {
        let conn = setup_agent_db();
        let expected = vec![
            "id", "job_id", "agent_id", "started_at", "ended_at",
            "status", "result", "policy_decision", "retry_count", "created_at",
        ];
        for col in &expected {
            assert!(has_column(&conn, "job_runs", col), "job_runs missing column: {col}");
        }
    }

    #[test]
    fn job_runs_defaults() {
        let conn = setup_agent_db();
        conn.execute(
            "INSERT INTO job_runs (id, job_id, agent_id, started_at, status, created_at)
             VALUES ('r1', 'j1', 'a', 1000, 'running', 1000)",
            [],
        ).unwrap();

        let retry_count: i64 = conn
            .query_row("SELECT retry_count FROM job_runs WHERE id = 'r1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(retry_count, 0);
    }

    // ========================================================================
    // METADATA DB — AGENTS TABLE
    // ========================================================================

    #[test]
    fn metadata_agents_table_has_all_columns() {
        let conn = setup_metadata_db();
        let expected = vec![
            "id", "name", "persona", "trust_level", "llm_provider",
            "llm_model", "db_path", "sync_enabled", "created_at",
        ];
        for col in &expected {
            assert!(has_column(&conn, "agents", col), "agents missing column: {col}");
        }
    }

    #[test]
    fn metadata_agents_defaults() {
        let conn = setup_metadata_db();
        conn.execute(
            "INSERT INTO agents (id, name, db_path, created_at) VALUES ('a1', 'main', '/tmp/main.db', 1)",
            [],
        ).unwrap();

        let (trust, provider, sync): (String, String, i64) = conn
            .query_row(
                "SELECT trust_level, llm_provider, sync_enabled FROM agents WHERE id = 'a1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();

        assert_eq!(trust, "standard");
        assert_eq!(provider, "local");
        assert_eq!(sync, 1);
    }

    #[test]
    fn metadata_agents_name_not_null() {
        let conn = setup_metadata_db();
        let info = column_info(&conn, "agents");
        let name_col = info.iter().find(|c| c.0 == "name").unwrap();
        assert!(name_col.2, "agents.name should be NOT NULL");
        let db_path_col = info.iter().find(|c| c.0 == "db_path").unwrap();
        assert!(db_path_col.2, "agents.db_path should be NOT NULL");
    }

    // ========================================================================
    // CROSS-CUTTING: all 13 tables exist in the right databases
    // ========================================================================

    #[test]
    fn agent_db_has_all_tables() {
        let conn = setup_agent_db();
        let expected_tables = vec![
            "facts", "fact_links", "fact_audit",
            "sessions", "messages", "tool_calls",
            "documents", "chunks", "edges", "skills",
            "scratch",
            "policies", "policy_audit",
            "jobs", "job_runs",
        ];

        let mut stmt = conn.prepare(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'"
        ).unwrap();
        let tables: Vec<String> = stmt
            .query_map([], |row| row.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();

        for t in &expected_tables {
            assert!(tables.contains(&t.to_string()), "agent db missing table: {t}");
        }
    }

    #[test]
    fn metadata_db_has_agents_table() {
        let conn = setup_metadata_db();
        let mut stmt = conn.prepare(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'agents'"
        ).unwrap();
        let tables: Vec<String> = stmt
            .query_map([], |row| row.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();

        assert_eq!(tables, vec!["agents"]);
    }

    // ========================================================================
    // SCHEMA VERSIONING
    // ========================================================================

    #[test]
    fn schema_version_is_tracked() {
        let conn = setup_agent_db();
        let version: i64 = conn
            .query_row("SELECT version FROM schema_version ORDER BY version DESC LIMIT 1", [], |r| r.get(0))
            .expect("schema_version table should exist and have a row");
        assert!(version >= 1, "schema version should be at least 1");
    }

    #[test]
    fn init_is_idempotent() {
        let conn = db::open_memory().expect("open");
        super::init_agent_db(&conn).expect("first init");
        super::init_agent_db(&conn).expect("second init should not fail");

        // Tables should still be there, no duplicates
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'facts'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }
}
