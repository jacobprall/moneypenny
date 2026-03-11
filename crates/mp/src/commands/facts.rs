//! Facts command — list, search, inspect, expand, reset-compaction, delete.

use anyhow::Result;
use mp_core::config::Config;

use crate::cli;
use crate::helpers::{open_agent_db, op_request, resolve_agent};
use crate::ui;

pub async fn run(config: &Config, cmd: cli::FactsCommand) -> Result<()> {
    match cmd {
        cli::FactsCommand::List { agent } => {
            let ag = resolve_agent(config, agent.as_deref())?;
            let conn = open_agent_db(config, &ag.name)?;
            let facts = mp_core::store::facts::list_active(&conn, &ag.name)?;

            ui::blank();
            if facts.is_empty() {
                ui::info(format!("No facts found for agent \"{}\".", ag.name));
            } else {
                ui::table_header(&[("ID", 36), ("CONF", 6), ("CMPCT", 6), ("POINTER", 50)]);
                for f in &facts {
                    println!(
                        "  {:36} {:<6.1} {:<6} {}",
                        f.id, f.confidence, f.compaction_level, f.pointer
                    );
                }
                ui::blank();
                ui::dim(format!("{} active facts", facts.len()));
            }
            ui::blank();
        }
        cli::FactsCommand::Search { query, agent } => {
            let ag = resolve_agent(config, agent.as_deref())?;
            let conn = open_agent_db(config, &ag.name)?;
            let req = op_request(
                &ag.name,
                "memory.search",
                serde_json::json!({
                    "query": query,
                    "agent_id": ag.name,
                    "limit": 20
                }),
            );
            let resp = mp_core::operations::execute(&conn, &req)?;
            if !resp.ok {
                ui::warn(format!("Memory search denied: {}", resp.message));
                return Ok(());
            }
            let results = resp.data.as_array().cloned().unwrap_or_default();

            ui::blank();
            if results.is_empty() {
                ui::info(format!("No results for \"{query}\"."));
            } else {
                for r in &results {
                    let preview: String = r["content"]
                        .as_str()
                        .unwrap_or("")
                        .chars()
                        .take(80)
                        .collect();
                    println!(
                        "  [{}] {:.4}  {}",
                        r["store"].as_str().unwrap_or("-"),
                        r["score"].as_f64().unwrap_or(0.0),
                        preview
                    );
                }
                ui::blank();
                ui::dim(format!("{} results", results.len()));
            }
            ui::blank();
        }
        cli::FactsCommand::Inspect { id } => {
            let ag = resolve_agent(config, None)?;
            let conn = open_agent_db(config, &ag.name)?;
            let req = op_request(
                &ag.name,
                "memory.fact.get",
                serde_json::json!({ "id": id.clone() }),
            );
            let resp = mp_core::operations::execute(&conn, &req)?;
            if !resp.ok {
                ui::warn(&resp.message);
                return Ok(());
            }

            ui::blank();
            ui::field("ID", 12, resp.data["id"].as_str().unwrap_or("-"));
            ui::field("Pointer", 12, resp.data["pointer"].as_str().unwrap_or("-"));
            ui::field("Summary", 12, resp.data["summary"].as_str().unwrap_or("-"));
            ui::field("Confidence", 12, format!("{:.1}", resp.data["confidence"].as_f64().unwrap_or(0.0)));
            ui::field("Version", 12, resp.data["version"].as_i64().unwrap_or(1));
            ui::field("Compact Lv", 12, resp.data["compaction_level"].as_i64().unwrap_or(0));
            if let Some(compact) = resp.data["context_compact"].as_str() {
                ui::field("Compact", 12, compact);
            }
            ui::blank();
            ui::info("Content:");
            ui::info(resp.data["content"].as_str().unwrap_or(""));
            ui::blank();

            let audit = mp_core::store::facts::get_audit(&conn, &id)?;
            if !audit.is_empty() {
                ui::info("Audit trail:");
                for a in &audit {
                    ui::hint(format!(
                        "{} — {}",
                        a.operation,
                        a.reason.as_deref().unwrap_or("")
                    ));
                }
            }
            ui::blank();
        }
        cli::FactsCommand::Expand { id } => {
            let ag = resolve_agent(config, None)?;
            let conn = open_agent_db(config, &ag.name)?;
            let req = op_request(&ag.name, "memory.fact.get", serde_json::json!({ "id": id }));
            let resp = mp_core::operations::execute(&conn, &req)?;
            if !resp.ok {
                ui::warn(&resp.message);
                return Ok(());
            }
            ui::blank();
            ui::info(format!(
                "[{}] {}",
                resp.data["id"].as_str().unwrap_or("-"),
                resp.data["pointer"].as_str().unwrap_or("-")
            ));
            ui::info("Full content:");
            ui::info(resp.data["content"].as_str().unwrap_or(""));
            ui::blank();
        }
        cli::FactsCommand::ResetCompaction {
            id,
            all,
            agent,
            confirm,
        } => {
            let ag = resolve_agent(config, agent.as_deref())?;
            let conn = open_agent_db(config, &ag.name)?;

            if all {
                if !confirm {
                    ui::info("Use --confirm with --all to reset compaction for every active fact.");
                    return Ok(());
                }
                let facts = mp_core::store::facts::list_active(&conn, &ag.name)?;
                if facts.is_empty() {
                    ui::info(format!("No active facts found for agent \"{}\".", ag.name));
                    return Ok(());
                }

                let mut reset_count = 0usize;
                for f in &facts {
                    let req = op_request(
                        &ag.name,
                        "memory.fact.compaction.reset",
                        serde_json::json!({
                            "id": f.id,
                            "reason": "bulk compaction reset via CLI",
                        }),
                    );
                    let resp = mp_core::operations::execute(&conn, &req)?;
                    if resp.ok {
                        reset_count += 1;
                    }
                }
                ui::success(format!(
                    "Reset compaction for {reset_count}/{} facts.",
                    facts.len()
                ));
                return Ok(());
            }

            let fact_id = match id {
                Some(v) => v,
                None => {
                    ui::info("Provide a fact ID, or use --all --confirm.");
                    return Ok(());
                }
            };

            let req = op_request(
                &ag.name,
                "memory.fact.compaction.reset",
                serde_json::json!({
                    "id": fact_id,
                    "reason": "compaction reset via CLI",
                }),
            );
            let resp = mp_core::operations::execute(&conn, &req)?;
            if !resp.ok {
                ui::warn(format!("Fact reset failed: {}", resp.message));
                return Ok(());
            }
            ui::success(format!(
                "Fact {} compaction reset.",
                resp.data["id"].as_str().unwrap_or("-")
            ));
        }
        cli::FactsCommand::Promote { id, scope } => {
            ui::info(format!("[mp facts promote {id} --scope {scope} — requires sync (M13)]"));
        }
        cli::FactsCommand::Delete { id, confirm } => {
            let ag = resolve_agent(config, None)?;
            let conn = open_agent_db(config, &ag.name)?;

            if !confirm {
                ui::info(format!("Use --confirm to delete fact {id}"));
            } else {
                let req = op_request(
                    &ag.name,
                    "fact.delete",
                    serde_json::json!({
                        "id": id,
                        "reason": "deleted via CLI"
                    }),
                );
                let resp = mp_core::operations::execute(&conn, &req)?;
                if !resp.ok {
                    ui::warn(format!("Fact delete failed: {}", resp.message));
                    return Ok(());
                }
                ui::success(format!(
                    "Fact {} deleted.",
                    resp.data["id"].as_str().unwrap_or("-")
                ));
            }
        }
    }
    Ok(())
}
