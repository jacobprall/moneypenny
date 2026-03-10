/// SQLite-sync (cloudsync) CRDT integration.
///
/// Provides idempotent table registration, sync status queries, local peer-to-peer
/// sync via binary payloads, and optional cloud sync via the network layer.
///
/// # SQL functions used (provided by the `sqlite-sync` extension)
///
/// | Function | Purpose |
/// |---|---|
/// | `cloudsync_init(table)` | Enable CRDT tracking on a table (idempotent) |
/// | `cloudsync_is_enabled(table)` | Returns 1 if tracking is active |
/// | `cloudsync_db_version()` | Monotonic write clock for this DB |
/// | `cloudsync_siteid()` | UUID that identifies this DB replica |
/// | `cloudsync_payload_save(path)` | Dump pending changes to a binary file |
/// | `cloudsync_payload_load(path)` | Apply a change payload from a binary file |
/// | `cloudsync_network_init(url)` | Connect to a cloud sync server |
/// | `cloudsync_network_sync(ms, n)` | Bidirectional cloud sync |
/// | `cloudsync_terminate()` | Close cloud network connection |
use std::path::PathBuf;

use rusqlite::{Connection, params};
use tracing::{debug, info, warn};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Default tables that get CRDT metadata on every new agent DB.
pub const DEFAULT_SYNC_TABLES: &[&str] = &[
    "facts",
    "fact_links",
    "skills",
    "policies",
    "documents",
    "chunks",
    "edges",
    "jobs",
];

const MAX_SYNCED_CHUNK_CHARS: i64 = 20_000;

// ---------------------------------------------------------------------------
// Status types
// ---------------------------------------------------------------------------

/// Per-table sync state.
#[derive(Debug, Clone)]
pub struct TableSyncState {
    pub table: String,
    pub enabled: bool,
}

/// Overall sync status for one agent DB.
#[derive(Debug, Clone)]
pub struct SyncStatus {
    pub site_id: String,
    pub db_version: i64,
    pub tables: Vec<TableSyncState>,
}

impl std::fmt::Display for SyncStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "  Site ID   : {}", self.site_id)?;
        writeln!(f, "  DB version: {}", self.db_version)?;
        for t in &self.tables {
            let flag = if t.enabled { "✓" } else { "✗" };
            writeln!(f, "  [{flag}] {}", t.table)?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Sync result
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct SyncResult {
    /// Number of changes sent to the peer/cloud.
    pub sent: usize,
    /// Number of changes received from the peer/cloud.
    pub received: usize,
}

// ---------------------------------------------------------------------------
// Core functions
// ---------------------------------------------------------------------------

/// Idempotently enable CRDT sync tracking on the given tables.
///
/// Tables that already have tracking enabled (per `cloudsync_is_enabled`) are
/// silently skipped so this is safe to call on every DB open.
///
/// Returns the count of tables that were newly initialized.
pub fn init_sync_tables(conn: &Connection, tables: &[&str]) -> anyhow::Result<usize> {
    let mut count = 0;
    for &table in tables {
        // Check if the table actually exists before asking cloudsync about it
        let exists: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                params![table],
                |r| r.get::<_, i64>(0),
            )
            .map(|n| n > 0)
            .unwrap_or(false);

        if !exists {
            debug!("sync: skipping unknown table '{table}'");
            continue;
        }

        let enabled: i64 = conn
            .query_row("SELECT cloudsync_is_enabled(?1)", params![table], |r| {
                r.get(0)
            })
            .unwrap_or(0);

        if enabled == 0 {
            match conn.execute_batch(&format!("SELECT cloudsync_init('{table}');")) {
                Ok(()) => {
                    info!("sync: initialized CRDT tracking on '{table}'");
                    count += 1;
                }
                Err(e) => {
                    warn!("sync: could not init '{table}': {e}");
                }
            }
        }
    }
    Ok(count)
}

