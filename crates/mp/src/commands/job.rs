//! Job command — list, create, run, pause, history.

use anyhow::Result;
use mp_core::config::Config;

use crate::cli;
use crate::helpers::{open_agent_db, op_request, resolve_agent};
use crate::ui;

pub async fn run(ctx: &crate::CommandContext<'_>, cmd: cli::JobCommand) -> Result<()> {
    let config = ctx.config;
    let ag = resolve_agent(config, None)?;
    let conn = open_agent_db(config, &ag.name)?;

    match cmd {
        cli::JobCommand::List { agent } => {
            let req = op_request(
                &ag.name,
                "job.list",
                serde_json::json!({
                    "agent_id": agent,
                }),
            );
            let resp = mp_core::operations::execute(&conn, &req)?;
            if !resp.ok {
                ui::warn(format!("Job list denied: {}", resp.message));
                return Ok(());
            }
            let jobs: Vec<serde_json::Value> =
                serde_json::from_value(resp.data).unwrap_or_default();
            ui::blank();
            if jobs.is_empty() {
                ui::info("No jobs scheduled.");
            } else {
                ui::table_header(&[("ID", 36), ("NAME", 20), ("TYPE", 8), ("STATUS", 10), ("SCHED", 8)]);
                for j in &jobs {
                    println!(
                        "  {:36} {:20} {:8} {:10} {:8}",
                        j["id"].as_str().unwrap_or("-"),
                        j["name"].as_str().unwrap_or("-"),
                        j["job_type"].as_str().unwrap_or("-"),
                        j["status"].as_str().unwrap_or("-"),
                        j["schedule"].as_str().unwrap_or("-")
                    );
                }
            }
            ui::blank();
        }
        cli::JobCommand::Create {
            name,
            schedule,
            job_type,
            payload,
            agent,
        } => {
            let req = op_request(
                &ag.name,
                "job.create",
                serde_json::json!({
                    "name": name,
                    "schedule": schedule,
                    "job_type": job_type,
                    "payload": payload,
                    "agent_id": agent,
                }),
            );
            let resp = mp_core::operations::execute(&conn, &req)?;
            if !resp.ok {
                ui::warn(format!("Job create denied: {}", resp.message));
                return Ok(());
            }
            let id = resp.data["id"].as_str().unwrap_or("-");
            let printed_name = resp.data["name"].as_str().unwrap_or("job");
            ui::success(format!("Job \"{printed_name}\" created ({id})"));
        }
        cli::JobCommand::Run { id } => {
            let req = op_request(&ag.name, "job.run", serde_json::json!({ "id": id }));
            let resp = mp_core::operations::execute(&conn, &req)?;
            if !resp.ok {
                ui::warn(format!("Job run failed: {}", resp.message));
                return Ok(());
            }
            ui::info(format!(
                "Run {}: {}",
                resp.data["run_id"].as_str().unwrap_or("-"),
                resp.data["status"].as_str().unwrap_or("-")
            ));
            if let Some(result) = resp.data["result"].as_str() {
                ui::info(format!("Result: {result}"));
            }
        }
        cli::JobCommand::Pause { id } => {
            let req = op_request(&ag.name, "job.pause", serde_json::json!({ "id": id }));
            let resp = mp_core::operations::execute(&conn, &req)?;
            if !resp.ok {
                ui::warn(format!("Job pause failed: {}", resp.message));
                return Ok(());
            }
            ui::success(format!("Job {} paused.", resp.data["id"].as_str().unwrap_or("-")));
        }
        cli::JobCommand::History { id } => {
            let mut args = serde_json::json!({ "limit": 20 });
            if let Some(ref job_id) = id {
                args["id"] = serde_json::json!(job_id);
            }
            let req = op_request(&ag.name, "job.history", args);
            let resp = mp_core::operations::execute(&conn, &req)?;
            if !resp.ok {
                ui::warn(format!("Job history denied: {}", resp.message));
                return Ok(());
            }
            let runs = resp.data.as_array().cloned().unwrap_or_default();
            ui::blank();
            if runs.is_empty() {
                ui::info("No job runs found.");
            } else {
                for r in &runs {
                    ui::info(format!(
                        "{}  job:{}  {}  {}",
                        r["id"].as_str().unwrap_or("-"),
                        r["job_id"].as_str().unwrap_or("-"),
                        r["status"].as_str().unwrap_or("-"),
                        r["result"].as_str().unwrap_or("-"),
                    ));
                }
            }
            ui::blank();
        }
    }
    Ok(())
}
