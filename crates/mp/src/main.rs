pub mod agent;
mod adapters;
mod cli;
mod domain_tools;
pub mod helpers;
mod sidecar;
mod ui;
pub mod worker;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Command};
use helpers::{
    build_embedding_provider, build_embedding_provider_with_override, build_provider,
    build_sidecar_request, csv_escape, embed_pending, embedding_model_id,
    ensure_embedding_models, extract_facts, maybe_summarize_session,
    normalize_embedding_target, op_request, open_agent_db, parse_duration_hours,
    resolve_agent, resolve_or_create_session, seed_bootstrap_facts, sidecar_error_response,
    sql_quote, toml_to_json, truncate,
};
use worker::{WorkerBus, WorkerHandle, run_scheduler, spawn_worker};
use mp_core::config::Config;
use std::path::Path;
use std::sync::Arc;
use tracing_subscriber::{EnvFilter, fmt};

#[tokio::main]
async fn main() -> Result<()> {
    let _ = dotenvy::dotenv();
    let cli = Cli::parse();

    if matches!(cli.command, Command::Init) {
        return cmd_init(&cli.config).await;
    }

    let config_path = Path::new(&cli.config);

    // Also load .env from the config file's directory so env vars are found
    // regardless of process cwd (e.g. when launched as an MCP server).
    if let Some(config_dir) = config_path.parent().and_then(|p| std::fs::canonicalize(p).ok()) {
        let _ = dotenvy::from_path(config_dir.join(".env"));
    }

    let config = Config::load(config_path).unwrap_or_else(|e| {
        eprintln!(
            "Failed to load config from {}: {e}\nRun `mp init` to create a config file.",
            cli.config
        );
        std::process::exit(1);
    });

    init_logging(&config.gateway.log_level);

    match cli.command {
        Command::Init => unreachable!(),
        Command::Start => cmd_start(&config, config_path).await,
        Command::Serve { agent } => cmd_serve(&config, config_path, agent).await,
        Command::Stop => cmd_stop(&config).await,
        Command::Agent(cmd) => cmd_agent(&config, cmd).await,
        Command::Chat { agent, session_id, new } => cmd_chat(&config, agent, session_id, new).await,
        Command::Send {
            agent,
            message,
            session_id,
        } => cmd_send(&config, &agent, &message, session_id).await,
        Command::Facts(cmd) => cmd_facts(&config, cmd).await,
        Command::Ingest {
            path,
            url,
            agent,
            openclaw_file,
            replay,
            status,
            replay_run,
            replay_latest,
            replay_offset,
            status_filter,
            file_filter,
            dry_run,
            apply,
            source,
            limit,
            cortex,
            claude_code,
            cursor,
        } => {
            cmd_ingest(
                &config,
                path,
                url,
                agent,
                openclaw_file,
                replay,
                status,
                replay_run,
                replay_latest,
                replay_offset,
                status_filter,
                file_filter,
                dry_run,
                apply,
                source,
                limit,
                cortex,
                claude_code,
                cursor,
            )
            .await
        }
        Command::Session(cmd) => cmd_session(&config, cmd).await,
        Command::Knowledge(cmd) => cmd_knowledge(&config, cmd).await,
        Command::Skill(cmd) => cmd_skill(&config, cmd).await,
        Command::Policy(cmd) => cmd_policy(&config, cmd).await,
        Command::Job(cmd) => cmd_job(&config, cmd).await,
        Command::Embeddings(cmd) => cmd_embeddings(&config, cmd).await,
        Command::Audit { agent, command } => cmd_audit(&config, agent, command).await,
        Command::Sync(cmd) => cmd_sync(&config, cmd).await,
        Command::Fleet(cmd) => cmd_fleet(&config, cmd).await,
        Command::Mpq { expression, agent, dry_run } => cmd_mpq(&config, &expression, agent, dry_run).await,
        Command::Db(cmd) => cmd_db(&config, cmd).await,
        Command::Health => cmd_health(&config).await,
        Command::Doctor => cmd_doctor(&config, config_path).await,
        Command::Worker { agent } => worker::cmd_worker(&config, &agent).await,
        Command::Sidecar { agent } => sidecar::cmd_sidecar(&config, agent).await,
        Command::Setup(cmd) => cmd_setup(&config, config_path, cmd).await,
        Command::Hook { event, agent } => cmd_hook(&config, &event, agent).await,
    }
}

fn init_logging(level: &str) {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level));

    fmt().with_env_filter(filter).with_target(true).init();
}

// resolve_agent, open_agent_db, build_provider, build_embedding_provider,
// build_embedding_provider_with_override, embedding_model_id — see helpers module

// agent_turn, build_llm_tools, intent classification, tool defs — see agent module

// extract_facts, maybe_summarize_session, embed_pending, model download,
// bootstrap facts — see helpers module

// =========================================================================
// Init
// =========================================================================

async fn cmd_init(config_path: &str) -> Result<()> {
    let path = Path::new(config_path);
    if path.exists() {
        anyhow::bail!("{config_path} already exists. Delete it first to re-initialize.");
    }

    let config = Config::default_config();
    let toml_str = config.to_toml()?;

    std::fs::write(path, &toml_str)?;
    std::fs::create_dir_all(&config.data_dir)?;
    std::fs::create_dir_all(config.models_dir())?;

    let meta_path = config.metadata_db_path();
    let meta_conn = mp_core::db::open(&meta_path)?;
    mp_core::schema::init_metadata_db(&meta_conn)?;

    let bootstrap_conn = {
        let conn = mp_core::db::open_memory()?;
        mp_core::schema::init_agent_db(&conn)?;
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, created_at)
             VALUES ('allow-bootstrap', 'allow bootstrap', 1000, 'allow', '*', '*', '*', ?1)",
            [chrono::Utc::now().timestamp()],
        )?;
        conn
    };

    for agent in &config.agents {
        let req = op_request(
            "bootstrap",
            "agent.create",
            serde_json::json!({
                "name": agent.name,
                "persona": agent.persona,
                "trust_level": agent.trust_level,
                "llm_provider": agent.llm.provider,
                "llm_model": agent.llm.model,
                "metadata_db_path": meta_path.to_string_lossy().to_string(),
                "agent_db_path": config.agent_db_path(&agent.name).to_string_lossy().to_string(),
            }),
        );
        let resp = mp_core::operations::execute(&bootstrap_conn, &req)?;
        if !resp.ok && resp.code != "already_exists" {
            anyhow::bail!(
                "failed to initialize agent '{}': {}",
                agent.name,
                resp.message
            );
        }
    }

    for agent in &config.agents {
        let agent_db_path = config.agent_db_path(&agent.name);
        if let Ok(agent_conn) = mp_core::db::open(&agent_db_path) {
            let _ = mp_core::schema::init_agent_db(&agent_conn);
            seed_bootstrap_facts(&agent_conn, &agent.name);
        }
    }

    ui::banner();
    ui::info(format!("Creating project in {}", config.data_dir.display()));
    ui::blank();
    ui::success(format!("Created {config_path}"));
    ui::success("Created data directory");
    ui::success("Created models directory");
    for agent in &config.agents {
        ui::success(format!("Initialized agent \"{}\"", agent.name));
        ui::detail(format!(
            "Embedding: {} ({}, {}D)",
            agent.embedding.provider, agent.embedding.model, agent.embedding.dimensions
        ));
    }
    ui::success("Seeded bootstrap facts");
    ui::blank();

    ensure_embedding_models(&config).await;

    ui::blank();
    ui::info("Ready! Next steps:");
    ui::blank();
    ui::hint("mp setup cursor --local            # register with Cursor");
    ui::hint("mp setup cortex                    # register with Cortex Code CLI");
    ui::hint("mp setup claude-code               # register with Claude Code");
    ui::blank();
    ui::info("Then ask your agent: \"What Moneypenny tools do you have?\"");
    ui::blank();
    ui::info("CLI agent:");
    ui::hint("mp chat                            # interactive terminal chat");
    ui::hint("mp send main \"remember X\"          # one-shot message");
    ui::blank();
    ui::info("Tip: Set ANTHROPIC_API_KEY in .env for LLM features (mp chat, mp send).");
    ui::blank();

    Ok(())
}

// =========================================================================
// Setup (MCP registration for AI coding agents)
// =========================================================================