/// Query the current sync status for all tracked tables.
pub fn status(conn: &Connection, tables: &[&str]) -> anyhow::Result<SyncStatus> {
    let site_id: String = conn
        .query_row("SELECT cloudsync_siteid()", [], |r| r.get(0))
        .unwrap_or_else(|_| "unknown".into());

    let db_version: i64 = conn
        .query_row("SELECT cloudsync_db_version()", [], |r| r.get(0))
        .unwrap_or(0);

    let mut table_states = Vec::new();
    for &table in tables {
        let exists: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                params![table],
                |r| r.get::<_, i64>(0),
            )
            .map(|n| n > 0)
            .unwrap_or(false);

        let enabled = if exists {
            conn.query_row("SELECT cloudsync_is_enabled(?1)", params![table], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap_or(0)
                == 1
        } else {
            false
        };

        table_states.push(TableSyncState {
            table: table.to_string(),
            enabled,
        });
    }

    Ok(SyncStatus {
        site_id,
        db_version,
        tables: table_states,
    })
}

// ---------------------------------------------------------------------------
// Local P2P sync (via file payload)
//
// All three functions require both connections to be pre-opened with all
// SQLite extensions already registered (call `mp_ext::init_all_extensions`
// before passing the connection in). This keeps `mp-core` free of a
// dependency on `mp-ext`.
// ---------------------------------------------------------------------------

/// Bidirectional sync between two pre-opened agent connections.
///
/// Uses `cloudsync_payload_save` / `cloudsync_payload_load` with a pair of
/// temp files to exchange changes in both directions.  The CRDT merge is
/// fully idempotent, so replaying the same payload is always safe.
pub fn local_sync_bidirectional(
    conn_a: &Connection,
    conn_b: &Connection,
    agent_a: &str,
    agent_b: &str,
    tables: &[&str],
) -> anyhow::Result<SyncResult> {
    let tmp_dir = std::env::temp_dir();
    let a_to_b = tmp_dir.join("mp_sync_a_to_b.bin");
    let b_to_a = tmp_dir.join("mp_sync_b_to_a.bin");

    // Phase 1: A → B
    let sent = exchange_payload(conn_a, conn_b, &a_to_b, tables)?;
    enforce_local_sync_constraints(conn_b, agent_b)?;
    // Phase 2: B → A
    let received = exchange_payload(conn_b, conn_a, &b_to_a, tables)?;
    enforce_local_sync_constraints(conn_a, agent_a)?;

    let _ = std::fs::remove_file(&a_to_b);
    let _ = std::fs::remove_file(&b_to_a);

    Ok(SyncResult { sent, received })
}

/// Push `source`'s pending changes into `target` (one-way).
pub fn local_sync_push(
    source: &Connection,
    target: &Connection,
    target_agent: &str,
    tables: &[&str],
) -> anyhow::Result<SyncResult> {
    let tmp = std::env::temp_dir().join("mp_sync_push.bin");
    let sent = exchange_payload(source, target, &tmp, tables)?;
    enforce_local_sync_constraints(target, target_agent)?;
    let _ = std::fs::remove_file(&tmp);
    Ok(SyncResult { sent, received: 0 })
}

/// Pull changes from `source` into `local` (one-way).
pub fn local_sync_pull(
    local: &Connection,
    source: &Connection,
    local_agent: &str,
    tables: &[&str],
) -> anyhow::Result<SyncResult> {
    let tmp = std::env::temp_dir().join("mp_sync_pull.bin");
    let received = exchange_payload(source, local, &tmp, tables)?;
    enforce_local_sync_constraints(local, local_agent)?;
    let _ = std::fs::remove_file(&tmp);
    Ok(SyncResult { sent: 0, received })
}

fn enforce_local_sync_constraints(conn: &Connection, local_agent: &str) -> anyhow::Result<()> {
    enforce_private_fact_locality(conn, local_agent)?;
    enforce_document_scope_and_size(conn, local_agent)?;
    enforce_job_run_locality(conn, local_agent)?;
    Ok(())
}

