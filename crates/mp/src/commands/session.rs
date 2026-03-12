//! Session command — list sessions.

use anyhow::Result;

use crate::cli;
use crate::helpers::{open_agent_db, op_request, resolve_agent};
use crate::ui;

pub async fn run(ctx: &crate::context::CommandContext<'_>, cmd: cli::SessionCommand) -> Result<()> {
    let config = ctx.config;
    match cmd {
        cli::SessionCommand::List { agent, limit } => {
            let ag = resolve_agent(config, agent.as_deref())?;
            let conn = open_agent_db(config, &ag.name)?;
            let limit = limit.max(1).min(200);
            let req = op_request(
                &ag.name,
                "session.list",
                serde_json::json!({
                    "agent_id": ag.name,
                    "limit": limit
                }),
            );
            let resp = mp_core::operations::execute(&conn, &req)?;
            if !resp.ok {
                ui::warn(format!("Session list denied: {}", resp.message));
                return Ok(());
            }
            let rows = resp.data.as_array().cloned().unwrap_or_default();

            if rows.is_empty() {
                ui::info(format!("No sessions found for agent '{}'.", ag.name));
                return Ok(());
            }

            let fmt_ts = |ts: i64| -> String {
                chrono::DateTime::<chrono::Utc>::from_timestamp(ts, 0)
                    .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                    .unwrap_or_else(|| ts.to_string())
            };

            ui::blank();
            ui::info(format!("Recent sessions for agent '{}':", ag.name));
            ui::blank();
            for row in rows {
                let id = row["id"].as_str().unwrap_or("-");
                let channel = row["channel"].as_str().unwrap_or("unknown");
                let started_at = row["started_at"].as_i64().unwrap_or(0);
                let ended_at = row["ended_at"].as_i64();
                let message_count = row["message_count"].as_i64().unwrap_or(0);
                let last_activity = row["last_activity"].as_i64().unwrap_or(started_at);
                ui::info(format!("Session: {}", id));
                ui::hint(format!("Channel:       {}", channel));
                ui::hint(format!("Started:       {}", fmt_ts(started_at)));
                ui::hint(format!("Last activity: {}", fmt_ts(last_activity)));
                ui::hint(format!("Messages:      {}", message_count));
                ui::hint(format!(
                    "Ended:         {}",
                    ended_at.map(fmt_ts).unwrap_or_else(|| "active".into())
                ));
                ui::blank();
            }
        }
    }
    Ok(())
}