async fn cmd_setup(config: &Config, config_path: &Path, cmd: cli::SetupCommand) -> Result<()> {
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
                    // Replace the existing Moneypenny section (from "## Moneypenny" to end
                    // of file, or to the next top-level heading).
                    let start = existing.find("## Moneypenny").unwrap();
                    let before = &existing[..start];
                    // Find the next "## " heading after the Moneypenny section start.
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

            // Write agent instructions as a Cortex skill
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

            // Write hooks config
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

    // Write .cursor/mcp.json
    let mcp_path = project_dir.join(".cursor").join("mcp.json");
    if let Some(parent) = mcp_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    upsert_json_mcp_config(&mcp_path, "moneypenny", mcp_server_entry)?;

    // Write .cursor/rules/moneypenny.mdc
    let rules_dir = project_dir.join(".cursor").join("rules");
    std::fs::create_dir_all(&rules_dir)?;
    let rule_path = rules_dir.join("moneypenny.mdc");
    let cursor_agent_conn = mp_core::db::open(&config.agent_db_path(&ag.name))
        .ok()
        .and_then(|c| mp_core::schema::init_agent_db(&c).ok().map(|_| c));
    std::fs::write(&rule_path, generate_agent_instructions(cursor_agent_conn.as_ref()))?;

    // Write .cursor/hooks.json
    let hooks_json_path = project_dir.join(".cursor").join("hooks.json");
    std::fs::write(&hooks_json_path, hooks_config)?;

    // Ensure data directory exists for Docker volume mount
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

// =========================================================================
// Hook (Cursor hooks → audit + policy enforcement)
// =========================================================================

async fn cmd_hook(config: &Config, event: &str, agent: Option<String>) -> Result<()> {
    let ag = resolve_agent(config, agent.as_deref())?;
    let conn = open_agent_db(config, &ag.name)?;

    let input: serde_json::Value = {
        let mut buf = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut buf)?;
        serde_json::from_str(&buf).unwrap_or_else(|_| serde_json::json!({}))
    };

    let conversation_id = input["conversation_id"].as_str().unwrap_or("unknown");
    let generation_id = input["generation_id"].as_str().unwrap_or("unknown");

    match event {
        // ── Observe-only events → activity_log ──
        "sessionStart" => {
            let model = input["model"].as_str().unwrap_or("unknown");
            record_activity(
                &conn, &ag.name, event, "session_start", "session",
                &format!("model={model}"),
                conversation_id, generation_id, None,
            )?;
            emit_hook_allow();
        }
        "sessionEnd" | "stop" => {
            let status = input["status"].as_str().unwrap_or("completed");
            record_activity(
                &conn, &ag.name, event, "session_end", "session",
                &format!("status={status}"),
                conversation_id, generation_id, None,
            )?;
            emit_hook_allow();
        }
        "postToolUse" => {
            let tool = input["tool_name"].as_str().unwrap_or("unknown");
            let duration = input["duration"].as_u64();
            record_activity(
                &conn, &ag.name, event, "tool_call", tool,
                &format!("Tool completed"),
                conversation_id, generation_id, duration,
            )?;
            emit_hook_allow();
        }
        "afterShellExecution" => {
            let command = input["command"].as_str().unwrap_or("");
            let duration = input["duration"].as_u64();
            record_activity(
                &conn, &ag.name, event, "shell_exec", "shell",
                truncate(command, 500),
                conversation_id, generation_id, duration,
            )?;
            emit_hook_allow();
        }
        "afterMCPExecution" => {
            let tool = input["tool_name"].as_str().unwrap_or("unknown");
            let duration = input["duration"].as_u64();
            record_activity(
                &conn, &ag.name, event, "mcp_call", tool,
                "MCP tool completed",
                conversation_id, generation_id, duration,
            )?;
            emit_hook_allow();
        }
        "afterFileEdit" => {
            let file_path = input["file_path"].as_str().unwrap_or("unknown");
            let edit_count = input["edits"].as_array().map(|a| a.len()).unwrap_or(0);
            record_activity(
                &conn, &ag.name, event, "file_edit", file_path,
                &format!("{edit_count} edit(s)"),
                conversation_id, generation_id, None,
            )?;
            emit_hook_allow();
        }

        // ── Policy-enforced events → policy_audit + activity_log ──
        "preToolUse" => {
            let tool = input["tool_name"].as_str().unwrap_or("unknown");
            let tool_res = mp_core::policy::resource::tool(tool);
            let decision = mp_core::policy::evaluate(
                &conn,
                &mp_core::policy::PolicyRequest {
                    actor: &ag.name,
                    action: "call",
                    resource: &tool_res,
                    sql_content: None,
                    channel: Some("cursor"),
                    arguments: None,
                },
            )?;
            record_policy_audit(
                &conn, &ag.name, "call", &tool_res,
                &decision, conversation_id, generation_id,
            )?;
            record_activity(
                &conn, &ag.name, event, "policy_check", &tool_res,
                &format!("{:?}", decision.effect),
                conversation_id, generation_id, None,
            )?;
            emit_hook_decision(&decision);
        }
        "beforeShellExecution" => {
            let command = input["command"].as_str().unwrap_or("");
            let decision = mp_core::policy::evaluate(
                &conn,
                &mp_core::policy::PolicyRequest {
                    actor: &ag.name,
                    action: "shell_exec",
                    resource: mp_core::policy::resource::SHELL,
                    sql_content: Some(command),
                    channel: Some("cursor"),
                    arguments: Some(command),
                },
            )?;
            record_policy_audit(
                &conn, &ag.name, "shell_exec", mp_core::policy::resource::SHELL,
                &decision, conversation_id, generation_id,
            )?;
            record_activity(
                &conn, &ag.name, event, "policy_check", mp_core::policy::resource::SHELL,
                &format!("{:?}: {}", decision.effect, truncate(command, 200)),
                conversation_id, generation_id, None,
            )?;
            emit_hook_decision(&decision);
        }
        "beforeMCPExecution" => {
            let tool = input["tool_name"].as_str().unwrap_or("unknown");
            let tool_res = mp_core::policy::resource::tool(tool);
            let decision = mp_core::policy::evaluate(
                &conn,
                &mp_core::policy::PolicyRequest {
                    actor: &ag.name,
                    action: "mcp_call",
                    resource: &tool_res,
                    sql_content: None,
                    channel: Some("cursor"),
                    arguments: None,
                },
            )?;
            record_policy_audit(
                &conn, &ag.name, "mcp_call", &tool_res,
                &decision, conversation_id, generation_id,
            )?;
            record_activity(
                &conn, &ag.name, event, "policy_check", &tool_res,
                &format!("{:?}", decision.effect),
                conversation_id, generation_id, None,
            )?;
            emit_hook_decision(&decision);
        }

        _ => {
            record_activity(
                &conn, &ag.name, event, "unknown", "unknown",
                "unhandled hook event",
                conversation_id, generation_id, None,
            )?;
            emit_hook_allow();
        }
    }

    Ok(())
}

fn record_activity(
    conn: &rusqlite::Connection,
    agent_id: &str,
    event: &str,
    action: &str,
    resource: &str,
    detail: &str,
    conversation_id: &str,
    generation_id: &str,
    duration_ms: Option<u64>,
) -> Result<()> {
    conn.execute(
        "INSERT INTO activity_log (id, agent_id, event, action, resource, detail, conversation_id, generation_id, duration_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        rusqlite::params![
            uuid::Uuid::new_v4().to_string(),
            agent_id,
            event,
            action,
            resource,
            detail,
            conversation_id,
            generation_id,
            duration_ms.map(|d| d as i64),
        ],
    )?;
    Ok(())
}

fn record_policy_audit(
    conn: &rusqlite::Connection,
    agent_id: &str,
    action: &str,
    resource: &str,
    decision: &mp_core::policy::PolicyDecision,
    conversation_id: &str,
    generation_id: &str,
) -> Result<()> {
    let now = chrono::Utc::now().timestamp();
    let effect_str = match decision.effect {
        mp_core::policy::Effect::Allow => "allow",
        mp_core::policy::Effect::Deny => "deny",
        mp_core::policy::Effect::Audit => "audit",
    };
    conn.execute(
        "INSERT INTO policy_audit (id, policy_id, actor, action, resource, effect, reason, correlation_id, session_id, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        rusqlite::params![
            uuid::Uuid::new_v4().to_string(),
            decision.policy_id,
            agent_id,
            action,
            resource,
            effect_str,
            decision.reason,
            generation_id,
            conversation_id,
            now,
        ],
    )?;
    Ok(())
}

fn emit_hook_allow() {
    println!("{}", serde_json::json!({ "permission": "allow" }));
}

fn emit_hook_decision(decision: &mp_core::policy::PolicyDecision) {
    match decision.effect {
        mp_core::policy::Effect::Deny => {
            let msg = decision.reason.as_deref().unwrap_or("Blocked by Moneypenny policy");
            println!("{}", serde_json::json!({
                "permission": "deny",
                "user_message": msg,
                "agent_message": format!("Policy denied this action: {msg}")
            }));
        }
        _ => {
            emit_hook_allow();
        }
    }
}

// truncate — see helpers module

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

