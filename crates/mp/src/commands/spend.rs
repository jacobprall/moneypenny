//! Spend command — show token/cost usage summary.

use anyhow::Result;
use mp_core::config::Config;

use crate::helpers::{open_agent_db, resolve_agent, op_request};
use crate::ui;

pub async fn run(config: &Config, agent: Option<String>, period: &str, group_by: &str) -> Result<()> {
    let ag = resolve_agent(config, agent.as_deref())?;
    let conn = open_agent_db(config, &ag.name)?;

    let req = op_request(
        &ag.name,
        "usage.summary",
        serde_json::json!({
            "period": period,
            "group_by": group_by,
        }),
    );
    let resp = mp_core::operations::execute(&conn, &req)?;

    if !resp.ok {
        ui::warn(format!("usage query failed: {}", resp.message));
        return Ok(());
    }

    let totals = &resp.data["totals"];
    let breakdown = resp.data["breakdown"].as_array();

    ui::banner();
    ui::info(format!(
        "Token spend ({period}, agent \"{}\")",
        ag.name
    ));
    ui::blank();

    ui::info(format!(
        "  Total tokens:  {}",
        totals["total_tokens"].as_i64().unwrap_or(0)
    ));
    ui::info(format!(
        "  Input tokens:  {}",
        totals["input_tokens"].as_i64().unwrap_or(0)
    ));
    ui::info(format!(
        "  Output tokens: {}",
        totals["output_tokens"].as_i64().unwrap_or(0)
    ));
    ui::info(format!(
        "  Cost:          ${:.4}",
        totals["cost_usd"].as_f64().unwrap_or(0.0)
    ));
    ui::info(format!(
        "  Events:        {}",
        totals["event_count"].as_i64().unwrap_or(0)
    ));
    ui::blank();

    if let Some(rows) = breakdown {
        if !rows.is_empty() {
            ui::info(format!("Breakdown by {group_by}:"));
            ui::blank();
            for row in rows {
                let key = row["key"].as_str().unwrap_or("-");
                let tokens = row["total_tokens"].as_i64().unwrap_or(0);
                let cost = row["cost_usd"].as_f64().unwrap_or(0.0);
                let count = row["count"].as_i64().unwrap_or(0);
                ui::info(format!(
                    "  {key:<40} {tokens:>10} tokens  ${cost:>8.4}  ({count} calls)"
                ));
            }
            ui::blank();
        }
    }

    Ok(())
}
