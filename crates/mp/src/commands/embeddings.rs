//! Embeddings command — status, retry-dead, backfill.

use anyhow::Result;

use crate::cli;
use crate::helpers::{
    build_embedding_provider_with_override, normalize_embedding_target, open_agent_db,
    resolve_agent,
};
use crate::ui;

pub async fn run(ctx: &crate::context::CommandContext<'_>, cmd: cli::EmbeddingsCommand) -> Result<()> {
    let config = ctx.config;
    match cmd {
        cli::EmbeddingsCommand::Status { agent } => {
            let ag = resolve_agent(config, agent.as_deref())?;
            let conn = open_agent_db(config, &ag.name)?;
            let stats = mp_core::store::embedding::queue_stats(&conn)?;
            let by_target = mp_core::store::embedding::queue_target_stats(&conn)?;

            ui::blank();
            ui::info(format!("Embedding queue status (agent: {})", ag.name));
            ui::info(format!(
                "total={} pending={} retry={} processing={} dead={}",
                stats.total, stats.pending, stats.retry, stats.processing, stats.dead
            ));
            if by_target.is_empty() {
                ui::info("No queue entries.");
            } else {
                ui::blank();
                ui::table_header(&[("TARGET", 14), ("TOTAL", 7), ("PENDING", 7), ("RETRY", 7), ("PROCESSING", 10), ("DEAD", 7)]);
                for row in &by_target {
                    println!(
                        "  {:14} {:7} {:7} {:7} {:10} {:7}",
                        row.target, row.total, row.pending, row.retry, row.processing, row.dead
                    );
                }
            }
            ui::blank();
        }
        cli::EmbeddingsCommand::RetryDead {
            agent,
            target,
            limit,
        } => {
            let ag = resolve_agent(config, agent.as_deref())?;
            let conn = open_agent_db(config, &ag.name)?;
            let target_norm = if let Some(raw) = target.as_deref() {
                Some(normalize_embedding_target(raw).ok_or_else(|| {
                    anyhow::anyhow!(
                        "Unknown --target value. Use one of: facts, messages, tool_calls, policy_audit, chunks"
                    )
                })?)
            } else {
                None
            };

            let revived = mp_core::store::embedding::retry_dead_jobs(&conn, target_norm, limit)?;
            ui::success(format!(
                "Revived {revived} dead embedding job{} for agent \"{}\"{}.",
                if revived == 1 { "" } else { "s" },
                ag.name,
                target_norm
                    .map(|t| format!(" (target={t})"))
                    .unwrap_or_default()
            ));
        }
        cli::EmbeddingsCommand::Backfill {
            agent,
            model,
            limit,
            batch_size,
            enqueue_only,
        } => {
            let ag = resolve_agent(config, agent.as_deref())?;
            let conn = open_agent_db(config, &ag.name)?;

            let embed_provider =
                build_embedding_provider_with_override(config, ag, model.as_deref())?;
            let model_name = model.as_deref().unwrap_or(&ag.embedding.model).to_string();
            let model_id = mp_core::store::embedding::model_identity(
                &ag.embedding.provider,
                &model_name,
                ag.embedding.dimensions,
            );

            let queued =
                mp_core::store::embedding::enqueue_drift_jobs(&conn, &ag.name, &model_id, limit)?;
            ui::info(format!(
                "Enqueued {queued} backfill candidat{} for agent \"{}\" using model \"{}\".",
                if queued == 1 { "e" } else { "es" },
                ag.name,
                model_name
            ));

            if enqueue_only {
                return Ok(());
            }

            let mut total_embedded = 0usize;
            let mut total_failed = 0usize;
            let mut rounds = 0usize;
            let embed_provider_ref = embed_provider.as_ref();
            let spinner = ui::spinner("Processing embedding queue...");
            loop {
                rounds += 1;
                let stats = mp_core::store::embedding::process_embedding_jobs(
                    &conn,
                    &ag.name,
                    &model_id,
                    batch_size.max(1),
                    5,
                    8,
                    |content| async move {
                        let vec = embed_provider_ref.embed(&content).await?;
                        Ok::<Vec<u8>, anyhow::Error>(mp_llm::f32_slice_to_blob(&vec))
                    },
                )
                .await?;

                total_embedded += stats.embedded;
                total_failed += stats.failed;

                if stats.claimed == 0 {
                    break;
                }
                if rounds >= 10_000 {
                    break;
                }
            }
            spinner.finish_and_clear();

            let queue = mp_core::store::embedding::queue_stats(&conn)?;
            ui::success(format!(
                "Backfill run complete: embedded={}, failed={}, queue pending={} retry={} processing={} dead={}.",
                total_embedded,
                total_failed,
                queue.pending,
                queue.retry,
                queue.processing,
                queue.dead
            ));
        }
    }
    Ok(())
}
