//! Setup command — MCP registration for AI coding agents.

use anyhow::Result;
use mp_core::config::Config;
use std::path::Path;

use crate::cli;
use crate::docs::{generate_agent_instructions, generate_claude_md, generate_cortex_skill};
use crate::helpers::{
    ensure_embedding_models, open_agent_db, resolve_agent, seed_bootstrap_facts,
};
use crate::ui;

pub async fn run(config: &Config, config_path: &Path, cmd: cli::SetupCommand) -> Result<()> {
    match &cmd {
        cli::SetupCommand::Models => {
            ui::blank();
            ui::info("Checking embedding models...");
            ui::blank();
            ensure_embedding_models(config).await;
            ui::blank();
            return Ok(());
        }
        cli::SetupCommand::Seed => {
            ui::blank();
            for agent in &config.agents {
                let conn = open_agent_db(config, &agent.name)?;
                let existing: i64 = conn.query_row(
                    "SELECT COUNT(*) FROM facts WHERE agent_id = ?1 AND confidence = 1.0 \
                     AND id IN (SELECT fact_id FROM fact_audit WHERE reason = 'bootstrap')",
                    rusqlite::params![agent.name],
                    |r| r.get(0),
                ).unwrap_or(0);
                if existing >= 4 {
                    ui::success(format!("{}: bootstrap facts already present ({existing})", agent.name));
                    continue;
                }
                seed_bootstrap_facts(&conn, &agent.name);
                ui::success(format!("{}: seeded bootstrap facts", agent.name));
            }
            ui::blank();
            return Ok(());
        }
        cli::SetupCommand::ClaudeCode { agent, scope } => {
            let ag = resolve_agent(config, agent.as_deref())?;
            let project_dir = std::env::current_dir()?;
            let data_dir = project_dir.join("mp-data");
            let config_abs = std::fs::canonicalize(config_path)
                .unwrap_or_else(|_| project_dir.join(config_path));
            let mp_binary = std::env::current_exe()?
                .canonicalize()
                .unwrap_or_else(|_| std::env::current_exe().unwrap());
            let mp_bin_str = mp_binary.to_string_lossy().to_string();

            let mcp_entry = serde_json::json!({
                "command": &mp_bin_str,
                "args": [
                    "--config", config_abs.to_string_lossy().as_ref(),
                    "serve", "--agent", &ag.name
                ],
                "type": "stdio"
            });

            let mcp_path = if scope == "user" {
                let home = std::env::var("HOME")
                    .or_else(|_| std::env::var("USERPROFILE"))
                    .unwrap_or_else(|_| "~".to_string());
                std::path::PathBuf::from(home).join(".claude.json")
            } else {
                project_dir.join(".mcp.json")
            };

            upsert_json_mcp_config(&mcp_path, "moneypenny", mcp_entry)?;

            let claude_md_path = project_dir.join("CLAUDE.md");
            let agent_conn = mp_core::db::open(&config.agent_db_path(&ag.name))
                .ok()
                .and_then(|c| mp_core::schema::init_agent_db(&c).ok().map(|_| c));
            let claude_md_content = generate_claude_md(agent_conn.as_ref());
            if claude_md_path.exists() {
                let existing = std::fs::read_to_string(&claude_md_path)?;
                if existing.contains("## Moneypenny") {
                    let start = existing.find("## Moneypenny").unwrap();
                    let before = &existing[..start];
                    let after_start = start + "## Moneypenny".len();
                    let after = existing[after_start..]
                        .find("\n## ")
                        .map(|pos| &existing[after_start + pos..])
                        .unwrap_or("");
                    std::fs::write(&claude_md_path, format!("{before}{claude_md_content}{after}"))?;
                    ui::success(format!("Updated Moneypenny instructions in {}", claude_md_path.display()));
                } else {
                    std::fs::write(&claude_md_path, format!("{existing}\n\n{claude_md_content}"))?;
                    ui::success(format!("Appended Moneypenny instructions to {}", claude_md_path.display()));
                }
            } else {
                std::fs::write(&claude_md_path, claude_md_content)?;
                ui::success(format!("Wrote agent instructions to {}", claude_md_path.display()));
            }

            std::fs::create_dir_all(&data_dir)?;

            ui::banner();
            ui::success(format!("Registered MCP server in {}", mcp_path.display()));
            ui::blank();
            ui::field("Scope", 9, scope);
            ui::field("Agent", 9, &ag.name);
            ui::field("Binary", 9, &mp_bin_str);
            ui::field("Data", 9, data_dir.display());
            ui::field("Project", 9, project_dir.display());
            ui::blank();
            ui::info("What's configured:");
            ui::hint("- MCP tools: moneypenny_facts, moneypenny_knowledge, moneypenny_policy, moneypenny_activity, moneypenny_execute");
            ui::hint("- CLAUDE.md: agent instructions for using Moneypenny");
            ui::blank();
            ui::info("Next steps:");
            ui::hint("1. Start Claude Code in this project directory");
            ui::hint("2. Ask: \"What Moneypenny tools do you have?\"");
            ui::blank();
            ui::info("CLI agent (same database, same agent):");
            ui::hint("mp chat                            # interactive terminal chat");
            ui::hint("mp send main \"remember X\"          # one-shot message");
            ui::blank();

            return Ok(());
        }
        cli::SetupCommand::Cortex { agent, scope } => {
            let ag = resolve_agent(config, agent.as_deref())?;
            let project_dir = std::env::current_dir()?;
            let data_dir = project_dir.join("mp-data");
            let config_abs = std::fs::canonicalize(config_path)
                .unwrap_or_else(|_| project_dir.join(config_path));
            let mp_binary = std::env::current_exe()?
                .canonicalize()
                .unwrap_or_else(|_| std::env::current_exe().unwrap());
            let mp_bin_str = mp_binary.to_string_lossy().to_string();

            let mcp_entry = serde_json::json!({
                "type": "stdio",
                "command": &mp_bin_str,
                "args": [
                    "--config", config_abs.to_string_lossy().as_ref(),
                    "sidecar", "--agent", &ag.name
                ],
                "env": {
                    "RUST_LOG": "error"
                }
            });

            let mcp_path = if scope == "user" {
                let home = std::env::var("HOME")
                    .or_else(|_| std::env::var("USERPROFILE"))
                    .unwrap_or_else(|_| "~".to_string());
                std::path::PathBuf::from(home).join(".snowflake/cortex/mcp.json")
            } else {
                project_dir.join(".cortex").join("mcp.json")
            };

            if let Some(parent) = mcp_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            upsert_json_mcp_config(&mcp_path, "moneypenny", mcp_entry)?;

            let skill_dir = if scope == "user" {
                let home = std::env::var("HOME")
                    .or_else(|_| std::env::var("USERPROFILE"))
                    .unwrap_or_else(|_| "~".to_string());
                std::path::PathBuf::from(home).join(".snowflake/cortex/skills/moneypenny")
            } else {
                project_dir.join(".cortex").join("skills").join("moneypenny")
            };
            std::fs::create_dir_all(&skill_dir)?;
            let skill_path = skill_dir.join("SKILL.md");
            let agent_conn = mp_core::db::open(&config.agent_db_path(&ag.name))
                .ok()
                .and_then(|c| mp_core::schema::init_agent_db(&c).ok().map(|_| c));
            std::fs::write(&skill_path, generate_cortex_skill(agent_conn.as_ref()))?;

            let hooks_config = generate_cortex_hooks_json(&mp_bin_str, &config_abs.to_string_lossy(), &ag.name);
            let hooks_path = if scope == "user" {
                let home = std::env::var("HOME")
                    .or_else(|_| std::env::var("USERPROFILE"))
                    .unwrap_or_else(|_| "~".to_string());
                std::path::PathBuf::from(home).join(".snowflake/cortex/hooks.json")
            } else {
                project_dir.join(".cortex").join("settings.local.json")
            };
            if let Some(parent) = hooks_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            upsert_json_hooks_config(&hooks_path, &hooks_config)?;

            std::fs::create_dir_all(&data_dir)?;

            ui::banner();
            ui::success(format!("Registered MCP server in {}", mcp_path.display()));
            ui::success(format!("Wrote agent skill to {}", skill_path.display()));
            ui::success(format!("Wrote hooks config to {}", hooks_path.display()));
            ui::blank();
            ui::field("Scope", 9, scope);
            ui::field("Agent", 9, &ag.name);
            ui::field("Binary", 9, &mp_bin_str);
            ui::field("Data", 9, data_dir.display());
            ui::field("Project", 9, project_dir.display());
            ui::blank();
            ui::info("What's configured:");
            ui::hint("- MCP tools: moneypenny_facts, moneypenny_knowledge, moneypenny_policy, moneypenny_activity, moneypenny_execute");
            ui::hint("- Hooks: audit trail + policy enforcement on every tool call");
            ui::hint("- Skill: instructs Cortex Code how to use Moneypenny");
            ui::blank();
            ui::info("Next steps:");
            ui::hint("1. Start Cortex Code in this project directory");
            ui::hint("2. Verify with: cortex mcp list");
            ui::hint("3. Ask: \"What Moneypenny tools do you have?\"");
            ui::blank();
            ui::info("CLI agent (same database, same agent):");
            ui::hint("mp chat                            # interactive terminal chat");
            ui::hint("mp send main \"remember X\"          # one-shot message");
            ui::blank();

            return Ok(());
        }
        _ => {}
    }
    let cli::SetupCommand::Cursor { agent, local, image } = &cmd else {
        unreachable!()
    };
    let ag = resolve_agent(config, agent.as_deref())?;
    let project_dir = std::env::current_dir()?;
    let data_dir = project_dir.join("mp-data");
    let config_abs = std::fs::canonicalize(config_path)
        .unwrap_or_else(|_| project_dir.join(config_path));

    let (mcp_server_entry, mode_label, hooks_config) = if *local {
        let mp_binary = std::env::current_exe()?
            .canonicalize()
            .unwrap_or_else(|_| std::env::current_exe().unwrap());
        let mp_bin_str = mp_binary.to_string_lossy().to_string();

        let entry = serde_json::json!({
            "command": &mp_bin_str,
            "args": [
                "--config", config_abs.to_string_lossy().as_ref(),
                "serve", "--agent", &ag.name
            ]
        });
        let hooks = generate_hooks_json(&mp_bin_str, &config_abs.to_string_lossy(), &ag.name);
        (entry, format!("local ({mp_bin_str})"), hooks)
    } else {
        let data_mount = format!("{}:/data", data_dir.display());
        let config_mount = format!("{}:/app/moneypenny.toml:ro", config_abs.display());
        let gateway_port = config.gateway.port;
        let entry = serde_json::json!({
            "command": "docker",
            "args": [
                "run", "-i", "--rm",
                "-p", format!("{gateway_port}:{gateway_port}"),
                "-v", &data_mount,
                "-v", &config_mount,
                "-e", "ANTHROPIC_API_KEY",
                &image,
                "serve", "--agent", &ag.name
            ]
        });
        let hooks = generate_hooks_json_docker(image, &data_mount, &config_mount, &ag.name);
        (entry, format!("docker ({image})"), hooks)
    };

    let mcp_path = project_dir.join(".cursor").join("mcp.json");
    if let Some(parent) = mcp_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    upsert_json_mcp_config(&mcp_path, "moneypenny", mcp_server_entry)?;

    let rules_dir = project_dir.join(".cursor").join("rules");
    std::fs::create_dir_all(&rules_dir)?;
    let rule_path = rules_dir.join("moneypenny.mdc");
    let cursor_agent_conn = mp_core::db::open(&config.agent_db_path(&ag.name))
        .ok()
        .and_then(|c| mp_core::schema::init_agent_db(&c).ok().map(|_| c));
    std::fs::write(&rule_path, generate_agent_instructions(cursor_agent_conn.as_ref()))?;

    let hooks_json_path = project_dir.join(".cursor").join("hooks.json");
    std::fs::write(&hooks_json_path, hooks_config)?;

    std::fs::create_dir_all(&data_dir)?;

    ui::banner();
    ui::success(format!("Registered MCP server in {}", mcp_path.display()));
    ui::success(format!("Wrote agent rules to {}", rule_path.display()));
    ui::success(format!("Wrote hooks config to {}", hooks_json_path.display()));
    ui::blank();
    ui::field("Mode", 9, &mode_label);
    ui::field("Agent", 9, &ag.name);
    ui::field("Data", 9, data_dir.display());
    ui::field("Project", 9, project_dir.display());
    if !local {
        ui::blank();
        ui::info("Docker quick start:");
        ui::hint(format!("docker build -t {image} ."));
        ui::hint(format!("docker run -it --rm -v {}:/data -e ANTHROPIC_API_KEY {image} init", data_dir.display()));
    }
    ui::blank();
    ui::info("What's configured:");
    ui::hint("- MCP tools: moneypenny_facts, moneypenny_knowledge, moneypenny_policy, moneypenny_activity, moneypenny_execute");
    ui::hint("- Hooks: audit trail + policy enforcement on every tool call, shell, file edit");
    ui::hint("- Agent rules: instructs Cursor how to use Moneypenny");
    ui::blank();
    ui::info("Next steps:");
    ui::hint("1. Restart Cursor (or reload the window)");
    ui::hint("2. Ask the agent: \"What Moneypenny tools do you have?\"");
    if *local {
        ui::blank();
        ui::info("CLI agent (same database, same agent):");
        ui::hint("mp chat                            # interactive terminal chat");
        ui::hint("mp send main \"remember X\"          # one-shot message");
    }
    ui::blank();

    Ok(())
}

