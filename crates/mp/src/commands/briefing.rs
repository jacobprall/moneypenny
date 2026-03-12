//! Briefing command — session recap with recent activity, facts, denials, and spend.

use anyhow::Result;
use mp_core::config::Config;

use crate::helpers::{open_agent_db, resolve_agent, op_request};
use crate::ui;

pub async fn run(ctx: &crate::CommandContext<'_>, agent: Option<String>) -> Result<()> {
    let config = ctx.config;
    let ag = resolve_agent(config, agent.as_deref())?;
    let conn = open_agent_db(config, &ag.name)?;

    let req = op_request(&ag.name, "briefing.compose", serde_json::json!({}));
    let resp = mp_core::operations::execute(&conn, &req)?;

    if !resp.ok {
        ui::warn(format!("briefing failed: {}", resp.message));
        return Ok(());
    }

    ui::banner();

    if let Some(text) = resp.data["text"].as_str() {
        println!("{text}");
    }

    Ok(())
}
