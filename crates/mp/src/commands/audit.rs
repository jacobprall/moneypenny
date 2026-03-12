//! Audit command — query and export policy audit.

use anyhow::Result;

use crate::cli;
use crate::helpers::{csv_escape, open_agent_db, op_request, resolve_agent, sql_quote};
use crate::ui;

pub async fn run(
    ctx: &crate::context::CommandContext<'_>,
    _agent: Option<String>,
    command: Option<cli::AuditCommand>,
) -> Result<()> {
    let config = ctx.config;
    let ag = resolve_agent(config, None)?;
    let conn = open_agent_db(config, &ag.name)?;

    match command {
        None => {
            let req = op_request(
                &ag.name,
                "audit.query",
                serde_json::json!({
                    "limit": 20
                }),
            );
            let resp = mp_core::operations::execute(&conn, &req)?;
            if !resp.ok {
                ui::warn(format!("Audit query denied: {}", resp.message));
                return Ok(());
            }
            let entries = resp.data.as_array().cloned().unwrap_or_default();

            ui::blank();
            if entries.is_empty() {
                ui::info("No audit entries.");
            } else {
                for e in &entries {
                    ui::info(format!(
                        "[{effect}] {actor} → {action} on {resource}: {}",
                        e["reason"].as_str().unwrap_or(""),
                        effect = e["effect"].as_str().unwrap_or(""),
                        actor = e["actor"].as_str().unwrap_or(""),
                        action = e["action"].as_str().unwrap_or(""),
                        resource = e["resource"].as_str().unwrap_or(""),
                    ));
                }
            }
            ui::blank();
        }
        Some(cli::AuditCommand::Search { query, since, until }) => {
            let req = op_request(
                &ag.name,
                "audit.query",
                serde_json::json!({
                    "query": query,
                    "since": since,
                    "until": until,
                    "limit": 20
                }),
            );
            let resp = mp_core::operations::execute(&conn, &req)?;
            if !resp.ok {
                ui::warn(format!("Audit query denied: {}", resp.message));
                return Ok(());
            }
            let entries = resp.data.as_array().cloned().unwrap_or_default();

            ui::blank();
            for e in &entries {
                ui::info(format!(
                    "[{effect}] {actor} → {action} on {resource}: {}",
                    e["reason"].as_str().unwrap_or(""),
                    effect = e["effect"].as_str().unwrap_or(""),
                    actor = e["actor"].as_str().unwrap_or(""),
                    action = e["action"].as_str().unwrap_or(""),
                    resource = e["resource"].as_str().unwrap_or(""),
                ));
            }
            ui::blank();
        }
        Some(cli::AuditCommand::Export {
            format,
            since,
            until,
        }) => {
            let req = op_request(
                &ag.name,
                "audit.query",
                serde_json::json!({
                    "since": since,
                    "until": until,
                    "limit": 10000
                }),
            );
            let resp = mp_core::operations::execute(&conn, &req)?;
            if !resp.ok {
                anyhow::bail!("Audit export denied: {}", resp.message);
            }
            let entries = resp.data.as_array().cloned().unwrap_or_default();

            match format.as_str() {
                "json" => {
                    println!("{}", serde_json::to_string_pretty(&entries)?);
                }
                "csv" => {
                    println!(
                        "id,actor,action,resource,effect,reason,session_id,created_at,correlation_id"
                    );
                    for e in &entries {
                        println!(
                            "{},{},{},{},{},{},{},{},{}",
                            csv_escape(e["id"].as_str().unwrap_or("")),
                            csv_escape(e["actor"].as_str().unwrap_or("")),
                            csv_escape(e["action"].as_str().unwrap_or("")),
                            csv_escape(e["resource"].as_str().unwrap_or("")),
                            csv_escape(e["effect"].as_str().unwrap_or("")),
                            csv_escape(e["reason"].as_str().unwrap_or("")),
                            csv_escape(e["session_id"].as_str().unwrap_or("")),
                            e["created_at"].as_i64().unwrap_or(0),
                            csv_escape(e["correlation_id"].as_str().unwrap_or("")),
                        );
                    }
                }
                "sql" => {
                    for e in &entries {
                        println!(
                            "INSERT INTO policy_audit (id, actor, action, resource, effect, reason, session_id, created_at, correlation_id) VALUES ({}, {}, {}, {}, {}, {}, {}, {}, {});",
                            sql_quote(e["id"].as_str().unwrap_or("")),
                            sql_quote(e["actor"].as_str().unwrap_or("")),
                            sql_quote(e["action"].as_str().unwrap_or("")),
                            sql_quote(e["resource"].as_str().unwrap_or("")),
                            sql_quote(e["effect"].as_str().unwrap_or("")),
                            sql_quote(e["reason"].as_str().unwrap_or("")),
                            sql_quote(e["session_id"].as_str().unwrap_or("")),
                            e["created_at"].as_i64().unwrap_or(0),
                            sql_quote(e["correlation_id"].as_str().unwrap_or("")),
                        );
                    }
                }
                other => {
                    anyhow::bail!("Unsupported export format: {other}. Use json, csv, or sql.");
                }
            }
        }
    }
    Ok(())
}
