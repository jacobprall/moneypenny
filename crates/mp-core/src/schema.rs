/// Schema definitions and migrations for Moneypenny databases.
///
/// Two database types:
/// - **Agent DB**: one per agent, contains all memory stores + policies + jobs
/// - **Metadata DB**: one per gateway, contains agent registry + routing
use rusqlite::Connection;

const AGENT_SCHEMA_VERSION: i64 = 13;
const METADATA_SCHEMA_VERSION: i64 = 1;

pub fn init_agent_db(conn: &Connection) -> anyhow::Result<()> {
    let current = get_schema_version(conn);
    if current >= AGENT_SCHEMA_VERSION {
        return Ok(());
    }

    if current < 1 {
        conn.execute_batch(AGENT_SCHEMA_V1)?;
        set_schema_version(conn, 1)?;
    }

    if current < 2 {
        conn.execute_batch(AGENT_SCHEMA_V2)?;
        set_schema_version(conn, 2)?;
    }

    if current < 3 {
        conn.execute_batch(AGENT_SCHEMA_V3)?;
        set_schema_version(conn, 3)?;
    }

    if current < 4 {
        conn.execute_batch(AGENT_SCHEMA_V4)?;
        set_schema_version(conn, 4)?;
    }

    if current < 5 {
        conn.execute_batch(AGENT_SCHEMA_V5)?;
        set_schema_version(conn, 5)?;
    }

    if current < 6 {
        conn.execute_batch(AGENT_SCHEMA_V6)?;
        set_schema_version(conn, 6)?;
    }

    if current < 7 {
        conn.execute_batch(AGENT_SCHEMA_V7)?;
        set_schema_version(conn, 7)?;
    }

    if current < 8 {
        conn.execute_batch(AGENT_SCHEMA_V8)?;
        set_schema_version(conn, 8)?;
    }

    if current < 9 {
        conn.execute_batch(AGENT_SCHEMA_V9)?;
        set_schema_version(conn, 9)?;
    }

    if current < 10 {
        conn.execute_batch(AGENT_SCHEMA_V10)?;
        set_schema_version(conn, 10)?;
    }

    if current < 11 {
        conn.execute_batch(AGENT_SCHEMA_V11)?;
        set_schema_version(conn, 11)?;
    }

    if current < 12 {
        conn.execute_batch(AGENT_SCHEMA_V12)?;
        set_schema_version(conn, 12)?;
    }

    if current < 13 {
        conn.execute_batch(AGENT_SCHEMA_V13)?;
        set_schema_version(conn, 13)?;
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
        ("facts", "content_embedding"),
        ("messages", "content_embedding"),
        ("tool_calls", "content_embedding"),
        ("policy_audit", "content_embedding"),
        ("chunks", "content_embedding"),
    ] {
        conn.query_row(
            "SELECT vector_init(?1, ?2, ?3)",
            rusqlite::params![table, col, opts],
            |_| Ok::<_, rusqlite::Error>(()),
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

const AGENT_SCHEMA_V3: &str = "
ALTER TABLE policy_audit ADD COLUMN correlation_id TEXT;
";

const AGENT_SCHEMA_V4: &str = "
CREATE TABLE IF NOT EXISTS external_events (
    id                  TEXT PRIMARY KEY,
    source              TEXT NOT NULL DEFAULT '',
    source_event_id     TEXT,
    event_type          TEXT NOT NULL DEFAULT '',
    event_ts            INTEGER NOT NULL DEFAULT 0,
    session_id          TEXT,
    payload_json        TEXT NOT NULL DEFAULT '',
    content_hash        TEXT NOT NULL DEFAULT '',
    run_id              TEXT NOT NULL DEFAULT '',
    line_no             INTEGER NOT NULL DEFAULT 0,
    raw_line            TEXT NOT NULL DEFAULT '',
    projected           INTEGER NOT NULL DEFAULT 0,
    projection_error    TEXT,
    ingested_at         INTEGER NOT NULL DEFAULT 0
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_external_events_source_event
ON external_events (source, source_event_id)
WHERE source_event_id IS NOT NULL;

CREATE UNIQUE INDEX IF NOT EXISTS idx_external_events_source_hash
ON external_events (source, content_hash)
WHERE source_event_id IS NULL;

CREATE TABLE IF NOT EXISTS ingest_runs (
    id                  TEXT PRIMARY KEY,
    source              TEXT NOT NULL DEFAULT '',
    file_path           TEXT NOT NULL DEFAULT '',
    from_line           INTEGER NOT NULL DEFAULT 1,
    to_line             INTEGER NOT NULL DEFAULT 0,
    processed_count     INTEGER NOT NULL DEFAULT 0,
    inserted_count      INTEGER NOT NULL DEFAULT 0,
    deduped_count       INTEGER NOT NULL DEFAULT 0,
    projected_count     INTEGER NOT NULL DEFAULT 0,
    error_count         INTEGER NOT NULL DEFAULT 0,
    status              TEXT NOT NULL DEFAULT 'running',
    last_error          TEXT,
    started_at          INTEGER NOT NULL DEFAULT 0,
    finished_at         INTEGER
);
";

const AGENT_SCHEMA_V5: &str = "
ALTER TABLE policy_audit ADD COLUMN idempotency_key TEXT;
ALTER TABLE policy_audit ADD COLUMN idempotency_state TEXT;

CREATE TABLE IF NOT EXISTS operation_idempotency (
    id                  TEXT PRIMARY KEY,
    actor_id            TEXT NOT NULL DEFAULT '',
    op                  TEXT NOT NULL DEFAULT '',
    idempotency_key     TEXT NOT NULL DEFAULT '',
    request_fingerprint TEXT NOT NULL DEFAULT '',
    response_json       TEXT NOT NULL DEFAULT '',
    created_at          INTEGER NOT NULL DEFAULT 0,
    last_replayed_at    INTEGER,
    replay_count        INTEGER NOT NULL DEFAULT 0
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_operation_idempotency_actor_op_key
ON operation_idempotency (actor_id, op, idempotency_key);
";

const AGENT_SCHEMA_V6: &str = "
CREATE TABLE IF NOT EXISTS operation_hooks (
    id                  TEXT PRIMARY KEY,
    op_pattern          TEXT NOT NULL DEFAULT '*',
    phase               TEXT NOT NULL DEFAULT 'pre',
    hook_type           TEXT NOT NULL DEFAULT '',
    config_json         TEXT NOT NULL DEFAULT '{}',
    enabled             INTEGER NOT NULL DEFAULT 1,
    created_at          INTEGER NOT NULL DEFAULT 0
);
";

const AGENT_SCHEMA_V7: &str = "
ALTER TABLE facts ADD COLUMN context_compact TEXT;
ALTER TABLE facts ADD COLUMN compaction_level INTEGER NOT NULL DEFAULT 0;
ALTER TABLE facts ADD COLUMN last_compacted_at INTEGER;
";

const AGENT_SCHEMA_V8: &str = "
ALTER TABLE tool_calls ADD COLUMN content_embedding BLOB;
ALTER TABLE policy_audit ADD COLUMN content_embedding BLOB;
";

const AGENT_SCHEMA_V9: &str = "
ALTER TABLE external_events ADD COLUMN normalized_provider TEXT;
ALTER TABLE external_events ADD COLUMN normalized_model TEXT;
ALTER TABLE external_events ADD COLUMN normalized_input_tokens INTEGER;
ALTER TABLE external_events ADD COLUMN normalized_output_tokens INTEGER;
ALTER TABLE external_events ADD COLUMN normalized_total_tokens INTEGER;
ALTER TABLE external_events ADD COLUMN normalized_cost_usd REAL;
ALTER TABLE external_events ADD COLUMN normalized_correlation_id TEXT;
";

const AGENT_SCHEMA_V10: &str = "
CREATE TABLE IF NOT EXISTS job_specs (
    id                  TEXT PRIMARY KEY,
    agent_id            TEXT NOT NULL DEFAULT '',
    intent              TEXT NOT NULL DEFAULT '',
    plan_json           TEXT NOT NULL DEFAULT '{}',
    job_name            TEXT NOT NULL DEFAULT '',
    schedule            TEXT NOT NULL DEFAULT '',
    job_type            TEXT NOT NULL DEFAULT 'prompt',
    payload_json        TEXT NOT NULL DEFAULT '{}',
    status              TEXT NOT NULL DEFAULT 'planned',
    proposed_by         TEXT NOT NULL DEFAULT 'agent',
    source_session_id   TEXT,
    source_message_id   TEXT,
    applied_job_id      TEXT,
    created_at          INTEGER NOT NULL DEFAULT 0,
    updated_at          INTEGER NOT NULL DEFAULT 0
);
";

const AGENT_SCHEMA_V11: &str = "
CREATE TABLE IF NOT EXISTS policy_specs (
    id                  TEXT PRIMARY KEY,
    agent_id            TEXT NOT NULL DEFAULT '',
    intent              TEXT NOT NULL DEFAULT '',
    plan_json           TEXT NOT NULL DEFAULT '{}',
    policy_name         TEXT NOT NULL DEFAULT '',
    effect              TEXT NOT NULL DEFAULT 'deny',
    priority            INTEGER NOT NULL DEFAULT 0,
    actor_pattern       TEXT,
    action_pattern      TEXT,
    resource_pattern    TEXT,
    argument_pattern    TEXT,
    channel_pattern     TEXT,
    sql_pattern         TEXT,
    rule_type           TEXT,
    rule_config         TEXT,
    message             TEXT,
    status              TEXT NOT NULL DEFAULT 'planned',
    proposed_by         TEXT NOT NULL DEFAULT 'agent',
    source_session_id   TEXT,
    source_message_id   TEXT,
    applied_policy_id   TEXT,
    created_at          INTEGER NOT NULL DEFAULT 0,
    updated_at          INTEGER NOT NULL DEFAULT 0
);
";

const AGENT_SCHEMA_V12: &str = "
ALTER TABLE facts ADD COLUMN embedding_model TEXT;
ALTER TABLE facts ADD COLUMN embedding_content_hash TEXT;

ALTER TABLE messages ADD COLUMN embedding_model TEXT;
ALTER TABLE messages ADD COLUMN embedding_content_hash TEXT;

ALTER TABLE tool_calls ADD COLUMN embedding_model TEXT;
ALTER TABLE tool_calls ADD COLUMN embedding_content_hash TEXT;

ALTER TABLE policy_audit ADD COLUMN embedding_model TEXT;
ALTER TABLE policy_audit ADD COLUMN embedding_content_hash TEXT;

ALTER TABLE chunks ADD COLUMN embedding_model TEXT;
ALTER TABLE chunks ADD COLUMN embedding_content_hash TEXT;

CREATE TABLE IF NOT EXISTS embedding_jobs (
    target              TEXT NOT NULL,
    row_id              TEXT NOT NULL,
    status              TEXT NOT NULL DEFAULT 'pending',
    attempts            INTEGER NOT NULL DEFAULT 0,
    last_error          TEXT,
    next_attempt_at     INTEGER NOT NULL DEFAULT 0,
    lease_expires_at    INTEGER,
    created_at          INTEGER NOT NULL DEFAULT 0,
    updated_at          INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (target, row_id)
);

CREATE INDEX IF NOT EXISTS idx_embedding_jobs_due
ON embedding_jobs (status, next_attempt_at, updated_at);

CREATE INDEX IF NOT EXISTS idx_embedding_jobs_lease
ON embedding_jobs (status, lease_expires_at);

CREATE TRIGGER IF NOT EXISTS trg_embed_jobs_facts_insert
AFTER INSERT ON facts
BEGIN
    INSERT INTO embedding_jobs (target, row_id, status, attempts, next_attempt_at, created_at, updated_at)
    VALUES ('facts', NEW.id, 'pending', 0, strftime('%s','now'), strftime('%s','now'), strftime('%s','now'))
    ON CONFLICT(target, row_id) DO UPDATE SET
        status = 'pending',
        attempts = 0,
        last_error = NULL,
        next_attempt_at = strftime('%s','now'),
        lease_expires_at = NULL,
        updated_at = strftime('%s','now');
END;

CREATE TRIGGER IF NOT EXISTS trg_embed_jobs_facts_update
AFTER UPDATE OF content ON facts
WHEN OLD.content IS NOT NEW.content
BEGIN
    UPDATE facts
       SET content_embedding = NULL,
           embedding_model = NULL,
           embedding_content_hash = NULL
     WHERE id = NEW.id;
    INSERT INTO embedding_jobs (target, row_id, status, attempts, next_attempt_at, created_at, updated_at)
    VALUES ('facts', NEW.id, 'pending', 0, strftime('%s','now'), strftime('%s','now'), strftime('%s','now'))
    ON CONFLICT(target, row_id) DO UPDATE SET
        status = 'pending',
        attempts = 0,
        last_error = NULL,
        next_attempt_at = strftime('%s','now'),
        lease_expires_at = NULL,
        updated_at = strftime('%s','now');
END;

CREATE TRIGGER IF NOT EXISTS trg_embed_jobs_messages_insert
AFTER INSERT ON messages
BEGIN
    INSERT INTO embedding_jobs (target, row_id, status, attempts, next_attempt_at, created_at, updated_at)
    VALUES ('messages', NEW.id, 'pending', 0, strftime('%s','now'), strftime('%s','now'), strftime('%s','now'))
    ON CONFLICT(target, row_id) DO UPDATE SET
        status = 'pending',
        attempts = 0,
        last_error = NULL,
        next_attempt_at = strftime('%s','now'),
        lease_expires_at = NULL,
        updated_at = strftime('%s','now');
END;

CREATE TRIGGER IF NOT EXISTS trg_embed_jobs_messages_update
AFTER UPDATE OF content ON messages
WHEN OLD.content IS NOT NEW.content
BEGIN
    UPDATE messages
       SET content_embedding = NULL,
           embedding_model = NULL,
           embedding_content_hash = NULL
     WHERE id = NEW.id;
    INSERT INTO embedding_jobs (target, row_id, status, attempts, next_attempt_at, created_at, updated_at)
    VALUES ('messages', NEW.id, 'pending', 0, strftime('%s','now'), strftime('%s','now'), strftime('%s','now'))
    ON CONFLICT(target, row_id) DO UPDATE SET
        status = 'pending',
        attempts = 0,
        last_error = NULL,
        next_attempt_at = strftime('%s','now'),
        lease_expires_at = NULL,
        updated_at = strftime('%s','now');
END;

CREATE TRIGGER IF NOT EXISTS trg_embed_jobs_tool_calls_insert
AFTER INSERT ON tool_calls
BEGIN
    INSERT INTO embedding_jobs (target, row_id, status, attempts, next_attempt_at, created_at, updated_at)
    VALUES ('tool_calls', NEW.id, 'pending', 0, strftime('%s','now'), strftime('%s','now'), strftime('%s','now'))
    ON CONFLICT(target, row_id) DO UPDATE SET
        status = 'pending',
        attempts = 0,
        last_error = NULL,
        next_attempt_at = strftime('%s','now'),
        lease_expires_at = NULL,
        updated_at = strftime('%s','now');
END;

CREATE TRIGGER IF NOT EXISTS trg_embed_jobs_tool_calls_update
AFTER UPDATE OF tool_name, arguments, result, status, policy_decision ON tool_calls
WHEN OLD.tool_name IS NOT NEW.tool_name
   OR OLD.arguments IS NOT NEW.arguments
   OR OLD.result IS NOT NEW.result
   OR OLD.status IS NOT NEW.status
   OR OLD.policy_decision IS NOT NEW.policy_decision
BEGIN
    UPDATE tool_calls
       SET content_embedding = NULL,
           embedding_model = NULL,
           embedding_content_hash = NULL
     WHERE id = NEW.id;
    INSERT INTO embedding_jobs (target, row_id, status, attempts, next_attempt_at, created_at, updated_at)
    VALUES ('tool_calls', NEW.id, 'pending', 0, strftime('%s','now'), strftime('%s','now'), strftime('%s','now'))
    ON CONFLICT(target, row_id) DO UPDATE SET
        status = 'pending',
        attempts = 0,
        last_error = NULL,
        next_attempt_at = strftime('%s','now'),
        lease_expires_at = NULL,
        updated_at = strftime('%s','now');
END;

CREATE TRIGGER IF NOT EXISTS trg_embed_jobs_policy_audit_insert
AFTER INSERT ON policy_audit
BEGIN
    INSERT INTO embedding_jobs (target, row_id, status, attempts, next_attempt_at, created_at, updated_at)
    VALUES ('policy_audit', NEW.id, 'pending', 0, strftime('%s','now'), strftime('%s','now'), strftime('%s','now'))
    ON CONFLICT(target, row_id) DO UPDATE SET
        status = 'pending',
        attempts = 0,
        last_error = NULL,
        next_attempt_at = strftime('%s','now'),
        lease_expires_at = NULL,
        updated_at = strftime('%s','now');
END;

CREATE TRIGGER IF NOT EXISTS trg_embed_jobs_policy_audit_update
AFTER UPDATE OF actor, action, resource, effect, reason ON policy_audit
WHEN OLD.actor IS NOT NEW.actor
   OR OLD.action IS NOT NEW.action
   OR OLD.resource IS NOT NEW.resource
   OR OLD.effect IS NOT NEW.effect
   OR OLD.reason IS NOT NEW.reason
BEGIN
    UPDATE policy_audit
       SET content_embedding = NULL,
           embedding_model = NULL,
           embedding_content_hash = NULL
     WHERE id = NEW.id;
    INSERT INTO embedding_jobs (target, row_id, status, attempts, next_attempt_at, created_at, updated_at)
    VALUES ('policy_audit', NEW.id, 'pending', 0, strftime('%s','now'), strftime('%s','now'), strftime('%s','now'))
    ON CONFLICT(target, row_id) DO UPDATE SET
        status = 'pending',
        attempts = 0,
        last_error = NULL,
        next_attempt_at = strftime('%s','now'),
        lease_expires_at = NULL,
        updated_at = strftime('%s','now');
END;

CREATE TRIGGER IF NOT EXISTS trg_embed_jobs_chunks_insert
AFTER INSERT ON chunks
BEGIN
    INSERT INTO embedding_jobs (target, row_id, status, attempts, next_attempt_at, created_at, updated_at)
    VALUES ('chunks', NEW.id, 'pending', 0, strftime('%s','now'), strftime('%s','now'), strftime('%s','now'))
    ON CONFLICT(target, row_id) DO UPDATE SET
        status = 'pending',
        attempts = 0,
        last_error = NULL,
        next_attempt_at = strftime('%s','now'),
        lease_expires_at = NULL,
        updated_at = strftime('%s','now');
END;

CREATE TRIGGER IF NOT EXISTS trg_embed_jobs_chunks_update
AFTER UPDATE OF content ON chunks
WHEN OLD.content IS NOT NEW.content
BEGIN
    UPDATE chunks
       SET content_embedding = NULL,
           embedding_model = NULL,
           embedding_content_hash = NULL
     WHERE id = NEW.id;
    INSERT INTO embedding_jobs (target, row_id, status, attempts, next_attempt_at, created_at, updated_at)
    VALUES ('chunks', NEW.id, 'pending', 0, strftime('%s','now'), strftime('%s','now'), strftime('%s','now'))
    ON CONFLICT(target, row_id) DO UPDATE SET
        status = 'pending',
        attempts = 0,
        last_error = NULL,
        next_attempt_at = strftime('%s','now'),
        lease_expires_at = NULL,
        updated_at = strftime('%s','now');
END;
";

const AGENT_SCHEMA_V13: &str = "
CREATE TABLE IF NOT EXISTS activity_log (
    id                  TEXT PRIMARY KEY,
    agent_id            TEXT NOT NULL DEFAULT '',
    event               TEXT NOT NULL DEFAULT '',
    action              TEXT NOT NULL DEFAULT '',
    resource            TEXT NOT NULL DEFAULT '',
    detail              TEXT NOT NULL DEFAULT '',
    conversation_id     TEXT NOT NULL DEFAULT '',
    generation_id       TEXT NOT NULL DEFAULT '',
    duration_ms         INTEGER,
    created_at          INTEGER NOT NULL DEFAULT (strftime('%s','now'))
);

CREATE INDEX IF NOT EXISTS idx_activity_log_agent
ON activity_log (agent_id, created_at);

CREATE INDEX IF NOT EXISTS idx_activity_log_event
ON activity_log (event, created_at);

CREATE INDEX IF NOT EXISTS idx_activity_log_conversation
ON activity_log (conversation_id, created_at);
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
                row.get::<_, String>(1)?,         // name
                row.get::<_, String>(2)?,         // type
                row.get::<_, bool>(3)?,           // notnull
                row.get::<_, Option<String>>(4)?, // dflt_value
                row.get::<_, bool>(5)?,           // pk
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
            "id",
            "agent_id",
            "content",
            "summary",
            "pointer",
            "content_embedding",
            "summary_embedding",
            "pointer_embedding",
            "embedding_model",
            "embedding_content_hash",
            "keywords",
            "source_message_id",
            "confidence",
            "created_at",
            "updated_at",
            "superseded_at",
            "version",
            "context_compact",
            "compaction_level",
            "last_compacted_at",
        ];
        for col in &expected {
            assert!(
                has_column(&conn, "facts", col),
                "facts missing column: {col}"
            );
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
        let required = [
            "agent_id",
            "content",
            "summary",
            "pointer",
            "created_at",
            "updated_at",
        ];
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
        assert_eq!(
            confidence.3.as_deref(),
            Some("1.0"),
            "confidence default should be 1.0"
        );
        let version = info.iter().find(|c| c.0 == "version").unwrap();
        assert_eq!(
            version.3.as_deref(),
            Some("1"),
            "version default should be 1"
        );
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
            assert!(
                has_column(&conn, "fact_links", col),
                "fact_links missing column: {col}"
            );
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
        )
        .unwrap();
        conn.execute(
            "INSERT INTO facts (id, agent_id, content, summary, pointer, created_at, updated_at)
             VALUES ('f2', 'a', 'c', 's', 'p', 1, 1)",
            [],
        )
        .unwrap();

        conn.execute(
            "INSERT INTO fact_links (source_id, target_id, relation) VALUES ('f1', 'f2', 'relates_to')",
            [],
        ).expect("first insert");

        let result = conn.execute(
            "INSERT INTO fact_links (source_id, target_id, relation) VALUES ('f1', 'f2', 'supersedes')",
            [],
        );
        assert!(
            result.is_err(),
            "duplicate (source_id, target_id) should violate PK"
        );
    }

    #[test]
    fn fact_links_strength_default() {
        let conn = setup_agent_db();
        conn.execute(
            "INSERT INTO facts (id, agent_id, content, summary, pointer, created_at, updated_at)
             VALUES ('f1', 'a', 'c', 's', 'p', 1, 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO facts (id, agent_id, content, summary, pointer, created_at, updated_at)
             VALUES ('f2', 'a', 'c', 's', 'p', 1, 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO fact_links (source_id, target_id) VALUES ('f1', 'f2')",
            [],
        )
        .unwrap();

        let strength: f64 = conn
            .query_row(
                "SELECT strength FROM fact_links WHERE source_id = 'f1'",
                [],
                |r| r.get(0),
            )
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
            "id",
            "fact_id",
            "operation",
            "old_content",
            "new_content",
            "reason",
            "source_message_id",
            "created_at",
        ];
        for col in &expected {
            assert!(
                has_column(&conn, "fact_audit", col),
                "fact_audit missing column: {col}"
            );
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
            "id",
            "agent_id",
            "channel",
            "started_at",
            "ended_at",
            "summary",
            "summary_embedding",
        ];
        for col in &expected {
            assert!(
                has_column(&conn, "sessions", col),
                "sessions missing column: {col}"
            );
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
            "id",
            "session_id",
            "role",
            "content",
            "content_embedding",
            "embedding_model",
            "embedding_content_hash",
            "created_at",
        ];
        for col in &expected {
            assert!(
                has_column(&conn, "messages", col),
                "messages missing column: {col}"
            );
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
            "id",
            "message_id",
            "session_id",
            "tool_name",
            "arguments",
            "result",
            "status",
            "policy_decision",
            "content_embedding",
            "embedding_model",
            "embedding_content_hash",
            "duration_ms",
            "created_at",
        ];
        for col in &expected {
            assert!(
                has_column(&conn, "tool_calls", col),
                "tool_calls missing column: {col}"
            );
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
            "id",
            "path",
            "title",
            "content_hash",
            "metadata",
            "created_at",
            "updated_at",
        ];
        for col in &expected {
            assert!(
                has_column(&conn, "documents", col),
                "documents missing column: {col}"
            );
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
            "id",
            "document_id",
            "content",
            "summary",
            "content_embedding",
            "summary_embedding",
            "embedding_model",
            "embedding_content_hash",
            "position",
            "created_at",
        ];
        for col in &expected {
            assert!(
                has_column(&conn, "chunks", col),
                "chunks missing column: {col}"
            );
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
            assert!(
                has_column(&conn, "edges", col),
                "edges missing column: {col}"
            );
        }
    }

    #[test]
    fn edges_composite_primary_key() {
        let conn = setup_agent_db();
        conn.execute(
            "INSERT INTO edges (source_id, target_id, relation) VALUES ('a', 'b', 'references')",
            [],
        )
        .expect("first insert");

        // Same triple should fail
        let dup = conn.execute(
            "INSERT INTO edges (source_id, target_id, relation) VALUES ('a', 'b', 'references')",
            [],
        );
        assert!(
            dup.is_err(),
            "duplicate (source, target, relation) should violate PK"
        );

        // Same pair, different relation should succeed
        conn.execute(
            "INSERT INTO edges (source_id, target_id, relation) VALUES ('a', 'b', 'depends_on')",
            [],
        )
        .expect("different relation should be allowed");
    }

    // ========================================================================
    // SKILLS
    // ========================================================================

    #[test]
    fn skills_table_has_all_columns() {
        let conn = setup_agent_db();
        let expected = vec![
            "id",
            "name",
            "description",
            "content",
            "tool_id",
            "content_embedding",
            "usage_count",
            "success_rate",
            "promoted",
            "created_at",
            "updated_at",
        ];
        for col in &expected {
            assert!(
                has_column(&conn, "skills", col),
                "skills missing column: {col}"
            );
        }
    }

    #[test]
    fn skills_defaults() {
        let conn = setup_agent_db();
        conn.execute(
            "INSERT INTO skills (id, name, description, content, created_at, updated_at)
             VALUES ('s1', 'test', 'desc', 'body', 1, 1)",
            [],
        )
        .unwrap();

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
        let expected = vec![
            "id",
            "session_id",
            "key",
            "content",
            "created_at",
            "updated_at",
        ];
        for col in &expected {
            assert!(
                has_column(&conn, "scratch", col),
                "scratch missing column: {col}"
            );
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
            "id",
            "name",
            "priority",
            "phase",
            "effect",
            "actor_pattern",
            "action_pattern",
            "resource_pattern",
            "sql_pattern",
            "argument_pattern",
            "agent_id",
            "channel_pattern",
            "schedule",
            "message",
            "enabled",
            "created_at",
            "rule_type",
            "rule_config",
        ];
        for col in &expected {
            assert!(
                has_column(&conn, "policies", col),
                "policies missing column: {col}"
            );
        }
    }

    #[test]
    fn policies_defaults() {
        let conn = setup_agent_db();
        conn.execute(
            "INSERT INTO policies (id, name, effect, created_at)
             VALUES ('p1', 'test', 'deny', 1)",
            [],
        )
        .unwrap();

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
            "id",
            "policy_id",
            "actor",
            "action",
            "resource",
            "effect",
            "reason",
            "content_embedding",
            "embedding_model",
            "embedding_content_hash",
            "correlation_id",
            "session_id",
            "idempotency_key",
            "idempotency_state",
            "created_at",
        ];
        for col in &expected {
            assert!(
                has_column(&conn, "policy_audit", col),
                "policy_audit missing: {col}"
            );
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
    // OPERATION_HOOKS
    // ========================================================================

    #[test]
    fn operation_hooks_table_has_all_columns() {
        let conn = setup_agent_db();
        let expected = vec![
            "id",
            "op_pattern",
            "phase",
            "hook_type",
            "config_json",
            "enabled",
            "created_at",
        ];
        for col in &expected {
            assert!(
                has_column(&conn, "operation_hooks", col),
                "operation_hooks missing: {col}"
            );
        }
    }

    #[test]
    fn embedding_jobs_table_has_all_columns() {
        let conn = setup_agent_db();
        let expected = vec![
            "target",
            "row_id",
            "status",
            "attempts",
            "last_error",
            "next_attempt_at",
            "lease_expires_at",
            "created_at",
            "updated_at",
        ];
        for col in &expected {
            assert!(
                has_column(&conn, "embedding_jobs", col),
                "embedding_jobs missing: {col}"
            );
        }
    }

    #[test]
    fn external_events_table_has_normalized_projection_columns() {
        let conn = setup_agent_db();
        let expected = vec![
            "normalized_provider",
            "normalized_model",
            "normalized_input_tokens",
            "normalized_output_tokens",
            "normalized_total_tokens",
            "normalized_cost_usd",
            "normalized_correlation_id",
        ];
        for col in &expected {
            assert!(
                has_column(&conn, "external_events", col),
                "external_events missing normalized projection column: {col}"
            );
        }
    }

    // ========================================================================
    // JOB_SPECS (agent-generated job planning)
    // ========================================================================

    #[test]
    fn job_specs_table_has_all_columns() {
        let conn = setup_agent_db();
        let expected = vec![
            "id",
            "agent_id",
            "intent",
            "plan_json",
            "job_name",
            "schedule",
            "job_type",
            "payload_json",
            "status",
            "proposed_by",
            "source_session_id",
            "source_message_id",
            "applied_job_id",
            "created_at",
            "updated_at",
        ];
        for col in &expected {
            assert!(
                has_column(&conn, "job_specs", col),
                "job_specs missing column: {col}"
            );
        }
    }

    #[test]
    fn job_specs_defaults() {
        let conn = setup_agent_db();
        conn.execute(
            "INSERT INTO job_specs (id, agent_id, intent, created_at, updated_at)
             VALUES ('spec1', 'a', 'weekly maintenance plan', 1, 1)",
            [],
        )
        .unwrap();

        let (job_type, status, proposed_by): (String, String, String) = conn
            .query_row(
                "SELECT job_type, status, proposed_by FROM job_specs WHERE id = 'spec1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(job_type, "prompt");
        assert_eq!(status, "planned");
        assert_eq!(proposed_by, "agent");
    }

    // ========================================================================
    // JOBS
    // ========================================================================

    #[test]
    fn jobs_table_has_all_columns() {
        let conn = setup_agent_db();
        let expected = vec![
            "id",
            "agent_id",
            "name",
            "description",
            "schedule",
            "next_run_at",
            "last_run_at",
            "timezone",
            "job_type",
            "payload",
            "max_retries",
            "retry_delay_ms",
            "timeout_ms",
            "overlap_policy",
            "status",
            "enabled",
            "created_at",
            "updated_at",
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
            "id",
            "job_id",
            "agent_id",
            "started_at",
            "ended_at",
            "status",
            "result",
            "policy_decision",
            "retry_count",
            "created_at",
        ];
        for col in &expected {
            assert!(
                has_column(&conn, "job_runs", col),
                "job_runs missing column: {col}"
            );
        }
    }

    #[test]
    fn job_runs_defaults() {
        let conn = setup_agent_db();
        conn.execute(
            "INSERT INTO job_runs (id, job_id, agent_id, started_at, status, created_at)
             VALUES ('r1', 'j1', 'a', 1000, 'running', 1000)",
            [],
        )
        .unwrap();

        let retry_count: i64 = conn
            .query_row(
                "SELECT retry_count FROM job_runs WHERE id = 'r1'",
                [],
                |r| r.get(0),
            )
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
            "id",
            "name",
            "persona",
            "trust_level",
            "llm_provider",
            "llm_model",
            "db_path",
            "sync_enabled",
            "created_at",
        ];
        for col in &expected {
            assert!(
                has_column(&conn, "agents", col),
                "agents missing column: {col}"
            );
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
            "facts",
            "fact_links",
            "fact_audit",
            "sessions",
            "messages",
            "tool_calls",
            "documents",
            "chunks",
            "edges",
            "skills",
            "scratch",
            "policies",
            "policy_audit",
            "jobs",
            "job_runs",
            "job_specs",
            "policy_specs",
            "external_events",
            "ingest_runs",
            "operation_idempotency",
            "operation_hooks",
            "embedding_jobs",
        ];

        let mut stmt = conn
            .prepare(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
            )
            .unwrap();
        let tables: Vec<String> = stmt
            .query_map([], |row| row.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();

        for t in &expected_tables {
            assert!(
                tables.contains(&t.to_string()),
                "agent db missing table: {t}"
            );
        }
    }

    #[test]
    fn metadata_db_has_agents_table() {
        let conn = setup_metadata_db();
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'agents'")
            .unwrap();
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
            .query_row(
                "SELECT version FROM schema_version ORDER BY version DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .expect("schema_version table should exist and have a row");
        assert_eq!(
            version,
            super::AGENT_SCHEMA_VERSION,
            "schema version should match current"
        );
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