fn generate_claude_md(agent_conn: Option<&rusqlite::Connection>) -> String {
    let mut md = r#"## Moneypenny

You have access to a Moneypenny MCP server. It provides persistent facts,
knowledge retrieval, document ingestion, governance policies, and activity
tracking.

### "mp" prefix

When the user starts a message with **"mp"** (e.g. "mp remember that we use
Redis for caching", "mp search facts about auth", "mp ingest this doc"), treat
it as a direct instruction to use Moneypenny. Translate the natural-language
request into the appropriate tool call and execute it immediately.

### Tools

| Tool | Purpose |
|------|---------|
| `moneypenny_facts` | CRUD for durable facts — persistent knowledge across sessions. |
| `moneypenny_knowledge` | Ingest and retrieve documents — long-term reference library. |
| `moneypenny_policy` | Governance — control what agents can and cannot do. |
| `moneypenny_activity` | Query session history and audit trail. |
| `moneypenny_execute` | Escape hatch for any canonical operation. |

**Important:** These tools are MCP tools served by the Moneypenny sidecar
process. They must appear in your callable tool list. If they do not, the MCP
server is not connected — tell the user to run `mp setup claude-code` in the
project directory.

### Tool usage

Each domain tool takes an `action` string and an `input` object.

**moneypenny_facts**: search, add, get, update, delete
**moneypenny_knowledge**: ingest, search, list
**moneypenny_policy**: add, list, disable, evaluate
**moneypenny_activity**: query (source: events | decisions | all)
**moneypenny_execute**: op + args (any canonical operation)

### When to use Moneypenny

- **User says "mp ..."**: Always route through Moneypenny
- **Remembering things**: Use `moneypenny_facts` action `add`
- **Recalling context**: Use `moneypenny_facts` action `search`
- **Ingesting documents**: Use `moneypenny_knowledge` action `ingest`
- **Activity trail**: Use `moneypenny_activity` action `query`
- **Governance**: Use `moneypenny_policy` to manage rules

### Best practices

- Search before inserting facts to avoid duplicates
- Use specific keywords when inserting facts
- Set confidence scores to reflect certainty (0.0 to 1.0)
- Use `moneypenny_execute` only for operations not covered by domain tools
"#.to_string();

    if let Some(conn) = agent_conn {
        md.push('\n');
        md.push_str(&mp_core::schema::generate_schema_summary(conn));
    }

    md
}

fn generate_agent_instructions(agent_conn: Option<&rusqlite::Connection>) -> String {
    let mut md = r#"---
description: Moneypenny MCP server - persistent facts, knowledge, governance, and activity tracking for AI agents
globs:
alwaysApply: true
---

# Moneypenny

You have access to a Moneypenny MCP server. It provides persistent facts,
knowledge retrieval, document ingestion, governance policies, and activity
tracking.

## "mp" prefix

When the user starts a message with **"mp"** (e.g. "mp remember that we use
Redis for caching", "mp search facts about auth", "mp ingest this doc"), treat
it as a direct instruction to use Moneypenny. Translate the natural-language
request into the appropriate tool call and execute it immediately.

## Tools

| Tool | Purpose |
|------|---------|
| `moneypenny_facts` | CRUD for durable facts — persistent knowledge across sessions. |
| `moneypenny_knowledge` | Ingest and retrieve documents — long-term reference library. |
| `moneypenny_policy` | Governance — control what agents can and cannot do. |
| `moneypenny_activity` | Query session history and audit trail. |
| `moneypenny_execute` | Escape hatch for any canonical operation. |

**Important:** These tools are MCP tools served by the Moneypenny sidecar
process. They must appear in your callable tool list. If they do not, the MCP
server is not connected — tell the user to run `mp setup cursor` and restart
Cursor (or reload the window).

## Tool usage

Each domain tool takes an `action` string and an `input` object.

### moneypenny_facts
- `search`: `{query, limit?}` — hybrid search across facts
- `add`: `{content, summary?, keywords?, confidence?}` — store a new fact
- `get`: `{id}` — retrieve a fact by ID
- `update`: `{id, content, summary?}` — update an existing fact
- `delete`: `{id, reason?}` — remove a fact

### moneypenny_knowledge
- `ingest`: `{path?, content?, title?}` — add a document (pass `path` as an HTTP URL to fetch a webpage, or provide `content` directly)
- `search`: `{query, limit?}` — search ingested documents
- `list`: `{}` — list all documents

### moneypenny_policy
- `add`: `{name, effect?, priority?, action_pattern?, resource_pattern?, sql_pattern?, message?}` — create a policy
- `list`: `{enabled?, effect?, limit?}` — list policies
- `disable`: `{id}` — disable a policy
- `evaluate`: `{actor, action, resource}` — test if action is allowed

### moneypenny_activity
- `query`: `{source?, event?, action?, resource?, query?, limit?}` — query events and decisions

### moneypenny_execute
- `op`: canonical operation name (e.g. `job.create`, `ingest.events`)
- `args`: operation-specific arguments

## When to use Moneypenny

- **User says "mp ..."**: Always route through Moneypenny
- **Remembering things**: Use `moneypenny_facts` action `add`
- **Recalling context**: Use `moneypenny_facts` action `search`
- **Ingesting documents**: Use `moneypenny_knowledge` action `ingest`
- **Activity trail**: Use `moneypenny_activity` action `query`
- **Governance**: Use `moneypenny_policy` to manage rules

## Best practices

- Search before inserting facts to avoid duplicates
- Use specific keywords when inserting facts
- Set confidence scores to reflect certainty (0.0 to 1.0)
- Use `moneypenny_execute` only for operations not covered by domain tools
"#.to_string();

    if let Some(conn) = agent_conn {
        md.push('\n');
        md.push_str(&mp_core::schema::generate_schema_summary(conn));
    }

    md
}

fn generate_cortex_skill(agent_conn: Option<&rusqlite::Connection>) -> String {
    let mut md = r#"---
name: moneypenny
description: Persistent facts, knowledge, governance, and activity tracking via Moneypenny MCP server
tools:
- mcp__moneypenny__moneypenny_facts
- mcp__moneypenny__moneypenny_knowledge
- mcp__moneypenny__moneypenny_policy
- mcp__moneypenny__moneypenny_activity
- mcp__moneypenny__moneypenny_execute
---

# When to Use

- User says "mp ..." (e.g. "mp remember that we use Redis for caching")
- Remembering things across sessions
- Recalling context or searching memory
- Ingesting documents into knowledge
- Querying the activity/audit trail
- Managing governance policies

# What This Skill Provides

Moneypenny is the intelligence and governance core for AI agents. It provides
structured memory, policy-governed execution, explainable audit, and portable
state — all in a single SQLite file per agent.

# Instructions

Translate natural-language requests into the appropriate MCP tool call and
execute it immediately.

## "mp" Prefix

When the user starts a message with **"mp"**, treat it as a direct instruction
to use Moneypenny. Examples:

- "mp remember that we use Redis for caching" → `moneypenny_facts` action `add`
- "mp search facts about auth" → `moneypenny_facts` action `search`
- "mp ingest this doc" → `moneypenny_knowledge` action `ingest`

## Tool Usage

Each domain tool takes an `action` string and an `input` object.

### moneypenny_facts
- `search`: `{query, limit?}` — hybrid search across facts
- `add`: `{content, summary?, keywords?, confidence?}` — store a new fact
- `get`: `{id}` — retrieve a fact by ID
- `update`: `{id, content, summary?}` — update an existing fact
- `delete`: `{id, reason?}` — remove a fact

### moneypenny_knowledge
- `ingest`: `{path?, content?, title?}` — add a document (pass `path` as an HTTP URL to fetch a webpage, or provide `content` directly)
- `search`: `{query, limit?}` — search ingested documents
- `list`: `{}` — list all documents

### moneypenny_policy
- `add`: `{name, effect?, priority?, action_pattern?, resource_pattern?, sql_pattern?, message?}` — create a policy
- `list`: `{enabled?, effect?, limit?}` — list policies
- `disable`: `{id}` — disable a policy
- `evaluate`: `{actor, action, resource}` — test if action is allowed

### moneypenny_activity
- `query`: `{source?, event?, action?, resource?, query?, limit?}` — query events and decisions

### moneypenny_execute
- `op`: canonical operation name (e.g. `job.create`, `ingest.events`)
- `args`: operation-specific arguments

## Best Practices

- Search before inserting facts to avoid duplicates
- Use specific keywords when inserting facts
- Set confidence scores to reflect certainty (0.0 to 1.0)
- Use `moneypenny_execute` only for operations not covered by domain tools
"#.to_string();

    if let Some(conn) = agent_conn {
        md.push('\n');
        md.push_str(&mp_core::schema::generate_schema_summary(conn));
    }

    md
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

// =========================================================================
// Start / Stop
// =========================================================================

async fn cmd_start(config: &Config, config_path: &Path) -> Result<()> {
    ui::banner();

    let shutdown = tokio::sync::broadcast::channel::<()>(1).0;

    // Spawn one worker subprocess per agent and register each in the WorkerBus
    let bus = WorkerBus::new();
    let mut workers: Vec<WorkerHandle> = Vec::new();
    for agent in &config.agents {
        let (handle, w_stdin, w_stdout) = spawn_worker(config, config_path, &agent.name)?;
        ui::info(format!("Worker \"{}\" started (pid {})", agent.name, handle.pid));
        bus.register(agent.name.clone(), w_stdin, w_stdout).await;
        workers.push(handle);
    }

    // Spawn the scheduler loop
    let sched_config = config.clone();
    let mut sched_shutdown = shutdown.subscribe();
    let scheduler_handle =
        tokio::spawn(async move { run_scheduler(&sched_config, &mut sched_shutdown).await });

    // Build the shared dispatcher used by all channel adapters.
    // It routes (agent, message, session_id) through the WorkerBus.
    let bus_for_dispatch = Arc::clone(&bus);
    let dispatch: adapters::DispatchFn = Arc::new(move |agent, message, session_id| {
        let bus = Arc::clone(&bus_for_dispatch);
        Box::pin(async move {
            bus.route_full(&agent, &message, session_id.as_deref())
                .await
        })
    });

    // Canonical operation HTTP parity dispatcher.
    let config_for_ops = config.clone();
    let op_dispatch: adapters::OpDispatchFn = Arc::new(move |payload| {
        let config = config_for_ops.clone();
        Box::pin(async move {
            let default_agent = config
                .agents
                .first()
                .map(|a| a.name.clone())
                .unwrap_or_else(|| "main".into());

            let req = match build_sidecar_request(payload, &default_agent) {
                Ok(r) => r,
                Err(e) => return Ok(sidecar_error_response("invalid_request", e.to_string())),
            };

            let conn = match open_agent_db(&config, &req.actor.agent_id) {
                Ok(c) => c,
                Err(e) => return Ok(sidecar_error_response("invalid_agent", e.to_string())),
            };

            let resp = match mp_core::operations::execute(&conn, &req) {
                Ok(r) => r,
                Err(e) => {
                    return Ok(sidecar_error_response(
                        "http_ops_execute_error",
                        e.to_string(),
                    ));
                }
            };

            Ok(serde_json::to_value(resp)
                .unwrap_or_else(|e| sidecar_error_response("serialization_error", e.to_string())))
        })
    });

    // Spawn the combined HTTP/Slack/Discord server if any HTTP-facing channel is configured.
    let has_http_channel = config.channels.http.is_some()
        || config.channels.slack.is_some()
        || config.channels.discord.is_some();

    if has_http_channel {
        let http_cfg = config.channels.http.clone();
        let slack_cfg = config.channels.slack.clone();
        let discord_cfg = config.channels.discord.clone();
        let default_agent = config
            .agents
            .first()
            .map(|a| a.name.clone())
            .unwrap_or_else(|| "main".into());
        let dispatch_clone = Arc::clone(&dispatch);
        let op_dispatch_clone = Arc::clone(&op_dispatch);
        let srv_shutdown = shutdown.subscribe();
        tokio::spawn(async move {
            if let Err(e) = adapters::run_http_server(
                http_cfg.as_ref(),
                slack_cfg.as_ref(),
                discord_cfg.as_ref(),
                default_agent,
                dispatch_clone,
                op_dispatch_clone,
                srv_shutdown,
            )
            .await
            {
                tracing::error!("HTTP server error: {e}");
            }
        });
    }

    // Spawn the Telegram long-polling adapter if configured.
    if let Some(tg_cfg) = config.channels.telegram.clone() {
        let default_agent = config
            .agents
            .first()
            .map(|a| a.name.clone())
            .unwrap_or_else(|| "main".into());
        let dispatch_clone = Arc::clone(&dispatch);
        let tg_shutdown = shutdown.subscribe();
        tokio::spawn(async move {
            adapters::run_telegram_polling(tg_cfg, default_agent, dispatch_clone, tg_shutdown)
                .await;
        });
    }

    // Spawn the periodic sync loop if interval_secs > 0 and peers/cloud are configured.
    let has_sync = config.sync.interval_secs > 0
        && (!config.sync.peers.is_empty() || config.sync.cloud_url.is_some());
    if has_sync {
        let sync_config = config.sync.clone();
        let sync_data_dir = config.data_dir.clone();
        let sync_agents: Vec<String> = config.agents.iter().map(|a| a.name.clone()).collect();
        let mut sync_shutdown = shutdown.subscribe();
        tokio::spawn(async move {
            let interval = std::time::Duration::from_secs(sync_config.interval_secs);
            let tables: Vec<&str> = sync_config.tables.iter().map(String::as_str).collect();
            loop {
                tokio::select! {
                    _ = tokio::time::sleep(interval) => {}
                    _ = sync_shutdown.recv() => break,
                }
                for agent_name in &sync_agents {
                    let db_path = sync_data_dir.join(format!("{agent_name}.db"));
                    let conn = match rusqlite::Connection::open(&db_path) {
                        Ok(c) => c,
                        Err(e) => {
                            tracing::warn!("sync: cannot open {agent_name}: {e}");
                            continue;
                        }
                    };
                    if let Err(e) = mp_ext::init_all_extensions(&conn) {
                        tracing::warn!("sync: ext init for {agent_name}: {e}");
                        continue;
                    }
                    let _ = mp_core::sync::init_sync_tables(&conn, &tables);
                    for peer in &sync_config.peers {
                        let peer_path =
                            if std::path::Path::new(peer).is_absolute() || peer.ends_with(".db") {
                                std::path::PathBuf::from(peer)
                            } else {
                                sync_data_dir.join(format!("{peer}.db"))
                            };
                        if !peer_path.exists() {
                            continue;
                        }
                        let peer_conn = match rusqlite::Connection::open(&peer_path).and_then(|c| {
                            mp_ext::init_all_extensions(&c).ok();
                            Ok(c)
                        }) {
                            Ok(c) => c,
                            Err(e) => {
                                tracing::warn!("auto-sync: cannot open peer {peer}: {e}");
                                continue;
                            }
                        };
                        let _ = mp_core::sync::init_sync_tables(&peer_conn, &tables);
                        match mp_core::sync::local_sync_bidirectional(
                            &conn,
                            &peer_conn,
                            agent_name,
                            peer,
                            &tables,
                        ) {
                            Ok(r) => {
                                tracing::debug!(agent = %agent_name, peer = %peer, sent = r.sent, received = r.received, "auto-sync")
                            }
                            Err(e) => {
                                tracing::warn!(agent = %agent_name, peer = %peer, "auto-sync error: {e}")
                            }
                        }
                    }
                    if let Some(ref url) = sync_config.cloud_url {
                        match mp_core::sync::cloud_sync(&conn, url) {
                            Ok(r) => {
                                tracing::debug!(agent = %agent_name, batches = r.sent, "cloud auto-sync")
                            }
                            Err(e) => tracing::warn!(agent = %agent_name, "cloud sync error: {e}"),
                        }
                    }
                }
            }
        });
    }

    // Write PID file for `mp stop`
    let pid_path = config.data_dir.join("mp.pid");
    std::fs::write(&pid_path, std::process::id().to_string())?;

    ui::blank();
    ui::info(format!("Gateway ready. {} agent(s) running.", config.agents.len()));
    if has_http_channel {
        let port = config
            .channels
            .http
            .as_ref()
            .map(|h| h.port)
            .unwrap_or(8080);
        ui::info(format!(
            "HTTP API listening on port {port}  (POST /v1/chat, POST /v1/ops, WS /v1/ws, GET /health)"
        ));
    }
    if config.channels.slack.is_some() {
        ui::info("Slack Events API endpoint: POST /slack/events");
    }
    if config.channels.discord.is_some() {
        ui::info("Discord Interactions endpoint: POST /discord/interactions");
    }
    if config.channels.telegram.is_some() {
        ui::info("Telegram long-polling active");
    }
    if has_sync {
        ui::info(format!(
            "Auto-sync every {}s ({} peer(s){})",
            config.sync.interval_secs,
            config.sync.peers.len(),
            if config.sync.cloud_url.is_some() {
                " + cloud"
            } else {
                ""
            }
        ));
    }
    ui::info("Press Ctrl-C to shut down.");
    ui::blank();

    // If CLI channel is enabled, run interactive chat on the default agent
    if config.channels.cli {
        let default_agent = config
            .agents
            .first()
            .map(|a| a.name.clone())
            .unwrap_or_else(|| "main".into());
        let ag = resolve_agent(config, Some(&default_agent))?;
        let conn = open_agent_db(config, &ag.name)?;
        let provider = build_provider(ag)?;
        let embed = build_embedding_provider(config, ag).ok();
        let sid = mp_core::store::log::create_session(&conn, &ag.name, Some("cli"))?;

        ui::info(format!("CLI channel active — agent: {}", ag.name));
        ui::info("Type /help for commands, Ctrl-C to shut down.");
        ui::blank();

        let mut shutdown_rx = shutdown.subscribe();
        let stdin = tokio::io::stdin();
        let mut reader = tokio::io::BufReader::new(stdin);

        loop {
            ui::prompt();

            let mut line = String::new();
            let read = tokio::select! {
                r = tokio::io::AsyncBufReadExt::read_line(&mut reader, &mut line) => r?,
                _ = shutdown_rx.recv() => break,
            };

            if read == 0 {
                break;
            }
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if trimmed == "/quit" || trimmed == "/exit" {
                break;
            }

            if trimmed == "/help" {
                ui::info("/facts    — list stored facts");
                ui::info("/scratch  — list scratch entries");
                ui::info("/quit     — exit");
                ui::blank();
                continue;
            }
            if trimmed == "/facts" {
                let facts = mp_core::store::facts::list_active(&conn, &ag.name)?;
                if facts.is_empty() {
                    ui::info("No facts stored.");
                } else {
                    for f in &facts {
                        ui::info(format!("[{:.1}] {}", f.confidence, f.pointer));
                    }
                }
                ui::blank();
                continue;
            }

            match agent::agent_turn(
                &conn,
                provider.as_ref(),
                embed.as_deref(),
                &ag.name,
                &sid,
                ag.persona.as_deref(),
                trimmed,
                ag.policy_mode(),
                Some(&bus),
            )
            .await
            {
                Ok(response) => {
                    ui::blank();
                    for l in response.lines() {
                        ui::info(l);
                    }
                    ui::blank();
                    if let Ok(n) = extract_facts(&conn, provider.as_ref(), &ag.name, &sid).await {
                        if n > 0 {
                            ui::dim(format!("({n} fact{} learned)", if n == 1 { "" } else { "s" }));
                            ui::blank();
                        }
                    }
                    if let Some(ref ep) = embed {
                        let model_id = embedding_model_id(ag);
                        embed_pending(&conn, ep.as_ref(), &ag.name, &model_id).await;
                    }
                    maybe_summarize_session(&conn, provider.as_ref(), &sid).await;
                }
                Err(e) => {
                    ui::error(e);
                    ui::blank();
                }
            }
        }
    } else {
        tokio::signal::ctrl_c().await?;
    }

    println!();
    ui::info("Shutting down...");
    let _ = shutdown.send(());
    scheduler_handle.abort();

    for mut w in workers {
        w.shutdown().await;
    }

    let _ = std::fs::remove_file(&pid_path);
    ui::info("Goodbye.");
    Ok(())
}

async fn cmd_serve(config: &Config, config_path: &Path, agent: Option<String>) -> Result<()> {
    let shutdown = tokio::sync::broadcast::channel::<()>(1).0;

    // Spawn one worker subprocess per agent and register each in the WorkerBus
    let bus = WorkerBus::new();
    let mut workers: Vec<WorkerHandle> = Vec::new();
    for ag in &config.agents {
        let (handle, w_stdin, w_stdout) = spawn_worker(config, config_path, &ag.name)?;
        tracing::info!(agent = %ag.name, pid = handle.pid, "worker started");
        bus.register(ag.name.clone(), w_stdin, w_stdout).await;
        workers.push(handle);
    }

    // Spawn the scheduler loop
    let sched_config = config.clone();
    let mut sched_shutdown = shutdown.subscribe();
    let scheduler_handle =
        tokio::spawn(async move { run_scheduler(&sched_config, &mut sched_shutdown).await });

    // Build the shared dispatcher used by all channel adapters.
    let bus_for_dispatch = Arc::clone(&bus);
    let dispatch: adapters::DispatchFn = Arc::new(move |agent, message, session_id| {
        let bus = Arc::clone(&bus_for_dispatch);
        Box::pin(async move {
            bus.route_full(&agent, &message, session_id.as_deref())
                .await
        })
    });

    // Canonical operation HTTP parity dispatcher.
    let config_for_ops = config.clone();
    let op_dispatch: adapters::OpDispatchFn = Arc::new(move |payload| {
        let config = config_for_ops.clone();
        Box::pin(async move {
            let default_agent = config
                .agents
                .first()
                .map(|a| a.name.clone())
                .unwrap_or_else(|| "main".into());

            let req = match build_sidecar_request(payload, &default_agent) {
                Ok(r) => r,
                Err(e) => return Ok(sidecar_error_response("invalid_request", e.to_string())),
            };

            let conn = match open_agent_db(&config, &req.actor.agent_id) {
                Ok(c) => c,
                Err(e) => return Ok(sidecar_error_response("invalid_agent", e.to_string())),
            };

            let resp = match mp_core::operations::execute(&conn, &req) {
                Ok(r) => r,
                Err(e) => {
                    return Ok(sidecar_error_response(
                        "http_ops_execute_error",
                        e.to_string(),
                    ));
                }
            };

            Ok(serde_json::to_value(resp)
                .unwrap_or_else(|e| sidecar_error_response("serialization_error", e.to_string())))
        })
    });

    // Force-enable HTTP channel in serve mode (use gateway port if not explicitly configured)
    let http_cfg = config.channels.http.clone().or_else(|| {
        Some(mp_core::config::HttpChannelConfig {
            port: config.gateway.port,
            api_key: None,
        })
    });
    let slack_cfg = config.channels.slack.clone();
    let discord_cfg = config.channels.discord.clone();
    let default_agent_name = config
        .agents
        .first()
        .map(|a| a.name.clone())
        .unwrap_or_else(|| "main".into());

    {
        let dispatch_clone = Arc::clone(&dispatch);
        let op_dispatch_clone = Arc::clone(&op_dispatch);
        let srv_shutdown = shutdown.subscribe();
        let da = default_agent_name.clone();
        tokio::spawn(async move {
            if let Err(e) = adapters::run_http_server(
                http_cfg.as_ref(),
                slack_cfg.as_ref(),
                discord_cfg.as_ref(),
                da,
                dispatch_clone,
                op_dispatch_clone,
                srv_shutdown,
            )
            .await
            {
                tracing::error!("HTTP server error: {e}");
            }
        });
    }

    // Spawn the Telegram long-polling adapter if configured.
    if let Some(tg_cfg) = config.channels.telegram.clone() {
        let dispatch_clone = Arc::clone(&dispatch);
        let tg_shutdown = shutdown.subscribe();
        let da = default_agent_name.clone();
        tokio::spawn(async move {
            adapters::run_telegram_polling(tg_cfg, da, dispatch_clone, tg_shutdown).await;
        });
    }

    // Spawn the periodic sync loop if configured.
    let has_sync = config.sync.interval_secs > 0
        && (!config.sync.peers.is_empty() || config.sync.cloud_url.is_some());
    if has_sync {
        let sync_config = config.sync.clone();
        let sync_data_dir = config.data_dir.clone();
        let sync_agents: Vec<String> = config.agents.iter().map(|a| a.name.clone()).collect();
        let mut sync_shutdown = shutdown.subscribe();
        tokio::spawn(async move {
            let interval = std::time::Duration::from_secs(sync_config.interval_secs);
            let tables: Vec<&str> = sync_config.tables.iter().map(String::as_str).collect();
            loop {
                tokio::select! {
                    _ = tokio::time::sleep(interval) => {}
                    _ = sync_shutdown.recv() => break,
                }
                for agent_name in &sync_agents {
                    let db_path = sync_data_dir.join(format!("{agent_name}.db"));
                    let conn = match rusqlite::Connection::open(&db_path) {
                        Ok(c) => c,
                        Err(e) => {
                            tracing::warn!("sync: cannot open {agent_name}: {e}");
                            continue;
                        }
                    };
                    if let Err(e) = mp_ext::init_all_extensions(&conn) {
                        tracing::warn!("sync: ext init for {agent_name}: {e}");
                        continue;
                    }
                    let _ = mp_core::sync::init_sync_tables(&conn, &tables);
                    for peer in &sync_config.peers {
                        let peer_path =
                            if std::path::Path::new(peer).is_absolute() || peer.ends_with(".db") {
                                std::path::PathBuf::from(peer)
                            } else {
                                sync_data_dir.join(format!("{peer}.db"))
                            };
                        if !peer_path.exists() {
                            continue;
                        }
                        let peer_conn = match rusqlite::Connection::open(&peer_path).and_then(|c| {
                            mp_ext::init_all_extensions(&c).ok();
                            Ok(c)
                        }) {
                            Ok(c) => c,
                            Err(e) => {
                                tracing::warn!("auto-sync: cannot open peer {peer}: {e}");
                                continue;
                            }
                        };
                        let _ = mp_core::sync::init_sync_tables(&peer_conn, &tables);
                        match mp_core::sync::local_sync_bidirectional(
                            &conn,
                            &peer_conn,
                            agent_name,
                            peer,
                            &tables,
                        ) {
                            Ok(r) => {
                                tracing::debug!(agent = %agent_name, peer = %peer, sent = r.sent, received = r.received, "auto-sync")
                            }
                            Err(e) => {
                                tracing::warn!(agent = %agent_name, peer = %peer, "auto-sync error: {e}")
                            }
                        }
                    }
                    if let Some(ref url) = sync_config.cloud_url {
                        match mp_core::sync::cloud_sync(&conn, url) {
                            Ok(r) => {
                                tracing::debug!(agent = %agent_name, batches = r.sent, "cloud auto-sync")
                            }
                            Err(e) => tracing::warn!(agent = %agent_name, "cloud sync error: {e}"),
                        }
                    }
                }
            }
        });
    }

    // Write PID file for `mp stop`
    let pid_path = config.data_dir.join("mp.pid");
    std::fs::write(&pid_path, std::process::id().to_string())?;

    let http_port = config
        .channels
        .http
        .as_ref()
        .map(|h| h.port)
        .unwrap_or(config.gateway.port);
    tracing::info!(
        agents = config.agents.len(),
        http_port = http_port,
        "serve mode ready — MCP on stdio, HTTP on port {http_port}"
    );

    // ── MCP sidecar loop on stdio (replaces the CLI chat loop from cmd_start) ──
    let ag = resolve_agent(config, agent.as_deref())?;
    let conn = open_agent_db(config, &ag.name)?;
    let embed_provider = build_embedding_provider(config, ag).ok();
    let sidecar_embedding_model_id = embedding_model_id(ag);

    let stdin = tokio::io::stdin();
    let reader = tokio::io::BufReader::new(stdin);
    let mut lines = tokio::io::AsyncBufReadExt::lines(reader);
    let mut stdout = tokio::io::stdout();
    let mut shutdown_rx = shutdown.subscribe();

    loop {
        tokio::select! {
            line_result = lines.next_line() => {
                match line_result {
                    Ok(Some(line)) => {
                        let parsed: serde_json::Value = match serde_json::from_str(&line) {
                            Ok(v) => v,
                            Err(e) => {
                                let err = sidecar_error_response("invalid_json", e.to_string());
                                tokio::io::AsyncWriteExt::write_all(&mut stdout, format!("{err}\n").as_bytes()).await?;
                                tokio::io::AsyncWriteExt::flush(&mut stdout).await?;
                                continue;
                            }
                        };

                        if let Some(mcp_response) = sidecar::handle_sidecar_mcp_request(
                            &conn, &parsed, &ag.name,
                            embed_provider.as_deref(), &sidecar_embedding_model_id,
                        ).await? {
                            tokio::io::AsyncWriteExt::write_all(&mut stdout, format!("{mcp_response}\n").as_bytes()).await?;
                            tokio::io::AsyncWriteExt::flush(&mut stdout).await?;
                            continue;
                        }

                        if parsed.get("method").is_some() && parsed.get("id").is_none() {
                            continue;
                        }

                        let request = match build_sidecar_request(parsed, &ag.name) {
                            Ok(r) => r,
                            Err(e) => {
                                let err = sidecar_error_response("invalid_request", e.to_string());
                                tokio::io::AsyncWriteExt::write_all(&mut stdout, format!("{err}\n").as_bytes()).await?;
                                tokio::io::AsyncWriteExt::flush(&mut stdout).await?;
                                continue;
                            }
                        };

                        let response = match sidecar::execute_sidecar_operation(
                            &conn, &request,
                            embed_provider.as_deref(), &sidecar_embedding_model_id,
                        ).await {
                            Ok(resp) => serde_json::to_value(resp)
                                .unwrap_or_else(|e| sidecar_error_response("serialization_error", e.to_string())),
                            Err(e) => sidecar_error_response("sidecar_execute_error", e.to_string()),
                        };

                        tokio::io::AsyncWriteExt::write_all(&mut stdout, format!("{response}\n").as_bytes()).await?;
                        tokio::io::AsyncWriteExt::flush(&mut stdout).await?;
                    }
                    Ok(None) => break, // stdin closed
                    Err(e) => {
                        tracing::error!("stdin read error: {e}");
                        break;
                    }
                }
            }
            _ = shutdown_rx.recv() => break,
        }
    }

    // Graceful shutdown
    tracing::info!("shutting down serve mode");
    let _ = shutdown.send(());
    scheduler_handle.abort();
    for mut w in workers {
        w.shutdown().await;
    }
    let _ = std::fs::remove_file(&pid_path);
    Ok(())
}

async fn cmd_stop(config: &Config) -> Result<()> {
    let pid_path = config.data_dir.join("mp.pid");
    if !pid_path.exists() {
        println!(
            "  No running gateway found (no PID file at {}).",
            pid_path.display()
        );
        return Ok(());
    }

    let pid_str = std::fs::read_to_string(&pid_path)?;
    let pid: i32 = pid_str
        .trim()
        .parse()
        .map_err(|e| anyhow::anyhow!("invalid PID in {}: {e}", pid_path.display()))?;

    println!("  Sending SIGTERM to gateway (pid {pid})...");
    #[cfg(unix)]
    {
        let status = unsafe { libc::kill(pid, libc::SIGTERM) };
        if status == 0 {
            println!("  Signal sent. Gateway should shut down gracefully.");
            let _ = std::fs::remove_file(&pid_path);
        } else {
            let errno = std::io::Error::last_os_error();
            if errno.raw_os_error() == Some(libc::ESRCH) {
                println!("  Process {pid} not found. Cleaning up stale PID file.");
                let _ = std::fs::remove_file(&pid_path);
            } else {
                anyhow::bail!("Failed to send signal to pid {pid}: {errno}");
            }
        }
    }
    #[cfg(not(unix))]
    {
        println!("  Signal-based stop is only supported on Unix. Kill process {pid} manually.");
    }
    Ok(())
}

// WorkerBus, WorkerHandle, spawn_worker, cmd_worker, run_scheduler — see worker module

// =========================================================================
// Agent
// =========================================================================

async fn cmd_agent(config: &Config, cmd: cli::AgentCommand) -> Result<()> {
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

// =========================================================================
// Chat & Send
// =========================================================================

async fn cmd_chat(
    config: &Config,
    agent: Option<String>,
    session_id: Option<String>,
    force_new: bool,
) -> Result<()> {
    let ag = resolve_agent(config, agent.as_deref())?;
    let conn = open_agent_db(config, &ag.name)?;
    let provider = build_provider(ag)?;
    let embed = build_embedding_provider(config, ag).ok();
    let (mut sid, resumed) =
        resolve_or_create_session(&conn, &ag.name, Some("cli"), session_id, force_new)?;

    ui::blank();
    if ui::styled() {
        use owo_colors::OwoColorize;
        println!(
            "  {} v{} — agent: {}",
            "Moneypenny".bold(),
            env!("CARGO_PKG_VERSION"),
            ag.name
        );
    } else {
        println!(
            "  Moneypenny v{} — agent: {}",
            env!("CARGO_PKG_VERSION"),
            ag.name
        );
    }
    ui::field("LLM", 11, format!(
        "{} ({})",
        ag.llm.provider,
        ag.llm.model.as_deref().unwrap_or("default")
    ));
    ui::field("Embedding", 11, format!(
        "{} ({}, {}D)",
        ag.embedding.provider, ag.embedding.model, ag.embedding.dimensions
    ));
    if resumed {
        let msg_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM messages WHERE session_id = ?1",
                [&sid],
                |r| r.get(0),
            )
            .unwrap_or(0);
        ui::dim(format!(
            "Resumed session ({msg_count} messages). Use /new for a fresh session."
        ));
    }
    ui::info("Type /help for commands, Ctrl-C to exit.");
    ui::blank();

    let stdin = std::io::stdin();
    loop {
        ui::prompt();

        let mut line = String::new();
        if stdin.read_line(&mut line)? == 0 {
            break;
        }
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        match line {
            "/quit" | "/exit" => break,
            "/help" => {
                ui::info("/facts    — list stored facts");
                ui::info("/scratch  — list scratch entries");
                ui::info("/session  — show session info");
                ui::info("/new      — start a fresh session");
                ui::info("/quit     — exit chat");
                ui::blank();
                continue;
            }
            "/facts" => {
                let facts = mp_core::store::facts::list_active(&conn, &ag.name)?;
                if facts.is_empty() {
                    ui::info("No facts stored.");
                } else {
                    for f in &facts {
                        ui::info(format!("[{:.1}] {}", f.confidence, f.pointer));
                    }
                }
                ui::blank();
                continue;
            }
            "/scratch" => {
                let entries = mp_core::store::scratch::list(&conn, &sid)?;
                if entries.is_empty() {
                    ui::info("Scratch is empty.");
                } else {
                    for e in &entries {
                        let preview: String = e.content.chars().take(60).collect();
                        ui::info(format!("[{}] {}", e.key, preview));
                    }
                }
                ui::blank();
                continue;
            }
            "/session" => {
                let msgs = mp_core::store::log::get_messages(&conn, &sid)?;
                ui::info(format!("Session: {sid}"));
                ui::info(format!("Messages: {}", msgs.len()));
                ui::blank();
                continue;
            }
            "/new" => {
                sid = mp_core::store::log::create_session(&conn, &ag.name, Some("cli"))?;
                ui::success("Started fresh session.");
                ui::blank();
                continue;
            }
            _ => {}
        }

        match agent::agent_turn(
            &conn,
            provider.as_ref(),
            embed.as_deref(),
            &ag.name,
            &sid,
            ag.persona.as_deref(),
            line,
            ag.policy_mode(),
            None,
        )
        .await
        {
            Ok(response) => {
                ui::blank();
                for resp_line in response.lines() {
                    ui::info(resp_line);
                }
                ui::blank();

                match extract_facts(&conn, provider.as_ref(), &ag.name, &sid).await {
                    Ok(n) if n > 0 => {
                        ui::dim(format!("({n} fact{} learned)", if n == 1 { "" } else { "s" }));
                        ui::blank();
                    }
                    Err(e) => tracing::debug!("extraction error: {e}"),
                    _ => {}
                }
                if let Some(ref ep) = embed {
                    let model_id = embedding_model_id(ag);
                    embed_pending(&conn, ep.as_ref(), &ag.name, &model_id).await;
                }
                maybe_summarize_session(&conn, provider.as_ref(), &sid).await;
            }
            Err(e) => {
                ui::error(e);
                ui::blank();
            }
        }
    }

    ui::info(format!("Session {sid} ended."));
    Ok(())
}

async fn cmd_send(
    config: &Config,
    agent_name: &str,
    message: &str,
    session_id: Option<String>,
) -> Result<()> {
    let agent = resolve_agent(config, Some(agent_name))?;
    let conn = open_agent_db(config, &agent.name)?;
    let provider = build_provider(agent)?;
    let embed = build_embedding_provider(config, agent).ok();
    let (sid, _) = resolve_or_create_session(&conn, &agent.name, Some("cli"), session_id, false)?;

    let response = agent::agent_turn(
        &conn,
        provider.as_ref(),
        embed.as_deref(),
        &agent.name,
        &sid,
        agent.persona.as_deref(),
        message,
        agent.policy_mode(),
        None,
    )
    .await?;

    ui::blank();
    for line in response.lines() {
        ui::info(line);
    }
    ui::blank();

    if let Ok(n) = extract_facts(&conn, provider.as_ref(), &agent.name, &sid).await {
        if n > 0 {
            ui::dim(format!("({n} fact{} learned)", if n == 1 { "" } else { "s" }));
            ui::blank();
        }
    }
    if let Some(ref ep) = embed {
        let model_id = embedding_model_id(agent);
        embed_pending(&conn, ep.as_ref(), &agent.name, &model_id).await;
    }
    maybe_summarize_session(&conn, provider.as_ref(), &sid).await;

    Ok(())
}

// =========================================================================
// Facts
// =========================================================================

async fn cmd_facts(config: &Config, cmd: cli::FactsCommand) -> Result<()> {
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

// =========================================================================
// Ingest
// =========================================================================

async fn cmd_ingest(
    config: &Config,
    path: Option<String>,
    url: Option<String>,
    agent: Option<String>,
    openclaw_file: Option<String>,
    replay: bool,
    status: bool,
    replay_run: Option<String>,
    replay_latest: bool,
    replay_offset: usize,
    status_filter: Option<String>,
    file_filter: Option<String>,
    dry_run: bool,
    apply: bool,
    source: String,
    limit: usize,
    cortex: bool,
    claude_code: Option<String>,
    cursor: Option<String>,
) -> Result<()> {
    let ag = resolve_agent(config, agent.as_deref())?;
    let conn = open_agent_db(config, &ag.name)?;

    if status {
        let req = op_request(
            &ag.name,
            "ingest.status",
            serde_json::json!({
                "source": source.clone(),
                "status": status_filter.clone(),
                "file_path_like": file_filter.clone(),
                "limit": limit
            }),
        );
        let resp = mp_core::operations::execute(&conn, &req)?;
        let rows = resp.data.as_array().cloned().unwrap_or_default();
        ui::blank();
        if rows.is_empty() {
            ui::info("No ingest runs found.");
        } else {
            ui::table_header(&[("RUN_ID", 36), ("SOURCE", 10), ("STATUS", 22), ("PROC", 8), ("INS", 8), ("DEDUP", 8), ("PROJ", 8), ("ERR", 8)]);
            for r in rows {
                println!(
                    "  {:36} {:10} {:22} {:8} {:8} {:8} {:8} {:8}",
                    r["id"].as_str().unwrap_or("-"),
                    r["source"].as_str().unwrap_or("-"),
                    r["status"].as_str().unwrap_or("-"),
                    r["processed_count"].as_i64().unwrap_or(0),
                    r["inserted_count"].as_i64().unwrap_or(0),
                    r["deduped_count"].as_i64().unwrap_or(0),
                    r["projected_count"].as_i64().unwrap_or(0),
                    r["error_count"].as_i64().unwrap_or(0),
                );
            }
        }
        ui::blank();
    } else if replay_run.is_some() || replay_latest {
        let selected_run_id = if let Some(run_id) = replay_run {
            run_id
        } else {
            let status_req = op_request(
                &ag.name,
                "ingest.status",
                serde_json::json!({
                    "source": source.clone(),
                    "status": status_filter.clone(),
                    "file_path_like": file_filter.clone(),
                    "limit": limit
                }),
            );
            let status_resp = mp_core::operations::execute(&conn, &status_req)?;
            let rows = status_resp.data.as_array().cloned().unwrap_or_default();
            let selected = rows.get(replay_offset).cloned().ok_or_else(|| {
                anyhow::anyhow!(
                    "no ingest run available at replay offset {} (after filters)",
                    replay_offset
                )
            })?;
            selected["id"]
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| anyhow::anyhow!("selected ingest run has no id"))?
        };

        // Operator-safe default: preview first; require --apply to execute writes.
        let effective_dry_run = if dry_run { true } else { !apply };
        let req = op_request(
            &ag.name,
            "ingest.replay",
            serde_json::json!({
                "run_id": selected_run_id,
                "dry_run": effective_dry_run
            }),
        );
        let resp = mp_core::operations::execute(&conn, &req)?;
        if !resp.ok {
            anyhow::bail!("replay denied: {}", resp.message);
        }
        if effective_dry_run {
            ui::info(format!(
                "Replay preview {}: processed={}, would_insert={}, would_dedupe={}, parse_errors={}, lines={}..{} (use --apply to execute)",
                resp.data["run_id"].as_str().unwrap_or("-"),
                resp.data["processed_count"].as_i64().unwrap_or(0),
                resp.data["would_insert_count"].as_i64().unwrap_or(0),
                resp.data["would_dedupe_count"].as_i64().unwrap_or(0),
                resp.data["parse_error_count"].as_i64().unwrap_or(0),
                resp.data["from_line"].as_i64().unwrap_or(0),
                resp.data["to_line"].as_i64().unwrap_or(0),
            ));
        } else {
            ui::success(format!(
                "Replay run {}: processed={}, inserted={}, deduped={}, projected={}, errors={}",
                resp.data["run_id"].as_str().unwrap_or("-"),
                resp.data["processed_count"].as_i64().unwrap_or(0),
                resp.data["inserted_count"].as_i64().unwrap_or(0),
                resp.data["deduped_count"].as_i64().unwrap_or(0),
                resp.data["projected_count"].as_i64().unwrap_or(0),
                resp.data["error_count"].as_i64().unwrap_or(0),
            ));
        }
    } else if let Some(file) = openclaw_file {
        let req = op_request(
            &ag.name,
            "ingest.events",
            serde_json::json!({
                "source": source,
                "file_path": file,
                "replay": replay,
            }),
        );
        let resp = mp_core::operations::execute(&conn, &req)?;
        if !resp.ok {
            anyhow::bail!("external ingest denied: {}", resp.message);
        }
        ui::success(format!(
            "Ingest run {}: processed={}, inserted={}, deduped={}, projected={}, errors={}",
            resp.data["run_id"].as_str().unwrap_or("-"),
            resp.data["processed_count"].as_i64().unwrap_or(0),
            resp.data["inserted_count"].as_i64().unwrap_or(0),
            resp.data["deduped_count"].as_i64().unwrap_or(0),
            resp.data["projected_count"].as_i64().unwrap_or(0),
            resp.data["error_count"].as_i64().unwrap_or(0),
        ));
    } else if cortex {
        let sessions = mp_core::ingest::discover_cortex_sessions();
        if sessions.is_empty() {
            ui::info("No Cortex Code conversations found in ~/.snowflake/cortex/conversations/");
            return Ok(());
        }
        ui::info(format!("Found {} Cortex Code session(s)", sessions.len()));
        ui::blank();
        let mut total_inserted = 0i64;
        let mut total_deduped = 0i64;
        let mut total_errors = 0i64;
        for session_path in &sessions {
            let lines = match mp_core::ingest::convert_cortex_session(session_path) {
                Ok(l) => l,
                Err(e) => {
                    ui::warn(format!(
                        "Skipping {:?}: {}",
                        session_path.file_name().unwrap_or_default(),
                        e
                    ));
                    total_errors += 1;
                    continue;
                }
            };
            if lines.is_empty() {
                continue;
            }
            let tmp = mp_core::ingest::write_temp_jsonl(&lines, "cortex")?;
            let summary =
                mp_core::ingest::ingest_jsonl_file(&conn, "cortex", &tmp, replay, &ag.name)?;
            let fname = session_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy();
            ui::info(format!(
                "{}: inserted={}, deduped={}, projected={}, errors={}",
                fname,
                summary.inserted_count,
                summary.deduped_count,
                summary.projected_count,
                summary.error_count,
            ));
            total_inserted += summary.inserted_count;
            total_deduped += summary.deduped_count;
            total_errors += summary.error_count;
            let _ = std::fs::remove_file(&tmp);
        }
        ui::blank();
        ui::dim(format!(
            "Total: {} sessions, {} inserted, {} deduped, {} errors",
            sessions.len(),
            total_inserted,
            total_deduped,
            total_errors
        ));
    } else if claude_code.is_some() {
        let slug = claude_code.as_deref().filter(|s| !s.is_empty());
        let sessions = mp_core::ingest::discover_claude_code_sessions(slug);
        if sessions.is_empty() {
            if let Some(s) = slug {
                ui::info(format!("No Claude Code sessions found for project slug: {s}"));
            } else {
                ui::info("No Claude Code sessions found in ~/.claude/projects/");
            }
            return Ok(());
        }
        ui::info(format!("Found {} Claude Code session(s)", sessions.len()));
        ui::blank();
        let mut total_inserted = 0i64;
        let mut total_deduped = 0i64;
        let mut total_errors = 0i64;
        for session_path in &sessions {
            let lines = match mp_core::ingest::convert_claude_code_session(session_path) {
                Ok(l) => l,
                Err(e) => {
                    ui::warn(format!(
                        "Skipping {:?}: {}",
                        session_path.file_name().unwrap_or_default(),
                        e
                    ));
                    total_errors += 1;
                    continue;
                }
            };
            if lines.is_empty() {
                continue;
            }
            let tmp = mp_core::ingest::write_temp_jsonl(&lines, "claude-code")?;
            let summary =
                mp_core::ingest::ingest_jsonl_file(&conn, "claude-code", &tmp, replay, &ag.name)?;
            let fname = session_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy();
            ui::info(format!(
                "{}: inserted={}, deduped={}, projected={}, errors={}",
                fname,
                summary.inserted_count,
                summary.deduped_count,
                summary.projected_count,
                summary.error_count,
            ));
            total_inserted += summary.inserted_count;
            total_deduped += summary.deduped_count;
            total_errors += summary.error_count;
            let _ = std::fs::remove_file(&tmp);
        }
        ui::blank();
        ui::dim(format!(
            "Total: {} sessions, {} inserted, {} deduped, {} errors",
            sessions.len(),
            total_inserted,
            total_deduped,
            total_errors
        ));
    } else if cursor.is_some() {
        let slug = cursor.as_deref().filter(|s| !s.is_empty());
        let sessions = mp_core::ingest::discover_cursor_sessions(slug);
        if sessions.is_empty() {
            if let Some(s) = slug {
                ui::info(format!("No Cursor sessions found for project slug: {s}"));
            } else {
                ui::info("No Cursor sessions found in ~/.cursor/projects/");
            }
            return Ok(());
        }
        ui::info(format!("Found {} Cursor session(s)", sessions.len()));
        ui::blank();
        let mut total_inserted = 0i64;
        let mut total_deduped = 0i64;
        let mut total_errors = 0i64;
        for session_path in &sessions {
            let lines = match mp_core::ingest::convert_cursor_session(session_path) {
                Ok(l) => l,
                Err(e) => {
                    ui::warn(format!(
                        "Skipping {:?}: {}",
                        session_path.file_name().unwrap_or_default(),
                        e
                    ));
                    total_errors += 1;
                    continue;
                }
            };
            if lines.is_empty() {
                continue;
            }
            let tmp = mp_core::ingest::write_temp_jsonl(&lines, "cursor")?;
            let summary =
                mp_core::ingest::ingest_jsonl_file(&conn, "cursor", &tmp, replay, &ag.name)?;
            let fname = session_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy();
            ui::info(format!(
                "{}: inserted={}, deduped={}, projected={}, errors={}",
                fname,
                summary.inserted_count,
                summary.deduped_count,
                summary.projected_count,
                summary.error_count,
            ));
            total_inserted += summary.inserted_count;
            total_deduped += summary.deduped_count;
            total_errors += summary.error_count;
            let _ = std::fs::remove_file(&tmp);
        }
        ui::blank();
        ui::dim(format!(
            "Total: {} sessions, {} inserted, {} deduped, {} errors",
            sessions.len(),
            total_inserted,
            total_deduped,
            total_errors
        ));
    } else if let Some(p) = path {
        let content = std::fs::read_to_string(&p)?;
        let title = Path::new(&p)
            .file_name()
            .map(|n| n.to_string_lossy().to_string());
        let req = op_request(
            &ag.name,
            "knowledge.ingest",
            serde_json::json!({
                "path": p.clone(),
                "title": title,
                "content": content,
            }),
        );
        let resp = mp_core::operations::execute(&conn, &req)?;
        if !resp.ok {
            anyhow::bail!("ingest denied: {}", resp.message);
        }
        let doc_id = resp.data["document_id"].as_str().unwrap_or("-");
        let chunks = resp.data["chunks_created"].as_u64().unwrap_or(0);
        ui::success(format!("Ingested {p}: {chunks} chunks (doc {doc_id})"));
        if let Ok(ep) = build_embedding_provider(config, ag) {
            let model_id = embedding_model_id(ag);
            embed_pending(&conn, ep.as_ref(), &ag.name, &model_id).await;
        }
    } else if let Some(u) = url {
        ui::info(format!("Ingesting URL {u} …"));

        let req = op_request(
            &ag.name,
            "knowledge.ingest",
            serde_json::json!({
                "path": u,
            }),
        );
        let resp = mp_core::operations::execute(&conn, &req)?;
        if !resp.ok {
            anyhow::bail!("ingest denied: {}", resp.message);
        }
        let doc_id = resp.data["document_id"].as_str().unwrap_or("-");
        let chunks = resp.data["chunks_created"].as_u64().unwrap_or(0);
        ui::success(format!("Ingested {u}: {chunks} chunks (doc {doc_id})"));
        if let Ok(ep) = build_embedding_provider(config, ag) {
            let model_id = embedding_model_id(ag);
            embed_pending(&conn, ep.as_ref(), &ag.name, &model_id).await;
        }
    } else {
        anyhow::bail!("Provide a path, --openclaw-file, or --url to ingest.");
    }
    Ok(())
}

// MCP sidecar, JSON-RPC protocol, tool stats, embedding operations — see sidecar module

// =========================================================================
// Knowledge
// =========================================================================

async fn cmd_knowledge(config: &Config, cmd: cli::KnowledgeCommand) -> Result<()> {
    let ag = resolve_agent(config, None)?;
    let conn = open_agent_db(config, &ag.name)?;

    match cmd {
        cli::KnowledgeCommand::Search { query } => {
            let results = mp_core::search::fts5_search_knowledge(&conn, &query, 20)?;
            ui::blank();
            if results.is_empty() {
                ui::info(format!("No knowledge results for \"{query}\"."));
            } else {
                for (id, content, _score) in &results {
                    let preview: String = content.chars().take(80).collect();
                    ui::info(format!("{id}: {preview}"));
                }
            }
            ui::blank();
        }
        cli::KnowledgeCommand::List => {
            let docs = mp_core::store::knowledge::list_documents(&conn)?;
            ui::blank();
            if docs.is_empty() {
                ui::info("No documents ingested.");
            } else {
                ui::table_header(&[("ID", 36), ("TITLE", 30), ("PATH", 20)]);
                for d in &docs {
                    println!(
                        "  {:36} {:30} {:20}",
                        d.id,
                        d.title.as_deref().unwrap_or("-"),
                        d.path.as_deref().unwrap_or("-"),
                    );
                }
            }
            ui::blank();
        }
    }
    Ok(())
}

// =========================================================================
// Skill
// =========================================================================

async fn cmd_skill(config: &Config, cmd: cli::SkillCommand) -> Result<()> {
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

// =========================================================================
// Policy
// =========================================================================

async fn cmd_policy(config: &Config, cmd: cli::PolicyCommand) -> Result<()> {
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

// parse_duration_hours, normalize_embedding_target — see helpers module

// =========================================================================
// Job
// =========================================================================

async fn cmd_job(config: &Config, cmd: cli::JobCommand) -> Result<()> {
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

// =========================================================================
// Embeddings
// =========================================================================

async fn cmd_embeddings(config: &Config, cmd: cli::EmbeddingsCommand) -> Result<()> {
    match cmd {
        cli::EmbeddingsCommand::Status { agent } => {
            let ag = resolve_agent(config, agent.as_deref())?;
            let conn = open_agent_db(config, &ag.name)?;
            let stats = mp_core::store::embedding::queue_stats(&conn)?;
            let by_target = mp_core::store::embedding::queue_target_stats(&conn)?;

            ui::blank();
            ui::info(format!("Embedding queue status (agent: {})", ag.name));
            ui::info(format!(
                "total={} pending={} retry={} processing={} dead={}",
                stats.total, stats.pending, stats.retry, stats.processing, stats.dead
            ));
            if by_target.is_empty() {
                ui::info("No queue entries.");
            } else {
                ui::blank();
                ui::table_header(&[("TARGET", 14), ("TOTAL", 7), ("PENDING", 7), ("RETRY", 7), ("PROCESSING", 10), ("DEAD", 7)]);
                for row in &by_target {
                    println!(
                        "  {:14} {:7} {:7} {:7} {:10} {:7}",
                        row.target, row.total, row.pending, row.retry, row.processing, row.dead
                    );
                }
            }
            ui::blank();
        }
        cli::EmbeddingsCommand::RetryDead {
            agent,
            target,
            limit,
        } => {
            let ag = resolve_agent(config, agent.as_deref())?;
            let conn = open_agent_db(config, &ag.name)?;
            let target_norm = if let Some(raw) = target.as_deref() {
                Some(normalize_embedding_target(raw).ok_or_else(|| {
                    anyhow::anyhow!(
                        "Unknown --target value. Use one of: facts, messages, tool_calls, policy_audit, chunks"
                    )
                })?)
            } else {
                None
            };

            let revived = mp_core::store::embedding::retry_dead_jobs(&conn, target_norm, limit)?;
            ui::success(format!(
                "Revived {revived} dead embedding job{} for agent \"{}\"{}.",
                if revived == 1 { "" } else { "s" },
                ag.name,
                target_norm
                    .map(|t| format!(" (target={t})"))
                    .unwrap_or_default()
            ));
        }
        cli::EmbeddingsCommand::Backfill {
            agent,
            model,
            limit,
            batch_size,
            enqueue_only,
        } => {
            let ag = resolve_agent(config, agent.as_deref())?;
            let conn = open_agent_db(config, &ag.name)?;

            let embed_provider =
                build_embedding_provider_with_override(config, ag, model.as_deref())?;
            let model_name = model.as_deref().unwrap_or(&ag.embedding.model).to_string();
            let model_id = mp_core::store::embedding::model_identity(
                &ag.embedding.provider,
                &model_name,
                ag.embedding.dimensions,
            );

            let queued =
                mp_core::store::embedding::enqueue_drift_jobs(&conn, &ag.name, &model_id, limit)?;
            ui::info(format!(
                "Enqueued {queued} backfill candidat{} for agent \"{}\" using model \"{}\".",
                if queued == 1 { "e" } else { "es" },
                ag.name,
                model_name
            ));

            if enqueue_only {
                return Ok(());
            }

            let mut total_embedded = 0usize;
            let mut total_failed = 0usize;
            let mut rounds = 0usize;
            let embed_provider_ref = embed_provider.as_ref();
            loop {
                rounds += 1;
                let stats = mp_core::store::embedding::process_embedding_jobs(
                    &conn,
                    &ag.name,
                    &model_id,
                    batch_size.max(1),
                    5,
                    8,
                    |content| async move {
                        let vec = embed_provider_ref.embed(&content).await?;
                        Ok::<Vec<u8>, anyhow::Error>(mp_llm::f32_slice_to_blob(&vec))
                    },
                )
                .await?;

                total_embedded += stats.embedded;
                total_failed += stats.failed;

                if stats.claimed == 0 {
                    break;
                }
                if rounds >= 10_000 {
                    break;
                }
            }

            let queue = mp_core::store::embedding::queue_stats(&conn)?;
            ui::success(format!(
                "Backfill run complete: embedded={}, failed={}, queue pending={} retry={} processing={} dead={}.",
                total_embedded,
                total_failed,
                queue.pending,
                queue.retry,
                queue.processing,
                queue.dead
            ));
        }
    }
    Ok(())
}

// =========================================================================
// Audit
// =========================================================================

async fn cmd_audit(
    config: &Config,
    _agent: Option<String>,
    command: Option<cli::AuditCommand>,
) -> Result<()> {
    let ag = resolve_agent(config, None)?;
    let conn = open_agent_db(config, &ag.name)?;

    match command {
        None => {
            let req = op_request(
                &ag.name,
                "audit.query",
                serde_json::json!({
                    "limit": 20
                }),
            );
            let resp = mp_core::operations::execute(&conn, &req)?;
            if !resp.ok {
                ui::warn(format!("Audit query denied: {}", resp.message));
                return Ok(());
            }
            let entries = resp.data.as_array().cloned().unwrap_or_default();

            ui::blank();
            if entries.is_empty() {
                ui::info("No audit entries.");
            } else {
                for e in &entries {
                    ui::info(format!(
                        "[{effect}] {actor} → {action} on {resource}: {}",
                        e["reason"].as_str().unwrap_or(""),
                        effect = e["effect"].as_str().unwrap_or(""),
                        actor = e["actor"].as_str().unwrap_or(""),
                        action = e["action"].as_str().unwrap_or(""),
                        resource = e["resource"].as_str().unwrap_or(""),
                    ));
                }
            }
            ui::blank();
        }
        Some(cli::AuditCommand::Search { query, since, until }) => {
            let req = op_request(
                &ag.name,
                "audit.query",
                serde_json::json!({
                    "query": query,
                    "since": since,
                    "until": until,
                    "limit": 20
                }),
            );
            let resp = mp_core::operations::execute(&conn, &req)?;
            if !resp.ok {
                ui::warn(format!("Audit query denied: {}", resp.message));
                return Ok(());
            }
            let entries = resp.data.as_array().cloned().unwrap_or_default();

            ui::blank();
            for e in &entries {
                ui::info(format!(
                    "[{effect}] {actor} → {action} on {resource}: {}",
                    e["reason"].as_str().unwrap_or(""),
                    effect = e["effect"].as_str().unwrap_or(""),
                    actor = e["actor"].as_str().unwrap_or(""),
                    action = e["action"].as_str().unwrap_or(""),
                    resource = e["resource"].as_str().unwrap_or(""),
                ));
            }
            ui::blank();
        }
        Some(cli::AuditCommand::Export {
            format,
            since,
            until,
        }) => {
            let req = op_request(
                &ag.name,
                "audit.query",
                serde_json::json!({
                    "since": since,
                    "until": until,
                    "limit": 10000
                }),
            );
            let resp = mp_core::operations::execute(&conn, &req)?;
            if !resp.ok {
                anyhow::bail!("Audit export denied: {}", resp.message);
            }
            let entries = resp.data.as_array().cloned().unwrap_or_default();

            match format.as_str() {
                "json" => {
                    println!("{}", serde_json::to_string_pretty(&entries)?);
                }
                "csv" => {
                    println!(
                        "id,actor,action,resource,effect,reason,session_id,created_at,correlation_id"
                    );
                    for e in &entries {
                        println!(
                            "{},{},{},{},{},{},{},{},{}",
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
                }
                "sql" => {
                    for e in &entries {
                        println!(
                            "INSERT INTO policy_audit (id, actor, action, resource, effect, reason, session_id, created_at, correlation_id) VALUES ({}, {}, {}, {}, {}, {}, {}, {}, {});",
                            sql_quote(e["id"].as_str().unwrap_or("")),
                            sql_quote(e["actor"].as_str().unwrap_or("")),
                            sql_quote(e["action"].as_str().unwrap_or("")),
                            sql_quote(e["resource"].as_str().unwrap_or("")),
                            sql_quote(e["effect"].as_str().unwrap_or("")),
                            sql_quote(e["reason"].as_str().unwrap_or("")),
                            sql_quote(e["session_id"].as_str().unwrap_or("")),
                            e["created_at"].as_i64().unwrap_or(0),
                            sql_quote(e["correlation_id"].as_str().unwrap_or("")),
                        );
                    }
                }
                other => {
                    anyhow::bail!("Unsupported export format: {other}. Use json, csv, or sql.");
                }
            }
        }
    }
    Ok(())
}

// =========================================================================
// Sync
// =========================================================================

async fn cmd_sync(config: &Config, cmd: cli::SyncCommand) -> Result<()> {
    let sync_tables: Vec<&str> = config.sync.tables.iter().map(String::as_str).collect();

    match cmd {
        // ------------------------------------------------------------------
        // Status
        // ------------------------------------------------------------------
        cli::SyncCommand::Status { agent } => {
            let ag = resolve_agent(config, agent.as_deref())?;
            let conn = open_agent_db(config, &ag.name)?;
            let st = mp_core::sync::status(&conn, &sync_tables)?;
            ui::blank();
            ui::info(format!("Sync status for agent \"{}\"", ag.name));
            println!("{st}");
        }

        // ------------------------------------------------------------------
        // Now — bidirectional sync with all configured peers + cloud
        // ------------------------------------------------------------------
        cli::SyncCommand::Now { agent } => {
            let ag = resolve_agent(config, agent.as_deref())?;
            let conn = open_agent_db(config, &ag.name)?;

            let mut total_sent = 0usize;
            let mut total_received = 0usize;

            // Local peer sync
            for peer in &config.sync.peers {
                let peer_path = resolve_peer_path(config, peer);
                if !peer_path.exists() {
                    ui::warn(format!("Peer DB not found: {}", peer_path.display()));
                    continue;
                }
                print!("  Syncing with peer \"{}\"… ", peer);
                ui::flush();
                let peer_conn = match open_peer_db(&peer_path, &sync_tables) {
                    Ok(c) => c,
                    Err(e) => {
                        ui::error(format!("error opening peer: {e}"));
                        continue;
                    }
                };
                match mp_core::sync::local_sync_bidirectional(
                    &conn,
                    &peer_conn,
                    &ag.name,
                    peer,
                    &sync_tables,
                ) {
                    Ok(r) => {
                        println!("sent {}B, received {}B", r.sent, r.received);
                        total_sent += r.sent;
                        total_received += r.received;
                    }
                    Err(e) => ui::error(format!("sync error: {e}")),
                }
            }

            if let Some(ref url) = config.sync.cloud_url {
                print!("  Cloud sync… ");
                ui::flush();
                match mp_core::sync::cloud_sync(&conn, url) {
                    Ok(r) => {
                        println!("{} batch(es)", r.sent);
                        total_sent += r.sent;
                    }
                    Err(e) => ui::error(format!("cloud sync error: {e}")),
                }
            }

            if config.sync.peers.is_empty() && config.sync.cloud_url.is_none() {
                ui::info("No peers or cloud URL configured.");
                ui::info("Add [sync] peers = [\"other-agent\"] or cloud_url = \"…\" to moneypenny.toml");
            } else {
                ui::blank();
                ui::success(format!(
                    "Sync complete. Sent {}B, received {}B.",
                    total_sent, total_received
                ));
            }
        }

        // ------------------------------------------------------------------
        // Push — one-way: this agent → target peer
        // ------------------------------------------------------------------
        cli::SyncCommand::Push { to, agent } => {
            let ag = resolve_agent(config, agent.as_deref())?;
            let conn = open_agent_db(config, &ag.name)?;
            let peer_path = resolve_peer_path(config, &to);
            if !peer_path.exists() {
                anyhow::bail!("target DB not found: {}", peer_path.display());
            }
            print!("  Pushing \"{}\" → \"{}\"… ", ag.name, to);
            ui::flush();
            let peer_conn = open_peer_db(&peer_path, &sync_tables)?;
            let r = mp_core::sync::local_sync_push(&conn, &peer_conn, &to, &sync_tables)?;
            println!("sent {}B", r.sent);
        }

        // ------------------------------------------------------------------
        // Pull — one-way: source peer → this agent
        // ------------------------------------------------------------------
        cli::SyncCommand::Pull { from, agent } => {
            let ag = resolve_agent(config, agent.as_deref())?;
            let conn = open_agent_db(config, &ag.name)?;
            let peer_path = resolve_peer_path(config, &from);
            if !peer_path.exists() {
                anyhow::bail!("source DB not found: {}", peer_path.display());
            }
            print!("  Pulling \"{}\" → \"{}\"… ", from, ag.name);
            ui::flush();
            let peer_conn = open_peer_db(&peer_path, &sync_tables)?;
            let r = mp_core::sync::local_sync_pull(&conn, &peer_conn, &ag.name, &sync_tables)?;
            println!("received {}B", r.received);
        }

        // ------------------------------------------------------------------
        // Connect — store cloud URL in the live config file
        // ------------------------------------------------------------------
        cli::SyncCommand::Connect { url, agent: _ } => {
            // Find the config file path from the CLI args (already resolved by main)
            // and update the [sync] cloud_url key.
            ui::info(format!("Cloud sync URL set to: {url}"));
            ui::info("Add this to your moneypenny.toml:");
            ui::blank();
            ui::hint("[sync]");
            ui::hint(format!("cloud_url = \"{url}\""));
            ui::blank();
            ui::info("Then run `mp sync now` to trigger an initial sync.");
        }
    }
    Ok(())
}

// =========================================================================
// Fleet
// =========================================================================

async fn cmd_fleet(config: &Config, cmd: cli::FleetCommand) -> Result<()> {
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

/// Resolve a peer name or path to a filesystem path.
///
/// If the peer looks like an absolute path, return it as-is.
/// Otherwise treat it as an agent name and derive the path from config.
fn resolve_peer_path(config: &Config, peer: &str) -> std::path::PathBuf {
    let p = std::path::Path::new(peer);
    if p.is_absolute() || peer.ends_with(".db") {
        p.to_path_buf()
    } else {
        config.agent_db_path(peer)
    }
}

/// Open a peer DB file, register extensions, and ensure sync tables are initialized.
fn open_peer_db(db_path: &std::path::Path, tables: &[&str]) -> Result<rusqlite::Connection> {
    let conn = rusqlite::Connection::open(db_path)?;
    mp_ext::init_all_extensions(&conn)?;
    mp_core::sync::init_sync_tables(&conn, tables)?;
    Ok(conn)
}

// =========================================================================
// Db
// =========================================================================

async fn cmd_mpq(config: &Config, expression: &str, agent: Option<String>, dry_run: bool) -> Result<()> {
    let ag = resolve_agent(config, agent.as_deref())?;
    let conn = open_agent_db(config, &ag.name)?;

    let ctx = mp_core::dsl::ExecuteContext {
        agent_id: ag.name.clone(),
        channel: Some("cli".into()),
        session_id: None,
        trace_id: None,
    };

    let response = mp_core::dsl::run(&conn, expression, dry_run, &ctx);
    let output = serde_json::to_string_pretty(&serde_json::json!({
        "ok": response.ok,
        "code": response.code,
        "message": response.message,
        "data": response.data,
    }))?;
    println!("{output}");

    if !response.ok {
        std::process::exit(1);
    }
    Ok(())
}

async fn cmd_db(config: &Config, cmd: cli::DbCommand) -> Result<()> {
    let ag = resolve_agent(config, None)?;
    let conn = open_agent_db(config, &ag.name)?;

    match cmd {
        cli::DbCommand::Query { sql, .. } => {
            let mut stmt = conn.prepare(&sql)?;
            let col_count = stmt.column_count();
            let col_names: Vec<String> = (0..col_count)
                .map(|i| stmt.column_name(i).unwrap_or("?").to_string())
                .collect();

            ui::blank();
            let header_cols: Vec<(&str, usize)> = col_names.iter().map(|n| (n.as_str(), n.len())).collect();
            ui::table_header(&header_cols);

            let mut rows = stmt.query([])?;
            while let Some(row) = rows.next()? {
                let vals: Vec<String> = (0..col_count)
                    .map(|i| row.get::<_, String>(i).unwrap_or_else(|_| "NULL".into()))
                    .collect();
                ui::info(vals.join(" | "));
            }
            ui::blank();
        }
        cli::DbCommand::Schema { .. } => {
            let mut stmt = conn
                .prepare("SELECT name, sql FROM sqlite_master WHERE type='table' ORDER BY name")?;
            let tables: Vec<(String, Option<String>)> = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<Result<Vec<_>, _>>()?;

            ui::blank();
            for (name, sql) in &tables {
                ui::dim(format!("-- {name}"));
                if let Some(s) = sql {
                    for line in s.lines() {
                        ui::info(line);
                    }
                }
                ui::blank();
            }
        }
    }
    Ok(())
}

async fn cmd_session(config: &Config, cmd: cli::SessionCommand) -> Result<()> {
    match cmd {
        cli::SessionCommand::List { agent, limit } => {
            let ag = resolve_agent(config, agent.as_deref())?;
            let conn = open_agent_db(config, &ag.name)?;
            let limit = limit.max(1).min(200);
            let req = op_request(
                &ag.name,
                "session.list",
                serde_json::json!({
                    "agent_id": ag.name,
                    "limit": limit
                }),
            );
            let resp = mp_core::operations::execute(&conn, &req)?;
            if !resp.ok {
                ui::warn(format!("Session list denied: {}", resp.message));
                return Ok(());
            }
            let rows = resp.data.as_array().cloned().unwrap_or_default();

            if rows.is_empty() {
                ui::info(format!("No sessions found for agent '{}'.", ag.name));
                return Ok(());
            }

            let fmt_ts = |ts: i64| -> String {
                chrono::DateTime::<chrono::Utc>::from_timestamp(ts, 0)
                    .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                    .unwrap_or_else(|| ts.to_string())
            };

            ui::blank();
            ui::info(format!("Recent sessions for agent '{}':", ag.name));
            ui::blank();
            for row in rows {
                let id = row["id"].as_str().unwrap_or("-");
                let channel = row["channel"].as_str().unwrap_or("unknown");
                let started_at = row["started_at"].as_i64().unwrap_or(0);
                let ended_at = row["ended_at"].as_i64();
                let message_count = row["message_count"].as_i64().unwrap_or(0);
                let last_activity = row["last_activity"].as_i64().unwrap_or(started_at);
                ui::info(format!("Session: {}", id));
                ui::hint(format!("Channel:       {}", channel));
                ui::hint(format!("Started:       {}", fmt_ts(started_at)));
                ui::hint(format!("Last activity: {}", fmt_ts(last_activity)));
                ui::hint(format!("Messages:      {}", message_count));
                ui::hint(format!(
                    "Ended:         {}",
                    ended_at.map(fmt_ts).unwrap_or_else(|| "active".into())
                ));
                ui::blank();
            }
        }
    }
    Ok(())
}

// =========================================================================
// Health
// =========================================================================

async fn cmd_health(config: &Config) -> Result<()> {
    ui::banner();

    let meta_path = config.metadata_db_path();
    if meta_path.exists() {
        ui::success(format!(
            "Gateway: data dir exists at {}",
            config.data_dir.display()
        ));
    } else {
        ui::warn("Gateway: not initialized (run `mp init`)");
    }

    for agent in &config.agents {
        let db_path = config.agent_db_path(&agent.name);
        if db_path.exists() {
            let conn = mp_core::db::open(&db_path)?;
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
            let embed_queue = mp_core::store::embedding::queue_stats(&conn).unwrap_or_default();

            let metadata = std::fs::metadata(&db_path)?;
            let size_kb = metadata.len() / 1024;
            ui::info(format!(
                "Agent \"{}\": {size_kb} KB, {fact_count} facts, {session_count} sessions, embedding jobs total={} (pending={}, retry={}, processing={}, dead={})",
                agent.name,
                embed_queue.total,
                embed_queue.pending,
                embed_queue.retry,
                embed_queue.processing,
                embed_queue.dead,
            ));
        } else {
            ui::warn(format!("Agent \"{}\": not initialized", agent.name));
        }
    }

    ui::blank();
    Ok(())
}

async fn cmd_doctor(config: &Config, config_path: &Path) -> Result<()> {
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

    // Built-in verify: run a quick SEARCH against the first healthy agent.
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

// toml_to_json, csv_escape, sql_quote — see helpers module