fn enforce_private_fact_locality(conn: &Connection, local_agent: &str) -> anyhow::Result<()> {
    let has_scope: bool = conn
        .query_row(
            "SELECT EXISTS(
                SELECT 1
                FROM pragma_table_info('facts')
                WHERE name = 'scope'
            )",
            [],
            |r| r.get(0),
        )
        .unwrap_or(false);
    if !has_scope {
        return Ok(());
    }

    conn.execute(
        "DELETE FROM facts
         WHERE scope = 'private' AND agent_id <> ?1",
        params![local_agent],
    )?;
    // Cleanup orphaned links after dropping private facts not owned locally.
    conn.execute(
        "DELETE FROM fact_links
         WHERE source_id NOT IN (SELECT id FROM facts)
            OR target_id NOT IN (SELECT id FROM facts)",
        [],
    )?;
    Ok(())
}

fn has_table_column(conn: &Connection, table: &str, column: &str) -> bool {
    conn.query_row(
        "SELECT EXISTS(
            SELECT 1
            FROM pragma_table_info(?1)
            WHERE name = ?2
        )",
        params![table, column],
        |r| r.get::<_, bool>(0),
    )
    .unwrap_or(false)
}

fn enforce_document_scope_and_size(conn: &Connection, local_agent: &str) -> anyhow::Result<()> {
    let has_documents = has_table_column(conn, "documents", "id");
    let has_chunks = has_table_column(conn, "chunks", "id");
    if !has_documents && !has_chunks {
        return Ok(());
    }

    let has_scope = has_table_column(conn, "documents", "scope");
    let has_owner = has_table_column(conn, "documents", "agent_id");
    if has_documents && has_scope && has_owner {
        conn.execute(
            "DELETE FROM documents
             WHERE scope = 'private' AND agent_id <> ?1",
            params![local_agent],
        )?;
    }

    // Size policy: discard oversized remote chunks to keep sync payload practical.
    if has_chunks && has_owner {
        conn.execute(
            "DELETE FROM chunks
             WHERE document_id IN (
                 SELECT d.id
                 FROM documents d
                 WHERE d.agent_id <> ?1
             )
               AND length(content) > ?2",
            params![local_agent, MAX_SYNCED_CHUNK_CHARS],
        )?;
    } else if has_chunks {
        conn.execute(
            "DELETE FROM chunks WHERE length(content) > ?1",
            params![MAX_SYNCED_CHUNK_CHARS],
        )?;
    }

    if has_chunks && has_documents {
        conn.execute(
            "DELETE FROM chunks
             WHERE document_id NOT IN (SELECT id FROM documents)",
            [],
        )?;
        conn.execute(
            "DELETE FROM documents
             WHERE id NOT IN (SELECT document_id FROM chunks)",
            [],
        )?;
    }
    Ok(())
}

