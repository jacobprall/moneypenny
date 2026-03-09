use anyhow::Result;
use rusqlite::Connection;
use std::future::Future;

use super::{facts, knowledge, log};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbeddingTargetKind {
    Facts,
    Messages,
    ToolCalls,
    PolicyAudit,
    KnowledgeChunks,
}

#[derive(Debug, Clone)]
pub struct PendingEmbedding {
    pub id: String,
    pub content: String,
    pub target: EmbeddingTargetKind,
}

pub trait EmbeddingStore {
    fn target() -> EmbeddingTargetKind;
    fn pending(conn: &Connection, agent_id: &str) -> Result<Vec<(String, String)>>;
    fn set(conn: &Connection, id: &str, blob: &[u8]) -> Result<()>;
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
                rusqlite::params![id],
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

    fn vector_index() -> (&'static str, &'static str) {
        ("chunks", "content_embedding")
    }
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

pub fn rebuild_vector_indexes(conn: &Connection) {
    for (table, col) in vector_indexes() {
        let _ = conn.execute(
            "SELECT vector_quantize(?1, ?2)",
            rusqlite::params![table, col],
        );
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmbeddingRunStats {
    pub embedded: usize,
    pub failed: usize,
}

/// Embed and persist all pending rows across supported stores.
///
/// The provided closure receives raw content and returns a FLOAT32 blob.
/// Failures are counted and the loop continues for other rows.
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

    let mut embedded = 0usize;
    let mut failed = 0usize;

    for row in pending {
        match embed(row.content).await {
            Ok(blob) => {
                if set_embedding(conn, row.target, &row.id, &blob).is_ok() {
                    embedded += 1;
                } else {
                    failed += 1;
                }
            }
            Err(_) => {
                failed += 1;
            }
        }
    }

    if embedded > 0 {
        rebuild_vector_indexes(conn);
    }

    Ok(EmbeddingRunStats { embedded, failed })
}

