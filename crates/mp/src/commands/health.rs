//! Health command — check gateway and agent status.

use anyhow::Result;
use mp_core::config::Config;

use crate::ui;

pub async fn run(ctx: &crate::CommandContext<'_>) -> Result<()> {
    let config = ctx.config;
    ui::banner();

    let meta_path = config.metadata_db_path();
    if meta_path.exists() {
        ui::success(format!(
            "Gateway: data dir exists at {}",
            config.data_dir.display()
        ));
    } else {
        ui::warn("Gateway: not initialized (run `mp init`)");
    }

    for agent in &config.agents {
        let db_path = config.agent_db_path(&agent.name);
        if db_path.exists() {
            let conn = mp_core::db::open(&db_path)?;
            let fact_count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM facts WHERE superseded_at IS NULL",
                    [],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            let session_count: i64 = conn
                .query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))
                .unwrap_or(0);
            let embed_queue = mp_core::store::embedding::queue_stats(&conn).unwrap_or_default();

            let metadata = std::fs::metadata(&db_path)?;
            let size_kb = metadata.len() / 1024;
            ui::info(format!(
                "Agent \"{}\": {size_kb} KB, {fact_count} facts, {session_count} sessions, embedding jobs total={} (pending={}, retry={}, processing={}, dead={})",
                agent.name,
                embed_queue.total,
                embed_queue.pending,
                embed_queue.retry,
                embed_queue.processing,
                embed_queue.dead,
            ));
        } else {
            ui::warn(format!("Agent \"{}\": not initialized", agent.name));
        }
    }

    ui::blank();
    Ok(())
}
