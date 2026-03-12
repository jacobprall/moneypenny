pub async fn embed_pending(
    conn: &rusqlite::Connection,
    embed: &dyn mp_llm::provider::EmbeddingProvider,
    agent_id: &str,
    embedding_model_id: &str,
) {
    let stats = match mp_core::store::embedding::process_embedding_jobs(
        conn,
        agent_id,
        embedding_model_id,
        128,
        5,
        8,
        |content| async move {
            let vec = embed.embed(&content).await?;
            Ok::<Vec<u8>, anyhow::Error>(mp_llm::f32_slice_to_blob(&vec))
        },
    )
    .await
    {
        Ok(stats) => stats,
        Err(e) => {
            tracing::debug!("embed_pending: pending query failed: {e}");
            return;
        }
    };

    if stats.failed > 0 {
        tracing::debug!(failed = stats.failed, "some embeddings failed");
    }
    if stats.embedded > 0 || stats.queued > 0 || stats.claimed > 0 {
        tracing::debug!(
            queued = stats.queued,
            claimed = stats.claimed,
            embedded = stats.embedded,
            failed = stats.failed,
            skipped = stats.skipped,
            "embedding queue processed"
        );
    }
}
