//! Brain command — list, checkpoint, restore, export.

use anyhow::Result;
use mp_core::config::Config;

use crate::cli;
use crate::helpers::{open_agent_db, op_request, resolve_agent};
use crate::ui;

pub async fn run(config: &Config, cmd: cli::BrainCommand) -> Result<()> {
    match cmd {
        cli::BrainCommand::List { agent } => {
            let ag = resolve_agent(config, agent.as_deref())?;
            let conn = open_agent_db(config, &ag.name)?;
            let req = op_request(&ag.name, "brain.list", serde_json::json!({ "agent_id": ag.name }));
            let resp = mp_core::operations::execute(&conn, &req)?;
            if !resp.ok {
                ui::warn(format!("Brain list denied: {}", resp.message));
                return Ok(());
            }
            let brains = resp.data["brains"].as_array().cloned().unwrap_or_default();
            ui::blank();
            if brains.is_empty() {
                ui::info(format!("No brains found for agent '{}'.", ag.name));
            } else {
                ui::info(format!("Brains for agent '{}':", ag.name));
                for b in brains {
                    ui::hint(format!("  {} — {}", b["brain_id"].as_str().unwrap_or("-"), b["name"].as_str().unwrap_or("-")));
                }
            }
            ui::blank();
        }
        cli::BrainCommand::Checkpoint { name, output, agent } => {
            let ag = resolve_agent(config, agent.as_deref())?;
            let conn = open_agent_db(config, &ag.name)?;
            let db_path = config.agent_db_path(&ag.name);
            let req = op_request(
                &ag.name,
                "brain.checkpoint",
                serde_json::json!({
                    "brain_id": ag.name,
                    "name": name,
                    "output_path": output,
                    "agent_db_path": db_path.to_string_lossy(),
                }),
            );
            let resp = mp_core::operations::execute(&conn, &req)?;
            if !resp.ok {
                ui::warn(format!("Checkpoint failed: {}", resp.message));
                return Ok(());
            }
            ui::info(format!("Checkpoint '{}' written to {}", name, output));
        }
        cli::BrainCommand::Restore {
            path,
            checkpoint_id,
            agent,
            confirm,
        } => {
            if !confirm {
                ui::warn("Restore requires --confirm (destructive operation)");
                return Ok(());
            }
            let ag = resolve_agent(config, agent.as_deref())?;
            let conn = open_agent_db(config, &ag.name)?;
            let db_path = config.agent_db_path(&ag.name).to_string_lossy().to_string();
            let mut args = serde_json::json!({ "agent_db_path": db_path, "mode": "replace" });
            if let Some(p) = path {
                args["checkpoint_path"] = serde_json::json!(p);
            } else if let Some(id) = checkpoint_id {
                args["checkpoint_id"] = serde_json::json!(id);
            } else {
                ui::warn("Restore requires --path or --checkpoint-id");
                return Ok(());
            }
            let req = op_request(&ag.name, "brain.restore", args);
            let resp = mp_core::operations::execute(&conn, &req)?;
            if !resp.ok {
                ui::warn(format!("Restore failed: {}", resp.message));
                return Ok(());
            }
            ui::info(resp.message);
        }
        cli::BrainCommand::Export { output, agent } => {
            let ag = resolve_agent(config, agent.as_deref())?;
            let conn = open_agent_db(config, &ag.name)?;
            let mut args = serde_json::json!({ "brain_id": ag.name, "format": "json" });
            if let Some(ref o) = output {
                args["output_path"] = serde_json::json!(o);
            }
            let req = op_request(&ag.name, "brain.export", args);
            let resp = mp_core::operations::execute(&conn, &req)?;
            if !resp.ok {
                ui::warn(format!("Export failed: {}", resp.message));
                return Ok(());
            }
            if output.as_ref().is_none() {
                println!("{}", serde_json::to_string_pretty(&resp.data)?);
            } else {
                ui::info(resp.message);
            }
        }
    }
    Ok(())
}
