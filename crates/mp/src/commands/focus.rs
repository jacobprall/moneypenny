//! Focus command — set, get, list, compose.

use anyhow::Result;
use mp_core::config::Config;

use crate::cli;
use crate::helpers::{open_agent_db, op_request, resolve_agent};
use crate::ui;

pub async fn run(ctx: &crate::CommandContext<'_>, cmd: cli::FocusCommand) -> Result<()> {
    let config = ctx.config;
    match cmd {
        cli::FocusCommand::Set {
            key,
            content,
            agent,
            session_id,
        } => {
            let ag = resolve_agent(config, agent.as_deref())?;
            let conn = open_agent_db(config, &ag.name)?;
            let sid = if let Some(s) = session_id {
                s
            } else {
                let req_resolve = op_request(
                    &ag.name,
                    "session.resolve",
                    serde_json::json!({ "agent_id": ag.name, "channel": "cli" }),
                );
                let resp = mp_core::operations::execute(&conn, &req_resolve)?;
                resp.data["session_id"]
                    .as_str()
                    .map(String::from)
                    .ok_or_else(|| anyhow::anyhow!("session_id required (could not resolve session)"))?
            };
            let req = op_request(
                &ag.name,
                "brain.focus.set",
                serde_json::json!({
                    "key": key,
                    "content": content,
                    "session_id": sid,
                }),
            );
            let resp = mp_core::operations::execute(&conn, &req)?;
            if !resp.ok {
                ui::warn(format!("Focus set denied: {}", resp.message));
                return Ok(());
            }
            ui::info(format!("Set focus[{key}]"));
        }
        cli::FocusCommand::Get { key, agent, session_id } => {
            let ag = resolve_agent(config, agent.as_deref())?;
            let conn = open_agent_db(config, &ag.name)?;
            let sid = if let Some(s) = session_id {
                s
            } else {
                let req_resolve = op_request(
                    &ag.name,
                    "session.resolve",
                    serde_json::json!({ "agent_id": ag.name, "channel": "cli" }),
                );
                let resp = mp_core::operations::execute(&conn, &req_resolve)?;
                resp.data["session_id"]
                    .as_str()
                    .map(String::from)
                    .ok_or_else(|| anyhow::anyhow!("session_id required (could not resolve session)"))?
            };
            let req = op_request(
                &ag.name,
                "brain.focus.get",
                serde_json::json!({
                    "key": key,
                    "session_id": sid,
                }),
            );
            let resp = mp_core::operations::execute(&conn, &req)?;
            if !resp.ok {
                ui::warn(format!("Focus get denied: {}", resp.message));
                return Ok(());
            }
            let content = resp.data["content"].as_str().unwrap_or("");
            println!("{content}");
        }
        cli::FocusCommand::List { agent, session_id } => {
            let ag = resolve_agent(config, agent.as_deref())?;
            let conn = open_agent_db(config, &ag.name)?;
            let sid = if let Some(s) = session_id {
                s
            } else {
                let req_resolve = op_request(
                    &ag.name,
                    "session.resolve",
                    serde_json::json!({ "agent_id": ag.name, "channel": "cli" }),
                );
                let resp = mp_core::operations::execute(&conn, &req_resolve)?;
                resp.data["session_id"]
                    .as_str()
                    .map(String::from)
                    .ok_or_else(|| anyhow::anyhow!("session_id required (could not resolve session)"))?
            };
            let req = op_request(
                &ag.name,
                "brain.focus.list",
                serde_json::json!({ "session_id": sid }),
            );
            let resp = mp_core::operations::execute(&conn, &req)?;
            if !resp.ok {
                ui::warn(format!("Focus list denied: {}", resp.message));
                return Ok(());
            }
            let entries = resp.data["entries"].as_array().cloned().unwrap_or_default();
            ui::blank();
            if entries.is_empty() {
                ui::info("No focus entries.");
            } else {
                for e in entries {
                    let k = e["key"].as_str().unwrap_or("-");
                    let preview: String = e["content"].as_str().unwrap_or("").chars().take(50).collect();
                    ui::hint(format!("  {k}: {preview}..."));
                }
            }
            ui::blank();
        }
        cli::FocusCommand::Compose {
            task_hint,
            max_tokens,
            agent,
            session_id,
        } => {
            let ag = resolve_agent(config, agent.as_deref())?;
            let conn = open_agent_db(config, &ag.name)?;
            let sid = if let Some(s) = session_id {
                s
            } else {
                let req_resolve = op_request(
                    &ag.name,
                    "session.resolve",
                    serde_json::json!({ "agent_id": ag.name, "channel": "cli" }),
                );
                let resp = mp_core::operations::execute(&conn, &req_resolve)?;
                resp.data["session_id"]
                    .as_str()
                    .map(String::from)
                    .ok_or_else(|| anyhow::anyhow!("session_id required (could not resolve session)"))?
            };
            let mut args = serde_json::json!({
                "brain_id": ag.name,
                "session_id": sid,
                "max_tokens": max_tokens,
            });
            if let Some(h) = task_hint {
                args["task_hint"] = serde_json::json!(h);
            }
            let req = op_request(&ag.name, "brain.focus.compose", args);
            let resp = mp_core::operations::execute(&conn, &req)?;
            if !resp.ok {
                ui::warn(format!("Compose denied: {}", resp.message));
                return Ok(());
            }
            let segments = resp.data["segments"].as_array().cloned().unwrap_or_default();
            ui::blank();
            ui::info(format!("Composed {} segments, {} tokens", segments.len(), resp.data["total_tokens"].as_i64().unwrap_or(0)));
            for s in segments {
                let label = s["label"].as_str().unwrap_or("-");
                let preview: String = s["content"].as_str().unwrap_or("").chars().take(80).collect();
                ui::hint(format!("  [{label}] {preview}..."));
            }
            ui::blank();
        }
    }
    Ok(())
}
