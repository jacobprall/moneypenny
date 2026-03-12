//! Experience command — search, stats.

use anyhow::Result;

use crate::cli;
use crate::helpers::{open_agent_db, op_request, resolve_agent};
use crate::ui;

pub async fn run(ctx: &crate::context::CommandContext<'_>, cmd: cli::ExperienceCommand) -> Result<()> {
    let config = ctx.config;
    match cmd {
        cli::ExperienceCommand::Search { query, limit, agent } => {
            let ag = resolve_agent(config, agent.as_deref())?;
            let conn = open_agent_db(config, &ag.name)?;
            let req = op_request(
                &ag.name,
                "brain.memories.experience.search",
                serde_json::json!({
                    "brain_id": ag.name,
                    "query": query,
                    "limit": limit,
                }),
            );
            let resp = mp_core::operations::execute(&conn, &req)?;
            if !resp.ok {
                ui::warn(format!("Experience search denied: {}", resp.message));
                return Ok(());
            }
            let cases = resp.data["cases"].as_array().cloned().unwrap_or_default();
            ui::blank();
            if cases.is_empty() {
                ui::info(format!("No experience priors found for \"{query}\"."));
            } else {
            for c in cases {
                let id = c["case_id"].as_str().unwrap_or("-");
                let case_type = c["type"].as_str().unwrap_or("-");
                let status = c["status"].as_str().unwrap_or("-");
                let context_preview: String = c["context"].as_str().unwrap_or("").chars().take(60).collect();
                ui::hint(format!("[{id}] {case_type} ({status}): {context_preview}..."));
            }
            }
            ui::blank();
        }
        cli::ExperienceCommand::Stats { agent } => {
            let ag = resolve_agent(config, agent.as_deref())?;
            let conn = open_agent_db(config, &ag.name)?;
            let req = op_request(
                &ag.name,
                "brain.memories.experience.stats",
                serde_json::json!({ "brain_id": ag.name }),
            );
            let resp = mp_core::operations::execute(&conn, &req)?;
            if !resp.ok {
                ui::warn(format!("Experience stats denied: {}", resp.message));
                return Ok(());
            }
            let rows = resp.data["by_type_status"].as_array().cloned().unwrap_or_default();
            ui::blank();
            ui::info("Experience stats:");
            for r in rows {
                let t = r["type"].as_str().unwrap_or("-");
                let status = r["status"].as_str().unwrap_or("-");
                let count = r["count"].as_i64().unwrap_or(0);
                let hits = r["total_hits"].as_i64().unwrap_or(0);
                ui::hint(format!("  {t} / {status}: {count} cases, {hits} hits"));
            }
            ui::blank();
        }
    }
    Ok(())
}
