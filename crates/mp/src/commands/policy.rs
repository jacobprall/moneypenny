//! Policy command — list, add, test, violations, load.

use anyhow::Result;
use mp_core::config::Config;

use crate::cli;
use crate::helpers::{open_agent_db, op_request, parse_duration_hours, resolve_agent, toml_to_json};
use crate::ui;

pub async fn run(config: &Config, cmd: cli::PolicyCommand) -> Result<()> {
    let ag = resolve_agent(config, None)?;
    let conn = open_agent_db(config, &ag.name)?;

    match cmd {
        cli::PolicyCommand::List => {
            let mut stmt = conn.prepare(
                "SELECT id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, enabled
                 FROM policies ORDER BY priority DESC"
            )?;
            let policies: Vec<(
                String,
                String,
                i64,
                String,
                Option<String>,
                Option<String>,
                Option<String>,
                bool,
            )> = stmt
                .query_map([], |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                        r.get(6)?,
                        r.get::<_, i64>(7)? != 0,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;

            ui::blank();
            if policies.is_empty() {
                ui::info("No policies configured.");
            } else {
                ui::table_header(&[("ID", 36), ("NAME", 20), ("PRI", 4), ("EFFECT", 6), ("ACTOR", 10), ("ACTION", 10), ("RESOURCE", 15)]);
                for (id, name, pri, effect, actor, action, resource, _) in &policies {
                    println!(
                        "  {:36} {:20} {:4} {:6} {:10} {:10} {:15}",
                        id,
                        name,
                        pri,
                        effect,
                        actor.as_deref().unwrap_or("*"),
                        action.as_deref().unwrap_or("*"),
                        resource.as_deref().unwrap_or("*"),
                    );
                }
            }
            ui::blank();
        }
        cli::PolicyCommand::Add {
            name,
            effect,
            priority,
            actor,
            action,
            resource,
            argument,
            channel,
            sql,
            rule_type,
            rule_config,
            message,
        } => {
            let req = op_request(
                &ag.name,
                "policy.add",
                serde_json::json!({
                    "name": name,
                    "effect": effect,
                    "priority": priority,
                    "actor_pattern": actor,
                    "action_pattern": action,
                    "resource_pattern": resource,
                    "argument_pattern": argument,
                    "channel_pattern": channel,
                    "sql_pattern": sql,
                    "rule_type": rule_type,
                    "rule_config": rule_config,
                    "message": message,
                }),
            );
            let resp = mp_core::operations::execute(&conn, &req)?;
            if !resp.ok {
                ui::warn(format!("Policy add denied: {}", resp.message));
                return Ok(());
            }
            let id = resp.data["id"].as_str().unwrap_or("-");
            let printed_name = resp.data["name"].as_str().unwrap_or("policy");
            let pri = resp.data["priority"].as_i64().unwrap_or(0);
            ui::success(format!("Policy \"{printed_name}\" added ({id}, priority={pri})"));
        }
        cli::PolicyCommand::Test { input } => {
            let resource = format!("sql:{input}");
            let req = op_request(
                &ag.name,
                "policy.explain",
                serde_json::json!({
                    "actor": ag.name,
                    "action": "execute",
                    "resource": resource,
                    "sql_content": input
                }),
            );
            let resp = mp_core::operations::execute(&conn, &req)?;
            if !resp.ok {
                ui::warn(format!("Policy explain denied: {}", resp.message));
                return Ok(());
            }
            ui::field("Effect", 12, resp.data["effect"].as_str().unwrap_or("unknown"));
            if let Some(reason) = resp.data["reason"].as_str() {
                ui::field("Reason", 12, reason);
            }
            if let Some(policy_id) = resp.data["policy_id"].as_str() {
                ui::field("Policy ID", 12, policy_id);
            }
        }
        cli::PolicyCommand::Violations { last } => {
            let hours = parse_duration_hours(&last);
            let since = chrono::Utc::now().timestamp() - (hours * 3600);
            let req = op_request(
                &ag.name,
                "audit.query",
                serde_json::json!({
                    "effect": "denied",
                    "limit": 50
                }),
            );
            let resp = mp_core::operations::execute(&conn, &req)?;
            if !resp.ok {
                ui::warn(format!("Audit query denied: {}", resp.message));
                return Ok(());
            }
            let violations: Vec<serde_json::Value> = resp
                .data
                .as_array()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter(|v| v["created_at"].as_i64().unwrap_or(0) >= since)
                .collect();

            ui::blank();
            if violations.is_empty() {
                ui::info(format!("No policy violations in the last {last}."));
            } else {
                for v in &violations {
                    ui::info(format!(
                        "[{effect}] {actor} → {action} on {resource}: {}",
                        v["reason"].as_str().unwrap_or(""),
                        effect = v["effect"].as_str().unwrap_or(""),
                        actor = v["actor"].as_str().unwrap_or(""),
                        action = v["action"].as_str().unwrap_or(""),
                        resource = v["resource"].as_str().unwrap_or(""),
                    ));
                }
            }
            ui::blank();
        }
        cli::PolicyCommand::Load { file } => {
            let content = std::fs::read_to_string(&file)?;
            let policies: Vec<serde_json::Value> = if file.ends_with(".toml") {
                let table: toml::Value = toml::from_str(&content)?;
                match table.get("policies").and_then(|v| v.as_array()) {
                    Some(arr) => arr.iter().map(|v| toml_to_json(v)).collect(),
                    None => anyhow::bail!("TOML file must contain a [[policies]] array"),
                }
            } else {
                serde_json::from_str(&content)?
            };

            let mut loaded = 0;
            let mut errors = 0;
            for p in &policies {
                let name = match p["name"].as_str() {
                    Some(n) => n,
                    None => {
                        ui::warn("Skipping policy without 'name' field");
                        errors += 1;
                        continue;
                    }
                };
                let mut args = p.clone();
                if args.get("effect").is_none() {
                    args["effect"] = serde_json::json!("deny");
                }
                let req = op_request(&ag.name, "policy.add", args);
                match mp_core::operations::execute(&conn, &req) {
                    Ok(resp) if resp.ok => {
                        let id = resp.data["id"].as_str().unwrap_or("-");
                        ui::success(format!("Loaded policy \"{name}\" ({id})"));
                        loaded += 1;
                    }
                    Ok(resp) => {
                        ui::error(format!("Failed to load \"{name}\": {}", resp.message));
                        errors += 1;
                    }
                    Err(e) => {
                        ui::error(format!("Error loading \"{name}\": {e}"));
                        errors += 1;
                    }
                }
            }
            ui::blank();
            ui::dim(format!("Loaded {loaded} policies ({errors} errors) from {file}"));
        }
    }
    Ok(())
}
