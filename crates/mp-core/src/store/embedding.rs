use anyhow::Result;
use rusqlite::{Connection, params};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::future::Future;

use super::{facts, knowledge, log};

const DEFAULT_CLAIM_LEASE_SECS: i64 = 60;
const MAX_RETRY_BACKOFF_SECS: i64 = 600;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EmbeddingTargetKind {
    Facts,
    Messages,
    ToolCalls,
    PolicyAudit,
    KnowledgeChunks,
}

impl EmbeddingTargetKind {
    pub fn queue_target(self) -> &'static str {
        match self {
            EmbeddingTargetKind::Facts => "facts",
            EmbeddingTargetKind::Messages => "messages",
            EmbeddingTargetKind::ToolCalls => "tool_calls",
            EmbeddingTargetKind::PolicyAudit => "policy_audit",
            EmbeddingTargetKind::KnowledgeChunks => "chunks",
        }
    }

    fn from_queue_target(value: &str) -> Option<Self> {
        match value {
            "facts" => Some(EmbeddingTargetKind::Facts),
            "messages" => Some(EmbeddingTargetKind::Messages),
            "tool_calls" => Some(EmbeddingTargetKind::ToolCalls),
            "policy_audit" => Some(EmbeddingTargetKind::PolicyAudit),
            "chunks" => Some(EmbeddingTargetKind::KnowledgeChunks),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PendingEmbedding {
    pub id: String,
    pub content: String,
    pub target: EmbeddingTargetKind,
}

#[derive(Debug, Clone)]
struct ClaimedEmbeddingJob {
    target: EmbeddingTargetKind,
    row_id: String,
    attempts: i64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EmbeddingRunStats {
    pub queued: usize,
    pub claimed: usize,
    pub embedded: usize,
    pub failed: usize,
    pub skipped: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EmbeddingQueueStats {
    pub total: i64,
    pub pending: i64,
    pub retry: i64,
    pub processing: i64,
    pub dead: i64,
}

pub trait EmbeddingStore {
    fn target() -> EmbeddingTargetKind;
    fn pending(conn: &Connection, agent_id: &str) -> Result<Vec<(String, String)>>;
    fn set(conn: &Connection, id: &str, blob: &[u8]) -> Result<()>;
    fn set_with_meta(
        conn: &Connection,
        id: &str,
        blob: &[u8],
        embedding_model: Option<&str>,
        embedding_content_hash: Option<&str>,
    ) -> Result<()>;
    fn vector_index() -> (&'static str, &'static str);
}

pub struct FactsEmbeddingStore;
pub struct MessagesEmbeddingStore;
pub struct ToolCallsEmbeddingStore;
pub struct PolicyAuditEmbeddingStore;
pub struct KnowledgeChunksEmbeddingStore;

impl EmbeddingStore for FactsEmbeddingStore {
    fn target() -> EmbeddingTargetKind {
        EmbeddingTargetKind::Facts
    }

    fn pending(conn: &Connection, agent_id: &str) -> Result<Vec<(String, String)>> {
        let ids = facts::ids_without_embedding(conn, agent_id)?;
        let mut rows = Vec::with_capacity(ids.len());
        for id in ids {
            if let Ok(content) = conn.query_row(
                "SELECT content FROM facts WHERE id = ?1",
                params![id],
                |r| r.get::<_, String>(0),
            ) {
                rows.push((id, content));
            }
        }
        Ok(rows)
    }

    fn set(conn: &Connection, id: &str, blob: &[u8]) -> Result<()> {
        facts::set_content_embedding(conn, id, blob)
    }

    fn set_with_meta(
        conn: &Connection,
        id: &str,
        blob: &[u8],
        embedding_model: Option<&str>,
        embedding_content_hash: Option<&str>,
    ) -> Result<()> {
        facts::set_content_embedding_with_meta(
            conn,
            id,
            blob,
            embedding_model,
            embedding_content_hash,
        )
    }

    fn vector_index() -> (&'static str, &'static str) {
        ("facts", "content_embedding")
    }
}

impl EmbeddingStore for MessagesEmbeddingStore {
    fn target() -> EmbeddingTargetKind {
        EmbeddingTargetKind::Messages
    }

    fn pending(conn: &Connection, agent_id: &str) -> Result<Vec<(String, String)>> {
        log::messages_without_embedding(conn, agent_id)
    }

    fn set(conn: &Connection, id: &str, blob: &[u8]) -> Result<()> {
        log::set_message_embedding(conn, id, blob)
    }

    fn set_with_meta(
        conn: &Connection,
        id: &str,
        blob: &[u8],
        embedding_model: Option<&str>,
        embedding_content_hash: Option<&str>,
    ) -> Result<()> {
        log::set_message_embedding_with_meta(
            conn,
            id,
            blob,
            embedding_model,
            embedding_content_hash,
        )
    }

    fn vector_index() -> (&'static str, &'static str) {
        ("messages", "content_embedding")
    }
}

impl EmbeddingStore for ToolCallsEmbeddingStore {
    fn target() -> EmbeddingTargetKind {
        EmbeddingTargetKind::ToolCalls
    }

    fn pending(conn: &Connection, agent_id: &str) -> Result<Vec<(String, String)>> {
        log::tool_calls_without_embedding(conn, agent_id)
    }

    fn set(conn: &Connection, id: &str, blob: &[u8]) -> Result<()> {
        log::set_tool_call_embedding(conn, id, blob)
    }

    fn set_with_meta(
        conn: &Connection,
        id: &str,
        blob: &[u8],
        embedding_model: Option<&str>,
        embedding_content_hash: Option<&str>,
    ) -> Result<()> {
        log::set_tool_call_embedding_with_meta(
            conn,
            id,
            blob,
            embedding_model,
            embedding_content_hash,
        )
    }

    fn vector_index() -> (&'static str, &'static str) {
        ("tool_calls", "content_embedding")
    }
}

impl EmbeddingStore for PolicyAuditEmbeddingStore {
    fn target() -> EmbeddingTargetKind {
        EmbeddingTargetKind::PolicyAudit
    }

    fn pending(conn: &Connection, agent_id: &str) -> Result<Vec<(String, String)>> {
        log::policy_audit_without_embedding(conn, agent_id)
    }

    fn set(conn: &Connection, id: &str, blob: &[u8]) -> Result<()> {
        log::set_policy_audit_embedding(conn, id, blob)
    }

    fn set_with_meta(
        conn: &Connection,
        id: &str,
        blob: &[u8],
        embedding_model: Option<&str>,
        embedding_content_hash: Option<&str>,
    ) -> Result<()> {
        log::set_policy_audit_embedding_with_meta(
            conn,
            id,
            blob,
            embedding_model,
            embedding_content_hash,
        )
    }

    fn vector_index() -> (&'static str, &'static str) {
        ("policy_audit", "content_embedding")
    }
}

impl EmbeddingStore for KnowledgeChunksEmbeddingStore {
    fn target() -> EmbeddingTargetKind {
        EmbeddingTargetKind::KnowledgeChunks
    }

    fn pending(conn: &Connection, _agent_id: &str) -> Result<Vec<(String, String)>> {
        knowledge::chunks_without_embedding(conn)
    }

    fn set(conn: &Connection, id: &str, blob: &[u8]) -> Result<()> {
        knowledge::set_chunk_embedding(conn, id, blob)
    }

    fn set_with_meta(
        conn: &Connection,
        id: &str,
        blob: &[u8],
        embedding_model: Option<&str>,
        embedding_content_hash: Option<&str>,
    ) -> Result<()> {
        knowledge::set_chunk_embedding_with_meta(
            conn,
            id,
            blob,
            embedding_model,
            embedding_content_hash,
        )
    }

    fn vector_index() -> (&'static str, &'static str) {
        ("chunks", "content_embedding")
    }
}

pub fn model_identity(provider: &str, model: &str, dimensions: usize) -> String {
    format!("{provider}:{model}:{dimensions}")
}

pub fn content_sha256(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    let out = hasher.finalize();
    format!("{out:x}")
}

pub fn collect_pending(conn: &Connection, agent_id: &str) -> Result<Vec<PendingEmbedding>> {
    fn append_store<S: EmbeddingStore>(
        conn: &Connection,
        agent_id: &str,
        out: &mut Vec<PendingEmbedding>,
    ) -> Result<()> {
        for (id, content) in S::pending(conn, agent_id)? {
            out.push(PendingEmbedding {
                id,
                content,
                target: S::target(),
            });
        }
        Ok(())
    }

    let mut rows = Vec::new();
    append_store::<FactsEmbeddingStore>(conn, agent_id, &mut rows)?;
    append_store::<MessagesEmbeddingStore>(conn, agent_id, &mut rows)?;
    append_store::<ToolCallsEmbeddingStore>(conn, agent_id, &mut rows)?;
    append_store::<PolicyAuditEmbeddingStore>(conn, agent_id, &mut rows)?;
    append_store::<KnowledgeChunksEmbeddingStore>(conn, agent_id, &mut rows)?;
    Ok(rows)
}

pub fn set_embedding(
    conn: &Connection,
    target: EmbeddingTargetKind,
    id: &str,
    blob: &[u8],
) -> Result<()> {
    match target {
        EmbeddingTargetKind::Facts => FactsEmbeddingStore::set(conn, id, blob),
        EmbeddingTargetKind::Messages => MessagesEmbeddingStore::set(conn, id, blob),
        EmbeddingTargetKind::ToolCalls => ToolCallsEmbeddingStore::set(conn, id, blob),
        EmbeddingTargetKind::PolicyAudit => PolicyAuditEmbeddingStore::set(conn, id, blob),
        EmbeddingTargetKind::KnowledgeChunks => KnowledgeChunksEmbeddingStore::set(conn, id, blob),
    }
}

pub fn set_embedding_with_meta(
    conn: &Connection,
    target: EmbeddingTargetKind,
    id: &str,
    blob: &[u8],
    embedding_model: Option<&str>,
    embedding_content_hash: Option<&str>,
) -> Result<()> {
    match target {
        EmbeddingTargetKind::Facts => FactsEmbeddingStore::set_with_meta(
            conn,
            id,
            blob,
            embedding_model,
            embedding_content_hash,
        ),
        EmbeddingTargetKind::Messages => MessagesEmbeddingStore::set_with_meta(
            conn,
            id,
            blob,
            embedding_model,
            embedding_content_hash,
        ),
        EmbeddingTargetKind::ToolCalls => ToolCallsEmbeddingStore::set_with_meta(
            conn,
            id,
            blob,
            embedding_model,
            embedding_content_hash,
        ),
        EmbeddingTargetKind::PolicyAudit => PolicyAuditEmbeddingStore::set_with_meta(
            conn,
            id,
            blob,
            embedding_model,
            embedding_content_hash,
        ),
        EmbeddingTargetKind::KnowledgeChunks => KnowledgeChunksEmbeddingStore::set_with_meta(
            conn,
            id,
            blob,
            embedding_model,
            embedding_content_hash,
        ),
    }
}

pub fn vector_indexes() -> [(&'static str, &'static str); 5] {
    [
        FactsEmbeddingStore::vector_index(),
        MessagesEmbeddingStore::vector_index(),
        ToolCallsEmbeddingStore::vector_index(),
        PolicyAuditEmbeddingStore::vector_index(),
        KnowledgeChunksEmbeddingStore::vector_index(),
    ]
}

pub fn rebuild_vector_indexes(conn: &Connection) {
    for (table, col) in vector_indexes() {
        let _ = conn.execute("SELECT vector_quantize(?1, ?2)", params![table, col]);
    }
}

fn rebuild_vector_indexes_for(conn: &Connection, touched: &HashSet<EmbeddingTargetKind>) {
    for target in touched {
        let (table, col) = match target {
            EmbeddingTargetKind::Facts => FactsEmbeddingStore::vector_index(),
            EmbeddingTargetKind::Messages => MessagesEmbeddingStore::vector_index(),
            EmbeddingTargetKind::ToolCalls => ToolCallsEmbeddingStore::vector_index(),
            EmbeddingTargetKind::PolicyAudit => PolicyAuditEmbeddingStore::vector_index(),
            EmbeddingTargetKind::KnowledgeChunks => KnowledgeChunksEmbeddingStore::vector_index(),
        };
        let _ = conn.execute("SELECT vector_quantize(?1, ?2)", params![table, col]);
    }
}

fn queue_upsert_sql(select_sql: &str) -> String {
    format!(
        "INSERT INTO embedding_jobs
            (target, row_id, status, attempts, last_error, next_attempt_at, lease_expires_at, created_at, updated_at)
         {select_sql}
         ON CONFLICT(target, row_id) DO UPDATE SET
             status = 'pending',
             attempts = 0,
             last_error = NULL,
             next_attempt_at = excluded.next_attempt_at,
             lease_expires_at = NULL,
             updated_at = excluded.updated_at"
    )
}

pub fn enqueue_drift_jobs(
    conn: &Connection,
    agent_id: &str,
    model_id: &str,
    limit_per_target: usize,
) -> Result<usize> {
    let now = chrono::Utc::now().timestamp();
    let limit = limit_per_target.max(1) as i64;
    let mut queued = 0usize;

    let sql = queue_upsert_sql(
        "SELECT 'facts', f.id, 'pending', 0, NULL, ?3, NULL, ?3, ?3
         FROM facts f
         WHERE f.agent_id = ?1
           AND f.superseded_at IS NULL
           AND (
                f.content_embedding IS NULL OR
                f.embedding_model IS NULL OR
                f.embedding_model <> ?2
           )
         ORDER BY f.updated_at ASC
         LIMIT ?4",
    );
    queued += conn.execute(&sql, params![agent_id, model_id, now, limit])?;

    let sql = queue_upsert_sql(
        "SELECT 'messages', m.id, 'pending', 0, NULL, ?3, NULL, ?3, ?3
         FROM messages m
         JOIN sessions s ON s.id = m.session_id
         WHERE s.agent_id = ?1
           AND (
                m.content_embedding IS NULL OR
                m.embedding_model IS NULL OR
                m.embedding_model <> ?2
           )
         ORDER BY m.created_at ASC
         LIMIT ?4",
    );
    queued += conn.execute(&sql, params![agent_id, model_id, now, limit])?;

    let sql = queue_upsert_sql(
        "SELECT 'tool_calls', tc.id, 'pending', 0, NULL, ?3, NULL, ?3, ?3
         FROM tool_calls tc
         JOIN sessions s ON s.id = tc.session_id
         WHERE s.agent_id = ?1
           AND (
                tc.content_embedding IS NULL OR
                tc.embedding_model IS NULL OR
                tc.embedding_model <> ?2
           )
         ORDER BY tc.created_at ASC
         LIMIT ?4",
    );
    queued += conn.execute(&sql, params![agent_id, model_id, now, limit])?;

    let sql = queue_upsert_sql(
        "SELECT 'policy_audit', pa.id, 'pending', 0, NULL, ?3, NULL, ?3, ?3
         FROM policy_audit pa
         WHERE (
                pa.actor = ?1 OR
                pa.session_id IN (SELECT id FROM sessions WHERE agent_id = ?1)
               )
           AND (
                pa.content_embedding IS NULL OR
                pa.embedding_model IS NULL OR
                pa.embedding_model <> ?2
           )
         ORDER BY pa.created_at ASC
         LIMIT ?4",
    );
    queued += conn.execute(&sql, params![agent_id, model_id, now, limit])?;

    let sql = queue_upsert_sql(
        "SELECT 'chunks', c.id, 'pending', 0, NULL, ?2, NULL, ?2, ?2
         FROM chunks c
         WHERE (
                c.content_embedding IS NULL OR
                c.embedding_model IS NULL OR
                c.embedding_model <> ?1
           )
         ORDER BY c.created_at ASC
         LIMIT ?3",
    );
    queued += conn.execute(&sql, params![model_id, now, limit])?;

    Ok(queued)
}

fn claim_due_jobs(
    conn: &Connection,
    limit: usize,
    lease_seconds: i64,
) -> Result<Vec<ClaimedEmbeddingJob>> {
    if limit == 0 {
        return Ok(Vec::new());
    }

    let now = chrono::Utc::now().timestamp();
    let lease_expires_at = now + lease_seconds.max(1);
    conn.execute_batch("BEGIN IMMEDIATE")?;

    let result = (|| -> Result<Vec<ClaimedEmbeddingJob>> {
        let mut stmt = conn.prepare(
            "SELECT target, row_id, attempts
             FROM embedding_jobs
             WHERE (
                    status IN ('pending', 'retry') OR
                    (status = 'processing' AND COALESCE(lease_expires_at, 0) <= ?1)
                   )
               AND next_attempt_at <= ?1
             ORDER BY updated_at ASC
             LIMIT ?2",
        )?;
        let candidates = stmt
            .query_map(params![now, limit as i64], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut claimed = Vec::new();
        for (target_str, row_id, attempts) in candidates {
            let Some(target) = EmbeddingTargetKind::from_queue_target(&target_str) else {
                continue;
            };
            let changed = conn.execute(
                "UPDATE embedding_jobs
                 SET status = 'processing',
                     lease_expires_at = ?1,
                     updated_at = ?2
                 WHERE target = ?3
                   AND row_id = ?4
                   AND (
                        status IN ('pending', 'retry') OR
                        (status = 'processing' AND COALESCE(lease_expires_at, 0) <= ?2)
                   )
                   AND next_attempt_at <= ?2",
                params![lease_expires_at, now, target.queue_target(), row_id],
            )?;
            if changed > 0 {
                claimed.push(ClaimedEmbeddingJob {
                    target,
                    row_id,
                    attempts,
                });
            }
        }
        Ok(claimed)
    })();

    match result {
        Ok(rows) => {
            conn.execute_batch("COMMIT")?;
            Ok(rows)
        }
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(e)
        }
    }
}

fn mark_job_done(conn: &Connection, target: EmbeddingTargetKind, row_id: &str) {
    let _ = conn.execute(
        "DELETE FROM embedding_jobs WHERE target = ?1 AND row_id = ?2",
        params![target.queue_target(), row_id],
    );
}

fn mark_job_failed(
    conn: &Connection,
    target: EmbeddingTargetKind,
    row_id: &str,
    attempts_so_far: i64,
    max_attempts: i64,
    retry_base_seconds: i64,
    error: &str,
) {
    let now = chrono::Utc::now().timestamp();
    let attempts = attempts_so_far + 1;
    let status = if attempts >= max_attempts {
        "dead"
    } else {
        "retry"
    };
    let exp = attempts.saturating_sub(1).clamp(0, 8) as u32;
    let backoff = (retry_base_seconds.max(1) * 2_i64.pow(exp)).min(MAX_RETRY_BACKOFF_SECS);
    let next_attempt_at = now + backoff;

    let _ = conn.execute(
        "UPDATE embedding_jobs
         SET status = ?1,
             attempts = ?2,
             last_error = ?3,
             next_attempt_at = ?4,
             lease_expires_at = NULL,
             updated_at = ?5
         WHERE target = ?6 AND row_id = ?7",
        params![
            status,
            attempts,
            error,
            next_attempt_at,
            now,
            target.queue_target(),
            row_id
        ],
    );
}

fn fetch_content_for_target(
    conn: &Connection,
    target: EmbeddingTargetKind,
    row_id: &str,
    agent_id: &str,
) -> Result<Option<String>> {
    let out = match target {
        EmbeddingTargetKind::Facts => conn.query_row(
            "SELECT content
             FROM facts
             WHERE id = ?1 AND agent_id = ?2 AND superseded_at IS NULL",
            params![row_id, agent_id],
            |r| r.get::<_, String>(0),
        ),
        EmbeddingTargetKind::Messages => conn.query_row(
            "SELECT m.content
             FROM messages m
             JOIN sessions s ON s.id = m.session_id
             WHERE m.id = ?1 AND s.agent_id = ?2",
            params![row_id, agent_id],
            |r| r.get::<_, String>(0),
        ),
        EmbeddingTargetKind::ToolCalls => conn.query_row(
            &{
                let projection = log::tool_call_projection_expr("tc");
                format!(
                    "SELECT
                        {projection}
                     FROM tool_calls tc
                     JOIN sessions s ON s.id = tc.session_id
                     WHERE tc.id = ?1 AND s.agent_id = ?2"
                )
            },
            params![row_id, agent_id],
            |r| r.get::<_, String>(0),
        ),
        EmbeddingTargetKind::PolicyAudit => conn.query_row(
            &{
                let projection = log::policy_audit_projection_expr("pa");
                format!(
                    "SELECT
                        {projection}
                     FROM policy_audit pa
                     WHERE pa.id = ?1
                       AND (
                            pa.actor = ?2 OR
                            pa.session_id IN (SELECT id FROM sessions WHERE agent_id = ?2)
                       )"
                )
            },
            params![row_id, agent_id],
            |r| r.get::<_, String>(0),
        ),
        EmbeddingTargetKind::KnowledgeChunks => conn.query_row(
            "SELECT content FROM chunks WHERE id = ?1",
            params![row_id],
            |r| r.get::<_, String>(0),
        ),
    }
    .ok();
    Ok(out)
}

pub fn queue_stats(conn: &Connection) -> Result<EmbeddingQueueStats> {
    let total = conn
        .query_row("SELECT COUNT(*) FROM embedding_jobs", [], |r| {
            r.get::<_, i64>(0)
        })
        .unwrap_or(0);
    let pending = conn
        .query_row(
            "SELECT COUNT(*) FROM embedding_jobs WHERE status = 'pending'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0);
    let retry = conn
        .query_row(
            "SELECT COUNT(*) FROM embedding_jobs WHERE status = 'retry'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0);
    let processing = conn
        .query_row(
            "SELECT COUNT(*) FROM embedding_jobs WHERE status = 'processing'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0);
    let dead = conn
        .query_row(
            "SELECT COUNT(*) FROM embedding_jobs WHERE status = 'dead'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0);

    Ok(EmbeddingQueueStats {
        total,
        pending,
        retry,
        processing,
        dead,
    })
}

/// Queue drifted rows, claim due jobs, embed, persist metadata, and update
/// vector quantization indexes for the touched targets.
pub async fn process_embedding_jobs<F, Fut>(
    conn: &Connection,
    agent_id: &str,
    embedding_model_id: &str,
    max_jobs: usize,
    retry_base_seconds: i64,
    max_attempts: i64,
    mut embed: F,
) -> Result<EmbeddingRunStats>
where
    F: FnMut(String) -> Fut,
    Fut: Future<Output = Result<Vec<u8>>>,
{
    if max_jobs == 0 {
        return Ok(EmbeddingRunStats::default());
    }

    let mut stats = EmbeddingRunStats::default();
    stats.queued = enqueue_drift_jobs(
        conn,
        agent_id,
        embedding_model_id,
        max_jobs.saturating_mul(4),
    )?;

    let claimed = claim_due_jobs(conn, max_jobs, DEFAULT_CLAIM_LEASE_SECS)?;
    stats.claimed = claimed.len();
    if claimed.is_empty() {
        return Ok(stats);
    }

    let mut touched = HashSet::new();

    for job in claimed {
        let content = match fetch_content_for_target(conn, job.target, &job.row_id, agent_id)? {
            Some(c) => c,
            None => {
                mark_job_done(conn, job.target, &job.row_id);
                stats.skipped += 1;
                continue;
            }
        };

        let content_hash = content_sha256(&content);
        match embed(content).await {
            Ok(blob) => {
                match set_embedding_with_meta(
                    conn,
                    job.target,
                    &job.row_id,
                    &blob,
                    Some(embedding_model_id),
                    Some(&content_hash),
                ) {
                    Ok(_) => {
                        mark_job_done(conn, job.target, &job.row_id);
                        touched.insert(job.target);
                        stats.embedded += 1;
                    }
                    Err(e) => {
                        mark_job_failed(
                            conn,
                            job.target,
                            &job.row_id,
                            job.attempts,
                            max_attempts.max(1),
                            retry_base_seconds,
                            &e.to_string(),
                        );
                        stats.failed += 1;
                    }
                }
            }
            Err(e) => {
                mark_job_failed(
                    conn,
                    job.target,
                    &job.row_id,
                    job.attempts,
                    max_attempts.max(1),
                    retry_base_seconds,
                    &e.to_string(),
                );
                stats.failed += 1;
            }
        }
    }

    if !touched.is_empty() {
        rebuild_vector_indexes_for(conn, &touched);
    }

    Ok(stats)
}

/// Legacy one-shot helper retained for compatibility with existing call-sites.
/// New code should prefer `process_embedding_jobs`.
pub async fn embed_all_pending<F, Fut>(
    conn: &Connection,
    agent_id: &str,
    mut embed: F,
) -> Result<EmbeddingRunStats>
where
    F: FnMut(String) -> Fut,
    Fut: Future<Output = Result<Vec<u8>>>,
{
    let pending = collect_pending(conn, agent_id)?;
    let mut stats = EmbeddingRunStats::default();

    for row in pending {
        stats.claimed += 1;
        match embed(row.content).await {
            Ok(blob) => {
                if set_embedding(conn, row.target, &row.id, &blob).is_ok() {
                    stats.embedded += 1;
                } else {
                    stats.failed += 1;
                }
            }
            Err(_) => {
                stats.failed += 1;
            }
        }
    }

    if stats.embedded > 0 {
        rebuild_vector_indexes(conn);
    }
    Ok(stats)
}
