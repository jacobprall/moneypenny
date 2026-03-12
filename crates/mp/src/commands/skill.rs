//! Skill command — add, list, promote skills.

use anyhow::Result;
use std::path::Path;

use crate::cli;
use crate::helpers::{open_agent_db, op_request, resolve_agent};
use crate::ui;

pub async fn run(ctx: &crate::context::CommandContext<'_>, cmd: cli::SkillCommand) -> Result<()> {
    let config = ctx.config;
    let ag = resolve_agent(config, None)?;
    let conn = open_agent_db(config, &ag.name)?;

    match cmd {
        cli::SkillCommand::Add { path, .. } => {
            let content = std::fs::read_to_string(&path)?;
            let name = Path::new(&path)
                .file_stem()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "unnamed".into());
            let req = op_request(
                &ag.name,
                "skill.add",
                serde_json::json!({
                    "name": name,
                    "description": format!("Skill from {path}"),
                    "content": content
                }),
            );
            let resp = mp_core::operations::execute(&conn, &req)?;
            if !resp.ok {
                ui::warn(format!("Skill add denied: {}", resp.message));
                return Ok(());
            }
            let id = resp.data["id"].as_str().unwrap_or("-");
            let printed_name = resp.data["name"].as_str().unwrap_or("skill");
            ui::success(format!("Added skill \"{printed_name}\" ({id})"));
        }
        cli::SkillCommand::List { .. } => {
            let mut stmt = conn.prepare(
                "SELECT id, name, usage_count, success_rate, promoted FROM skills ORDER BY usage_count DESC"
            )?;
            let skills: Vec<(String, String, i64, Option<f64>, bool)> = stmt
                .query_map([], |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get::<_, i64>(4)? != 0,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;

            ui::blank();
            if skills.is_empty() {
                ui::info("No skills registered.");
            } else {
                ui::table_header(&[("ID", 36), ("NAME", 20), ("USES", 6), ("RATE", 8), ("PROMO", 8)]);
                for (id, name, uses, rate, promoted) in &skills {
                    let rate_str = rate
                        .map(|r| format!("{:.0}%", r * 100.0))
                        .unwrap_or("-".into());
                    println!(
                        "  {:36} {:20} {:6} {:8} {:8}",
                        id,
                        name,
                        uses,
                        rate_str,
                        if *promoted { "yes" } else { "" }
                    );
                }
            }
            ui::blank();
        }
        cli::SkillCommand::Promote { id } => {
            let req = op_request(&ag.name, "skill.promote", serde_json::json!({ "id": id }));
            let resp = mp_core::operations::execute(&conn, &req)?;
            if !resp.ok {
                ui::warn(format!("Skill promote failed: {}", resp.message));
                return Ok(());
            }
            ui::success(format!(
                "Skill {} promoted.",
                resp.data["id"].as_str().unwrap_or("-")
            ));
        }
    }
    Ok(())
}
