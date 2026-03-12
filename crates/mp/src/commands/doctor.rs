//! Doctor command — diagnose Moneypenny setup.

use anyhow::Result;
use mp_core::config::Config;
use std::path::Path;

use crate::helpers::op_request;
use crate::ui;

pub async fn run(ctx: &crate::CommandContext<'_>) -> Result<()> {
    let config = ctx.config;
    let config_path = ctx.config_path;
    ui::banner();
    ui::info("Moneypenny doctor checks");
    ui::blank();

    let mut warnings = 0usize;

    if config_path.exists() {
        ui::success(format!("Config file found: {}", config_path.display()));
    } else {
        warnings += 1;
        ui::warn(format!(
            "Config file missing: {} (run `mp init`)",
            config_path.display()
        ));
    }

    if config.agents.is_empty() {
        warnings += 1;
        ui::warn("No agents configured in moneypenny.toml.");
    }

    let meta_path = config.metadata_db_path();
    if meta_path.exists() {
        match mp_core::db::open(&meta_path).and_then(|c| mp_core::schema::init_metadata_db(&c)) {
            Ok(()) => ui::success(format!("Metadata DB OK: {}", meta_path.display())),
            Err(e) => {
                warnings += 1;
                ui::warn(format!("Metadata DB error at {}: {e}", meta_path.display()));
            }
        }
    } else {
        warnings += 1;
        ui::warn(format!(
            "Metadata DB missing at {} (run `mp init`)",
            meta_path.display()
        ));
    }

    for agent in &config.agents {
        let db_path = config.agent_db_path(&agent.name);
        if !db_path.exists() {
            warnings += 1;
            ui::warn(format!(
                "Agent \"{}\" DB missing at {}",
                agent.name,
                db_path.display()
            ));
            continue;
        }

        match mp_core::db::open(&db_path) {
            Ok(conn) => {
                if let Err(e) = mp_core::schema::init_agent_db(&conn) {
                    warnings += 1;
                    ui::warn(format!("Agent \"{}\" schema error: {e}", agent.name));
                    continue;
                }
                let facts: i64 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM facts WHERE superseded_at IS NULL",
                        [],
                        |r| r.get(0),
                    )
                    .unwrap_or(0);
                let sessions: i64 = conn
                    .query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))
                    .unwrap_or(0);
                ui::success(format!(
                    "Agent \"{}\" DB OK (facts={}, sessions={})",
                    agent.name, facts, sessions
                ));
            }
            Err(e) => {
                warnings += 1;
                ui::warn(format!("Agent \"{}\" DB open failed: {e}", agent.name));
            }
        }

        let model_path = agent.embedding.resolve_model_path(&config.models_dir());
        if model_path.exists() {
            ui::info(format!(
                "Embedding model present for \"{}\": {}",
                agent.name,
                model_path.display()
            ));
        } else {
            warnings += 1;
            ui::warn(format!(
                "Embedding model missing for \"{}\": {} (run `mp setup models`)",
                agent.name,
                model_path.display()
            ));
        }
    }

    let project_cursor_mcp = std::env::current_dir()?.join(".cursor/mcp.json");
    let project_claude_mcp = std::env::current_dir()?.join(".mcp.json");
    let project_cortex_mcp = std::env::current_dir()?.join(".cortex/mcp.json");
    let user_cortex_mcp = std::env::var("HOME")
        .map(|h| std::path::PathBuf::from(h).join(".snowflake/cortex/mcp.json"))
        .unwrap_or_default();
    if project_cursor_mcp.exists() {
        ui::success(format!("Cursor MCP config found: {}", project_cursor_mcp.display()));
    }
    if project_claude_mcp.exists() {
        ui::success(format!(
            "Claude Code MCP config found: {}",
            project_claude_mcp.display()
        ));
    }
    if project_cortex_mcp.exists() {
        ui::success(format!("Cortex Code MCP config found: {}", project_cortex_mcp.display()));
    } else if user_cortex_mcp.exists() {
        let has_mp = std::fs::read_to_string(&user_cortex_mcp)
            .map(|c| c.contains("moneypenny"))
            .unwrap_or(false);
        if has_mp {
            ui::success(format!("Cortex Code MCP config found (user): {}", user_cortex_mcp.display()));
        }
    }
    if !project_cursor_mcp.exists() && !project_claude_mcp.exists() && !project_cortex_mcp.exists() {
        warnings += 1;
        ui::warn("No local MCP config found in this project.");
        ui::hint("Run one of:");
        ui::hint("- mp setup cursor --local");
        ui::hint("- mp setup claude-code");
        ui::hint("- mp setup cortex");
    }

    if let Some(agent) = config.agents.first() {
        let db_path = config.agent_db_path(&agent.name);
        if db_path.exists() {
            if let Ok(conn) = mp_core::db::open(&db_path) {
                let _ = mp_core::schema::init_agent_db(&conn);
                let req = op_request(
                    &agent.name,
                    "memory.search",
                    serde_json::json!({ "query": "test", "limit": 3 }),
                );
                match mp_core::operations::execute(&conn, &req) {
                    Ok(resp) if resp.ok => {
                        let count = resp.data.as_array().map(|a| a.len()).unwrap_or(0);
                        ui::success(format!(
                            "Verify query OK — memory.search returned {count} result(s)."
                        ));
                    }
                    Ok(resp) => {
                        warnings += 1;
                        ui::warn(format!(
                            "Verify query failed: {} ({})",
                            resp.message, resp.code
                        ));
                        if resp.code == "policy_denied" {
                            ui::hint(
                                "This usually means no allow policy exists. Run `mp init` to seed bootstrap policies.",
                            );
                        }
                    }
                    Err(e) => {
                        warnings += 1;
                        ui::warn(format!("Verify query error: {e}"));
                    }
                }
            }
        }
    }

    ui::blank();
    if warnings == 0 {
        ui::success("All doctor checks passed. Moneypenny is ready.");
    } else {
        ui::warn(format!("Doctor completed with {warnings} warning(s)."));
        ui::hint("Fix the warnings above, then rerun `mp doctor`.");
    }
    ui::blank();
    Ok(())
}