fn generate_hooks_json(mp_bin: &str, config_path: &str, agent: &str) -> String {
    let cmd = |event: &str| -> serde_json::Value {
        serde_json::json!({
            "command": format!("\"{}\" --config \"{}\" hook --event {} --agent {}", mp_bin, config_path, event, agent)
        })
    };

    let hooks = serde_json::json!({
        "version": 1,
        "hooks": {
            "sessionStart": [cmd("sessionStart")],
            "stop": [cmd("stop")],
            "preToolUse": [cmd("preToolUse")],
            "postToolUse": [cmd("postToolUse")],
            "beforeShellExecution": [cmd("beforeShellExecution")],
            "afterShellExecution": [cmd("afterShellExecution")],
            "beforeMCPExecution": [cmd("beforeMCPExecution")],
            "afterMCPExecution": [cmd("afterMCPExecution")],
            "afterFileEdit": [cmd("afterFileEdit")]
        }
    });
    serde_json::to_string_pretty(&hooks).unwrap_or_default() + "\n"
}

fn generate_hooks_json_docker(image: &str, data_mount: &str, config_mount: &str, agent: &str) -> String {
    let cmd = |event: &str| -> serde_json::Value {
        serde_json::json!({
            "command": format!(
                "docker run --rm -i -v {data_mount} -v {config_mount} {image} hook --event {event} --agent {agent}"
            )
        })
    };

    let hooks = serde_json::json!({
        "version": 1,
        "hooks": {
            "sessionStart": [cmd("sessionStart")],
            "stop": [cmd("stop")],
            "preToolUse": [cmd("preToolUse")],
            "postToolUse": [cmd("postToolUse")],
            "beforeShellExecution": [cmd("beforeShellExecution")],
            "afterShellExecution": [cmd("afterShellExecution")],
            "beforeMCPExecution": [cmd("beforeMCPExecution")],
            "afterMCPExecution": [cmd("afterMCPExecution")],
            "afterFileEdit": [cmd("afterFileEdit")]
        }
    });
    serde_json::to_string_pretty(&hooks).unwrap_or_default() + "\n"
}