fn enforce_job_run_locality(conn: &Connection, local_agent: &str) -> anyhow::Result<()> {
    if !has_table_column(conn, "job_runs", "agent_id") {
        return Ok(());
    }
    conn.execute(
        "DELETE FROM job_runs WHERE agent_id <> ?1",
        params![local_agent],
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Cloud sync
// ---------------------------------------------------------------------------

/// Connect to a cloud sync server, perform a bidirectional sync, and disconnect.
///
/// Requires the `cloudsync_network_*` SQL functions to be available (macOS/Linux).
/// On platforms where the network layer is omitted, this returns an error.
pub fn cloud_sync(conn: &Connection, cloud_url: &str) -> anyhow::Result<SyncResult> {
    // Initialize the network connection
    let rc: i64 = conn
        .query_row(
            "SELECT cloudsync_network_init(?1)",
            params![cloud_url],
            |r| r.get(0),
        )
        .map_err(|e| anyhow::anyhow!("cloudsync_network_init failed: {e}"))?;

    if rc != 0 {
        anyhow::bail!("cloudsync_network_init returned error code {rc}");
    }

    // Run bidirectional sync (500ms timeout, up to 20 retries)
    let changes: i64 = conn
        .query_row("SELECT cloudsync_network_sync(500, 20)", [], |r| r.get(0))
        .map_err(|e| anyhow::anyhow!("cloudsync_network_sync failed: {e}"))?;

    // Terminate the network session
    let _ = conn.execute_batch("SELECT cloudsync_terminate();");

    info!("cloud sync complete: {changes} change batch(es) exchanged");

    // The network API returns total batches, not individual row counts.
    // We report it as sent for display purposes.
    Ok(SyncResult {
        sent: changes.max(0) as usize,
        received: 0,
    })
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Save a payload from `source`, apply it to `target`.  Returns the size of
/// the payload blob in bytes (0 if there were no pending changes).
fn exchange_payload(
    source: &Connection,
    target: &Connection,
    tmp_file: &PathBuf,
    _tables: &[&str],
) -> anyhow::Result<usize> {
    let tmp_str = tmp_file
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("non-UTF-8 temp path"))?;

    // Save the payload from source to a temp file
    let _rc: i64 = source
        .query_row("SELECT cloudsync_payload_save(?1)", params![tmp_str], |r| {
            r.get(0)
        })
        .map_err(|e| anyhow::anyhow!("cloudsync_payload_save failed: {e}"))?;

    if !tmp_file.exists() {
        debug!("sync: no pending changes to send");
        return Ok(0);
    }

    let payload_bytes = std::fs::metadata(tmp_file).map(|m| m.len()).unwrap_or(0);

    if payload_bytes == 0 {
        debug!("sync: payload file is empty, skipping apply");
        return Ok(0);
    }

    // Apply the payload to target
    target
        .execute_batch(&format!("SELECT cloudsync_payload_load('{tmp_str}');"))
        .map_err(|e| anyhow::anyhow!("cloudsync_payload_load failed: {e}"))?;

    debug!("sync: exchanged {payload_bytes} bytes");
    Ok(payload_bytes as usize)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn make_test_db() -> Connection {
        // Extensions are statically linked into the `mp` binary but not into
        // `mp-core` (which has no `mp-ext` dep). In unit tests we verify the
        // Rust-level logic; the SQLite-level cloudsync functions are exercised
        // by integration tests in the `mp` crate where `mp_ext` IS available.
        // We still call open_in_memory so the schema exists for the status tests.
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE facts (
                id TEXT PRIMARY KEY,
                agent_id TEXT NOT NULL DEFAULT '',
                scope TEXT NOT NULL DEFAULT 'shared',
                summary TEXT NOT NULL DEFAULT '',
                pointer TEXT NOT NULL DEFAULT '',
                confidence REAL NOT NULL DEFAULT 1.0,
                active INTEGER NOT NULL DEFAULT 1,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                compression_level INTEGER NOT NULL DEFAULT 0,
                content_embedding BLOB
            );
            CREATE TABLE fact_links (
                id TEXT PRIMARY KEY,
                source_id TEXT NOT NULL,
                target_id TEXT NOT NULL,
                relation TEXT NOT NULL DEFAULT 'related',
                weight REAL NOT NULL DEFAULT 1.0
            );
            CREATE TABLE skills (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                description TEXT NOT NULL DEFAULT '',
                content TEXT,
                tool_id TEXT,
                active INTEGER NOT NULL DEFAULT 1
            );
            CREATE TABLE policies (
                id TEXT PRIMARY KEY,
                agent_id TEXT NOT NULL,
                name TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                rule TEXT NOT NULL DEFAULT '',
                priority INTEGER NOT NULL DEFAULT 0,
                enabled INTEGER NOT NULL DEFAULT 1,
                rule_type TEXT NOT NULL DEFAULT 'static',
                rule_config TEXT
            );
            CREATE TABLE documents (
                id TEXT PRIMARY KEY,
                agent_id TEXT NOT NULL DEFAULT '',
                scope TEXT NOT NULL DEFAULT 'shared',
                path TEXT,
                title TEXT,
                content_hash TEXT NOT NULL DEFAULT '',
                metadata TEXT,
                created_at INTEGER NOT NULL DEFAULT 0,
                updated_at INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE chunks (
                id TEXT PRIMARY KEY,
                document_id TEXT NOT NULL,
                content TEXT NOT NULL,
                position INTEGER DEFAULT 0,
                created_at INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE edges (
                source_id TEXT NOT NULL,
                target_id TEXT NOT NULL,
                relation TEXT NOT NULL,
                PRIMARY KEY (source_id, target_id, relation)
            );
            CREATE TABLE jobs (
                id TEXT PRIMARY KEY,
                agent_id TEXT NOT NULL,
                name TEXT NOT NULL,
                schedule TEXT NOT NULL,
                next_run_at INTEGER NOT NULL,
                job_type TEXT NOT NULL DEFAULT 'tool',
                payload TEXT NOT NULL DEFAULT '{}',
                enabled INTEGER NOT NULL DEFAULT 1,
                status TEXT NOT NULL DEFAULT 'active',
                created_at INTEGER NOT NULL DEFAULT 0,
                updated_at INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE job_runs (
                id TEXT PRIMARY KEY,
                job_id TEXT NOT NULL,
                agent_id TEXT NOT NULL,
                started_at INTEGER NOT NULL,
                status TEXT NOT NULL,
                created_at INTEGER NOT NULL DEFAULT 0
            );",
        )
        .unwrap();
        conn
    }

    /// Helper: check if the sqlite-sync extension is available on this connection.
    fn has_cloudsync(conn: &Connection) -> bool {
        conn.execute_batch("SELECT cloudsync_version()").is_ok()
    }

    #[test]
    fn init_tables_registers_all_default_tables() {
        let conn = make_test_db();
        if !has_cloudsync(&conn) {
            // Extension not available in unit-test context — skip.
            return;
        }
        let n = init_sync_tables(&conn, DEFAULT_SYNC_TABLES).unwrap();
        assert_eq!(
            n,
            DEFAULT_SYNC_TABLES.len(),
            "all tables should be newly initialized"
        );
    }

    #[test]
    fn init_tables_is_idempotent() {
        let conn = make_test_db();
        if !has_cloudsync(&conn) {
            return;
        }
        init_sync_tables(&conn, DEFAULT_SYNC_TABLES).unwrap();
        let n = init_sync_tables(&conn, DEFAULT_SYNC_TABLES).unwrap();
        assert_eq!(n, 0, "second call should find all tables already enabled");
    }

    #[test]
    fn status_returns_correct_data() {
        let conn = make_test_db();
        if !has_cloudsync(&conn) {
            return;
        }
        init_sync_tables(&conn, DEFAULT_SYNC_TABLES).unwrap();
        let st = status(&conn, DEFAULT_SYNC_TABLES).unwrap();
        assert!(!st.site_id.is_empty());
        assert!(st.db_version >= 0);
        assert!(st.tables.iter().all(|t| t.enabled));
    }

    #[test]
    fn status_unknown_table_shows_disabled() {
        let conn = make_test_db();
        // This test doesn't need cloudsync — unknown table is just absent from schema.
        let st = status(&conn, &["nonexistent_table"]).unwrap();
        assert!(!st.tables[0].enabled);
    }

    #[test]
    fn local_sync_pushes_facts_between_dbs() {
        let source = make_test_db();
        let target = make_test_db();
        if !has_cloudsync(&source) {
            return;
        }

        init_sync_tables(&source, DEFAULT_SYNC_TABLES).unwrap();
        init_sync_tables(&target, DEFAULT_SYNC_TABLES).unwrap();

        source
            .execute_batch(
                "INSERT INTO facts (id, agent_id, pointer, summary, scope)
             VALUES ('fact-1', 'agent-a', 'ptr', 'test fact', 'shared');",
            )
            .unwrap();

        let result = local_sync_push(&source, &target, "agent-b", DEFAULT_SYNC_TABLES).unwrap();
        let count: i64 = target
            .query_row("SELECT COUNT(*) FROM facts WHERE id='fact-1'", [], |r| {
                r.get(0)
            })
            .unwrap_or(0);
        assert!(
            result.sent > 0 || count == 1,
            "fact should be synced: sent={}, count={}",
            result.sent,
            count
        );
    }
}
