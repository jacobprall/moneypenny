//! Agent command — list, create, delete, status, config.

use anyhow::Result;

use crate::cli;
use crate::helpers::{open_agent_db, op_request, resolve_agent};

pub async fn run(ctx: &crate::context::CommandContext<'_>, cmd: cli::AgentCommand) -> Result<()> {
    let config = ctx.config;
    match cmd {
        cli::AgentCommand::List => {
            println!();
            println!(
                "  {:20} {:15} {:15} {:10}",
                "NAME", "TRUST", "LLM", "SOURCE"
            );
            println!(
                "  {:20} {:15} {:15} {:10}",
                "----", "-----", "---", "------"
            );
            let mut listed: std::collections::HashSet<String> = std::collections::HashSet::new();
            for agent in &config.agents {
                println!(
                    "  {:20} {:15} {:15} {:10}",
                    agent.name, agent.trust_level, agent.llm.provider, "config"
                );
                listed.insert(agent.name.clone());
            }
            let meta_path = config.metadata_db_path();
            if meta_path.exists() {
                if let Ok(meta_conn) = mp_core::db::open(&meta_path) {
                    if let Ok(db_agents) = mp_core::gateway::list_agents(&meta_conn) {
                        for a in db_agents {
                            if !listed.contains(&a.name) {
                                println!(
                                    "  {:20} {:15} {:15} {:10}",
                                    a.name, a.trust_level, a.llm_provider, "runtime"
                                );
                                listed.insert(a.name.clone());
                            }
                        }
                    }
                }
            }
            println!();
        }
        cli::AgentCommand::Create { name } => {
            let actor = resolve_agent(config, None)?;
            let conn = open_agent_db(config, &actor.name)?;
            let req = op_request(
                &actor.name,
                "agent.create",
                serde_json::json!({
                    "name": name,
                    "metadata_db_path": config.metadata_db_path().to_string_lossy().to_string(),
                    "agent_db_path": config.agent_db_path(&name).to_string_lossy().to_string(),
                    "trust_level": "standard",
                    "llm_provider": "local"
                }),
            );
            let resp = mp_core::operations::execute(&conn, &req)?;
            if !resp.ok {
                println!("  Agent create failed: {}", resp.message);
                return Ok(());
            }
            println!(
                "  Agent {} created.",
                resp.data["name"].as_str().unwrap_or("-")
            );
        }
        cli::AgentCommand::Delete { name, confirm } => {
            if !confirm {
                println!("  Use --confirm to delete agent {name}");
                return Ok(());
            }
            let actor = resolve_agent(config, None)?;
            let conn = open_agent_db(config, &actor.name)?;
            let req = op_request(
                &actor.name,
                "agent.delete",
                serde_json::json!({
                    "name": name,
                    "metadata_db_path": config.metadata_db_path().to_string_lossy().to_string(),
                }),
            );
            let resp = mp_core::operations::execute(&conn, &req)?;
            if !resp.ok {
                println!("  Agent delete failed: {}", resp.message);
                return Ok(());
            }
            println!(
                "  Agent {} deleted.",
                resp.data["name"].as_str().unwrap_or("-")
            );
        }
        cli::AgentCommand::Status { name } => {
            let agent = resolve_agent(config, name.as_deref())?;
            let conn = open_agent_db(config, &agent.name)?;

            let fact_count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM facts WHERE superseded_at IS NULL",
                    [],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            let session_count: i64 = conn
                .query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))
                .unwrap_or(0);
            let doc_count: i64 = conn
                .query_row("SELECT COUNT(*) FROM documents", [], |r| r.get(0))
                .unwrap_or(0);
            let skill_count: i64 = conn
                .query_row("SELECT COUNT(*) FROM skills", [], |r| r.get(0))
                .unwrap_or(0);

            println!();
            println!("  Agent: {}", agent.name);
            println!("  Trust: {}", agent.trust_level);
            println!(
                "  LLM:       {} ({})",
                agent.llm.provider,
                agent.llm.model.as_deref().unwrap_or("default")
            );
            println!(
                "  Embedding: {} ({}, {}D)",
                agent.embedding.provider, agent.embedding.model, agent.embedding.dimensions
            );
            println!();
            println!("  Facts:     {fact_count}");
            println!("  Sessions:  {session_count}");
            println!("  Documents: {doc_count}");
            println!("  Skills:    {skill_count}");
            println!();
        }
        cli::AgentCommand::Config { name, key, value } => {
            let actor = resolve_agent(config, None)?;
            let conn = open_agent_db(config, &actor.name)?;
            let req = op_request(
                &actor.name,
                "agent.config",
                serde_json::json!({
                    "name": name,
                    "key": key,
                    "value": value,
                    "metadata_db_path": config.metadata_db_path().to_string_lossy().to_string(),
                }),
            );
            let resp = mp_core::operations::execute(&conn, &req)?;
            if !resp.ok {
                println!("  Agent config failed: {}", resp.message);
                return Ok(());
            }
            println!(
                "  Agent {} config updated: {}={}",
                resp.data["name"].as_str().unwrap_or("-"),
                resp.data["key"].as_str().unwrap_or("-"),
                resp.data["value"].as_str().unwrap_or("-"),
            );
        }
    }
    Ok(())
}