fn generate_cortex_hooks_json(mp_bin: &str, config_path: &str, agent: &str) -> serde_json::Value {
    let hook_entry = |event: &str, matcher: &str| -> serde_json::Value {
        serde_json::json!({
            "matcher": matcher,
            "hooks": [{
                "type": "command",
                "command": format!("{} --config {} hook --event {} --agent {}", mp_bin, config_path, event, agent),
                "timeout": 10
            }]
        })
    };

    serde_json::json!({
        "hooks": {
            "SessionStart": [hook_entry("sessionStart", "*")],
            "Stop": [hook_entry("stop", "*")],
            "PreToolUse": [hook_entry("preToolUse", "*")],
            "PostToolUse": [hook_entry("postToolUse", "*")]
        }
    })
}

fn upsert_json_hooks_config(
    path: &std::path::Path,
    new_hooks: &serde_json::Value,
) -> Result<()> {
    let mut root: serde_json::Value = if path.exists() {
        let content = std::fs::read_to_string(path)?;
        serde_json::from_str(&content).unwrap_or_else(|_| serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    let root_obj = root
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("{} is not a JSON object", path.display()))?;

    if let Some(new_hooks_obj) = new_hooks.get("hooks").and_then(|h| h.as_object()) {
        let hooks = root_obj
            .entry("hooks")
            .or_insert_with(|| serde_json::json!({}));
        if let Some(hooks_obj) = hooks.as_object_mut() {
            for (event, entries) in new_hooks_obj {
                hooks_obj.insert(event.clone(), entries.clone());
            }
        }
    }

    let formatted = serde_json::to_string_pretty(&root)?;
    std::fs::write(path, format!("{formatted}\n"))?;
    Ok(())
}

fn upsert_json_mcp_config(
    path: &std::path::Path,
    server_name: &str,
    entry: serde_json::Value,
) -> Result<()> {
    let mut root: serde_json::Value = if path.exists() {
        let content = std::fs::read_to_string(path)?;
        serde_json::from_str(&content).unwrap_or_else(|_| serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    let root_obj = root
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("{} is not a JSON object", path.display()))?;

    let servers = root_obj
        .entry("mcpServers")
        .or_insert_with(|| serde_json::json!({}));
    servers
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("\"mcpServers\" is not a JSON object"))?
        .insert(server_name.to_string(), entry);

    let formatted = serde_json::to_string_pretty(&root)?;
    std::fs::write(path, format!("{formatted}\n"))?;
    Ok(())
}
