//! Fleet command — init, push-policy, audit, list, status, tag.

use anyhow::Result;
use mp_core::config::Config;

use crate::cli;
use crate::helpers::{csv_escape, open_agent_db, op_request, resolve_agent};
use crate::ui;

pub async fn run(config: &Config, cmd: cli::FleetCommand) -> Result<()> {
    match cmd {
        cli::FleetCommand::Init {
            template,
            scope,
            dry_run,
        } => {
            let actor = resolve_agent(config, None)?;
            let actor_conn = open_agent_db(config, &actor.name)?;
            let tpl = mp_core::fleet::load_fleet_template(&template)?;
            if tpl.agents.is_empty() {
                anyhow::bail!("template has no agents");
            }

            let scoped_agents: Vec<_> = tpl
                .agents
                .into_iter()
                .filter(|a| {
                    mp_core::fleet::matches_scope(
                        Some(&mp_core::fleet::tags_to_csv(&a.tags)),
                        scope.as_deref(),
                    )
                })
                .collect();
            if scoped_agents.is_empty() {
                ui::warn("No template agents matched --scope filter.");
                return Ok(());
            }

            ui::info(format!("Fleet init: {} agent(s)", scoped_agents.len()));
            for agent_tpl in scoped_agents {
                if dry_run {
                    ui::hint(format!(
                        "plan create: {} tags={} policies={} tools={} facts={} knowledge={}",
                        agent_tpl.name,
                        mp_core::fleet::tags_to_csv(&agent_tpl.tags),
                        agent_tpl.policies.len(),
                        agent_tpl.tools.len(),
                        agent_tpl.seed_facts.len(),
                        agent_tpl.seed_knowledge.len(),
                    ));
                    continue;
                }

                let create_resp = mp_core::operations::execute(
                    &actor_conn,
                    &op_request(
                        &actor.name,
                        "agent.create",
                        serde_json::json!({
                            "name": agent_tpl.name,
                            "metadata_db_path": config.metadata_db_path().to_string_lossy().to_string(),
                            "agent_db_path": config.agent_db_path(&agent_tpl.name).to_string_lossy().to_string(),
                            "trust_level": agent_tpl.trust_level,
                            "llm_provider": agent_tpl.llm_provider,
                            "llm_model": agent_tpl.llm_model,
                            "persona": agent_tpl.persona,
                            "tags": mp_core::fleet::tags_to_csv(&agent_tpl.tags),
                        }),
                    ),
                )?;
                if !create_resp.ok && create_resp.code != "already_exists" {
                    ui::error(format!("agent {} create failed: {}", agent_tpl.name, create_resp.message));
                    continue;
                }

                let target_conn = open_agent_db(config, &agent_tpl.name)?;

                for p in &agent_tpl.policies {
                    let resp = mp_core::operations::execute(
                        &target_conn,
                        &op_request(&agent_tpl.name, "policy.add", p.clone()),
                    )?;
                    if !resp.ok {
                        ui::warn(format!("policy add skipped on {}: {}", agent_tpl.name, resp.message));
                    }
                }

                for tool in &agent_tpl.tools {
                    let resp = mp_core::operations::execute(
                        &target_conn,
                        &op_request(
                            &agent_tpl.name,
                            "js_tool.add",
                            serde_json::json!({
                                "name": tool.name,
                                "description": tool.description,
                                "script": tool.script,
                            }),
                        ),
                    )?;
                    if !resp.ok {
                        ui::warn(format!("tool add skipped on {}: {}", agent_tpl.name, resp.message));
                    }
                }

                for fact in &agent_tpl.seed_facts {
                    let resp = mp_core::operations::execute(
                        &target_conn,
                        &op_request(
                            &agent_tpl.name,
                            "memory.fact.add",
                            serde_json::json!({
                                "content": fact.content,
                                "summary": fact.summary.clone().unwrap_or_else(|| fact.content.clone()),
                                "pointer": fact.pointer.clone().unwrap_or_else(|| fact.content.clone()),
                                "keywords": fact.keywords,
                                "scope": fact.scope,
                            }),
                        ),
                    )?;
                    if !resp.ok {
                        ui::warn(format!("fact seed skipped on {}: {}", agent_tpl.name, resp.message));
                    }
                }

                for doc in &agent_tpl.seed_knowledge {
                    let resp = mp_core::operations::execute(
                        &target_conn,
                        &op_request(
                            &agent_tpl.name,
                            "knowledge.ingest",
                            serde_json::json!({
                                "title": doc.title,
                                "path": doc.path,
                                "content": doc.content,
                                "metadata": doc.metadata,
                                "scope": doc.scope,
                            }),
                        ),
                    )?;
                    if !resp.ok {
                        ui::warn(format!(
                            "knowledge seed skipped on {}: {}",
                            agent_tpl.name, resp.message
                        ));
                    }
                }

                ui::success(format!("fleet initialized agent {}", agent_tpl.name));
            }
        }
        cli::FleetCommand::PushPolicy {
            file,
            scope,
            rollback_file,
            dry_run,
        } => {
            let actor = resolve_agent(config, None)?;
            let bundle = mp_core::fleet::load_policy_bundle(&file)?;
            mp_core::fleet::verify_policy_bundle_signature(&bundle)?;

            let targets = fleet_target_agents(config, scope.as_deref())?;
            if targets.is_empty() {
                ui::warn("No agents matched for policy push.");
                return Ok(());
            }

            let mut rollback = serde_json::Map::new();

            for a in &targets {
                let conn = mp_core::db::open(std::path::Path::new(&a.db_path))?;
                mp_core::schema::init_agent_db(&conn)?;
                if rollback_file.is_some() {
                    let existing = export_policies_json(&conn)?;
                    rollback.insert(a.name.clone(), serde_json::Value::Array(existing));
                }
                if dry_run {
                    ui::hint(format!(
                        "plan push-policy: {} policies -> {}",
                        bundle.policies.len(),
                        a.name
                    ));
                    continue;
                }
                for policy in &bundle.policies {
                    let resp = mp_core::operations::execute(
                        &conn,
                        &op_request(&actor.name, "policy.add", policy.clone()),
                    )?;
                    if !resp.ok {
                        ui::warn(format!("policy push denied on {}: {}", a.name, resp.message));
                    }
                }
                ui::success(format!("policy bundle pushed to {}", a.name));
            }

            if let Some(path) = rollback_file {
                let payload = serde_json::Value::Object(rollback);
                std::fs::write(&path, serde_json::to_string_pretty(&payload)?)?;
                ui::info(format!("rollback snapshot written to {}", path));
            }
        }
        cli::FleetCommand::Audit {
            scope,
            since,
            until,
            format,
            limit,
        } => {
            let actor = resolve_agent(config, None)?;
            let targets = fleet_target_agents(config, scope.as_deref())?;
            let mut rows = Vec::new();
            for a in &targets {
                let conn = mp_core::db::open(std::path::Path::new(&a.db_path))?;
                mp_core::schema::init_agent_db(&conn)?;
                let resp = mp_core::operations::execute(
                    &conn,
                    &op_request(
                        &actor.name,
                        "audit.query",
                        serde_json::json!({
                            "since": since,
                            "until": until,
                            "limit": limit,
                        }),
                    ),
                )?;
                if !resp.ok {
                    ui::warn(format!("fleet audit denied on {}: {}", a.name, resp.message));
                    continue;
                }
                let mut entries = resp.data.as_array().cloned().unwrap_or_default();
                for e in &mut entries {
                    if let Some(obj) = e.as_object_mut() {
                        obj.insert("agent".to_string(), serde_json::Value::String(a.name.clone()));
                    }
                }
                rows.extend(entries);
            }

            if format.eq_ignore_ascii_case("csv") {
                println!("agent,id,actor,action,resource,effect,reason,session_id,created_at,correlation_id");
                for e in &rows {
                    println!(
                        "{},{},{},{},{},{},{},{},{},{}",
                        csv_escape(e["agent"].as_str().unwrap_or("")),
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
            } else {
                println!("{}", serde_json::to_string_pretty(&rows)?);
            }
        }
        cli::FleetCommand::List { scope } => {
            let targets = fleet_target_agents(config, scope.as_deref())?;
            ui::blank();
            ui::table_header(&[
                ("NAME", 20),
                ("TRUST", 12),
                ("LLM", 12),
                ("TAGS", 30),
                ("SYNC", 6),
            ]);
            for a in targets {
                println!(
                    "  {:20} {:12} {:12} {:30} {:6}",
                    a.name,
                    a.trust_level,
                    a.llm_provider,
                    a.tags.unwrap_or_default(),
                    if a.sync_enabled { "yes" } else { "no" }
                );
            }
            ui::blank();
        }
        cli::FleetCommand::Status { scope } => {
            let targets = fleet_target_agents(config, scope.as_deref())?;
            let sync_tables: Vec<&str> = config.sync.tables.iter().map(String::as_str).collect();
            ui::blank();
            ui::table_header(&[
                ("AGENT", 18),
                ("FACTS", 8),
                ("SESS", 8),
                ("JOBS", 8),
                ("SYNCv", 8),
                ("DRIFT", 8),
            ]);
            for a in &targets {
                let conn = mp_core::db::open(std::path::Path::new(&a.db_path))?;
                mp_core::schema::init_agent_db(&conn)?;
                let health = mp_core::observability::agent_health(&conn, &a.name, &a.db_path)?;
                let jobs = mp_core::observability::jobs_health(&conn)?;
                let sync = mp_core::sync::status(&conn, &sync_tables)?;
                let drift = fleet_agent_drift(config, a);
                println!(
                    "  {:18} {:8} {:8} {:8} {:8} {:8}",
                    a.name,
                    health.facts,
                    health.sessions,
                    jobs.active,
                    sync.db_version,
                    if drift { "yes" } else { "no" }
                );
            }
            ui::blank();
        }
        cli::FleetCommand::Tag { agent, tags } => {
            let meta_path = config.metadata_db_path();
            let meta_conn = mp_core::db::open(&meta_path)?;
            mp_core::schema::init_metadata_db(&meta_conn)?;
            let tags_csv = mp_core::fleet::tags_to_csv(&mp_core::fleet::parse_tags_csv(&tags));
            let updated = meta_conn.execute(
                "UPDATE agents SET tags = ?1 WHERE name = ?2",
                rusqlite::params![tags_csv, agent],
            )?;
            if updated == 0 {
                anyhow::bail!("agent '{}' not found in metadata registry", agent);
            }
            ui::success(format!("updated tags for {}", agent));
        }
    }
    Ok(())
}

fn fleet_target_agents(
    config: &Config,
    scope: Option<&str>,
) -> anyhow::Result<Vec<mp_core::gateway::AgentEntry>> {
    let meta_path = config.metadata_db_path();
    let meta_conn = mp_core::db::open(&meta_path)?;
    mp_core::schema::init_metadata_db(&meta_conn)?;
    let agents = mp_core::gateway::list_agents(&meta_conn)?;
    Ok(agents
        .into_iter()
        .filter(|a| mp_core::fleet::matches_scope(a.tags.as_deref(), scope))
        .collect())
}

fn fleet_agent_drift(config: &Config, runtime: &mp_core::gateway::AgentEntry) -> bool {
    match config.agents.iter().find(|a| a.name == runtime.name) {
        Some(cfg) => {
            runtime.trust_level != cfg.trust_level
                || runtime.llm_provider != cfg.llm.provider
                || runtime.llm_model.as_deref().unwrap_or("")
                    != cfg.llm.model.as_deref().unwrap_or("")
        }
        None => true,
    }
}

fn export_policies_json(conn: &rusqlite::Connection) -> anyhow::Result<Vec<serde_json::Value>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, priority, effect, actor_pattern, action_pattern, resource_pattern,
                argument_pattern, channel_pattern, sql_pattern, rule_type, rule_config, message,
                enabled, created_at
         FROM policies
         ORDER BY priority DESC, created_at ASC",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok(serde_json::json!({
                "id": r.get::<_, String>(0)?,
                "name": r.get::<_, String>(1)?,
                "priority": r.get::<_, i64>(2)?,
                "effect": r.get::<_, String>(3)?,
                "actor_pattern": r.get::<_, Option<String>>(4)?,
                "action_pattern": r.get::<_, Option<String>>(5)?,
                "resource_pattern": r.get::<_, Option<String>>(6)?,
                "argument_pattern": r.get::<_, Option<String>>(7)?,
                "channel_pattern": r.get::<_, Option<String>>(8)?,
                "sql_pattern": r.get::<_, Option<String>>(9)?,
                "rule_type": r.get::<_, Option<String>>(10)?,
                "rule_config": r.get::<_, Option<String>>(11)?,
                "message": r.get::<_, Option<String>>(12)?,
                "enabled": r.get::<_, i64>(13)?,
                "created_at": r.get::<_, i64>(14)?,
            }))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}
