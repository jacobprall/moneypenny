mod adapters;
mod cli;
mod domain_tools;
mod ui;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Command};
use mp_core::config::Config;
use mp_llm::provider::{EmbeddingProvider, LlmProvider};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, LazyLock, Mutex};
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
        Command::Worker { agent } => cmd_worker(&config, &agent).await,
        Command::Sidecar { agent } => cmd_sidecar(&config, agent).await,
        Command::Setup(cmd) => cmd_setup(&config, config_path, cmd).await,
        Command::Hook { event, agent } => cmd_hook(&config, &event, agent).await,
    }
}

fn init_logging(level: &str) {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level));

    fmt().with_env_filter(filter).with_target(true).init();
}

fn resolve_agent<'a>(
    config: &'a Config,
    name: Option<&str>,
) -> Result<&'a mp_core::config::AgentConfig> {
    match name {
        Some(n) => config
            .agents
            .iter()
            .find(|a| a.name == n)
            .ok_or_else(|| {
                let available: Vec<&str> = config.agents.iter().map(|a| a.name.as_str()).collect();
                if available.is_empty() {
                    anyhow::anyhow!(
                        "Agent '{n}' not found — no agents configured.\n\
                         Fix: run `mp init` to create a default configuration."
                    )
                } else {
                    anyhow::anyhow!(
                        "Agent '{n}' not found. Available agents: {}\n\
                         Fix: use one of the names above, or add '{n}' to moneypenny.toml.",
                        available.join(", ")
                    )
                }
            }),
        None => config
            .agents
            .first()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "No agents configured in moneypenny.toml.\n\
                     Fix: run `mp init` to create a default configuration with a starter agent."
                )
            }),
    }
}

fn open_agent_db(config: &Config, agent_name: &str) -> Result<rusqlite::Connection> {
    let db_path = config.agent_db_path(agent_name);
    let conn = mp_core::db::open(&db_path).map_err(|e| {
        if !db_path.exists() {
            anyhow::anyhow!(
                "Agent database not found at {}\n\
                 Fix: run `mp init` to initialize the project and create agent databases.",
                db_path.display()
            )
        } else {
            anyhow::anyhow!(
                "Failed to open agent database at {}: {e}\n\
                 Fix: run `mp doctor` to diagnose the issue.",
                db_path.display()
            )
        }
    })?;
    mp_core::schema::init_agent_db(&conn)?;
    mp_ext::init_all_extensions(&conn)?;
    if let Some(agent) = config.agents.iter().find(|a| a.name == agent_name) {
        // Register vector indexes so vector_quantize_scan is usable immediately.
        let _ = mp_core::schema::init_vector_indexes(&conn, agent.embedding.dimensions);
        // Idempotently enable CRDT sync tracking on the default sync tables.
        if let Err(e) = mp_core::schema::init_sync_tables(&conn) {
            tracing::warn!(agent = agent_name, "sync table init warning: {e}");
        }
        // Discover and register MCP tools from configured servers.
        // Runs synchronously at open time; unreachable servers are skipped with a warning.
        if !agent.mcp_servers.is_empty() {
            match mp_core::mcp::discover_and_register(&conn, &agent.mcp_servers) {
                Ok(n) if n > 0 => {
                    tracing::info!(agent = agent_name, tools = n, "MCP tools registered")
                }
                Ok(_) => {}
                Err(e) => tracing::warn!(agent = agent_name, "MCP discovery error: {e}"),
            }
        }
    }
    Ok(conn)
}

fn build_provider(agent: &mp_core::config::AgentConfig) -> Result<Box<dyn LlmProvider>> {
    mp_llm::build_provider(
        &agent.llm.provider,
        agent.llm.api_base.as_deref(),
        agent.llm.api_key.as_deref(),
        agent.llm.model.as_deref(),
    )
    .map_err(|e| {
        let provider = &agent.llm.provider;
        let hint = match provider.as_str() {
            "anthropic" => "Fix: set ANTHROPIC_API_KEY in your environment or .env file.",
            "openai" => "Fix: set OPENAI_API_KEY in your environment or .env file.",
            _ => "Fix: check the LLM provider configuration in moneypenny.toml.",
        };
        anyhow::anyhow!("LLM provider '{provider}' failed to initialize: {e}\n{hint}")
    })
}

fn build_embedding_provider(
    config: &Config,
    agent: &mp_core::config::AgentConfig,
) -> Result<Box<dyn mp_llm::provider::EmbeddingProvider>> {
    let model_path = agent.embedding.resolve_model_path(&config.models_dir());
    mp_llm::build_embedding_provider(
        &agent.embedding.provider,
        &agent.embedding.model,
        &model_path,
        agent.embedding.dimensions,
        agent.embedding.api_base.as_deref(),
        agent.embedding.api_key.as_deref(),
    )
}

fn build_embedding_provider_with_override(
    config: &Config,
    agent: &mp_core::config::AgentConfig,
    model_override: Option<&str>,
) -> Result<Box<dyn mp_llm::provider::EmbeddingProvider>> {
    let mut embed_cfg = agent.embedding.clone();
    if let Some(model) = model_override {
        embed_cfg.model = model.to_string();
        // If config pins a path for a different model, fall back to the derived path.
        embed_cfg.model_path = None;
    }
    let model_path = embed_cfg.resolve_model_path(&config.models_dir());
    mp_llm::build_embedding_provider(
        &embed_cfg.provider,
        &embed_cfg.model,
        &model_path,
        embed_cfg.dimensions,
        embed_cfg.api_base.as_deref(),
        embed_cfg.api_key.as_deref(),
    )
}

fn embedding_model_id(agent: &mp_core::config::AgentConfig) -> String {
    mp_core::store::embedding::model_identity(
        &agent.embedding.provider,
        &agent.embedding.model,
        agent.embedding.dimensions,
    )
}

fn build_llm_tools() -> Vec<mp_llm::types::ToolDef> {
    vec![
        mp_llm::types::ToolDef {
            name: "web_search".into(),
            description: "Search the public web for up-to-date information and return result snippets with URLs.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search query" },
                    "limit": { "type": "integer", "description": "Maximum number of results", "default": 5 }
                },
                "required": ["query"]
            }),
        },
        mp_llm::types::ToolDef {
            name: "memory_search".into(),
            description: "Search the agent's memory across facts, conversation history, and knowledge base.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Natural language search query" },
                    "limit": { "type": "integer", "description": "Max results", "default": 10 }
                },
                "required": ["query"]
            }),
        },
        mp_llm::types::ToolDef {
            name: "fact_add".into(),
            description: "Store a new fact in long-term memory. Use when the user tells you something important to remember.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "content": { "type": "string", "description": "The full fact text" },
                    "summary": { "type": "string", "description": "A shorter version for quick scanning" },
                    "pointer": { "type": "string", "description": "A one-line label (2-5 words)" },
                    "keywords": { "type": "string", "description": "Space-separated keywords for search" },
                    "confidence": { "type": "number", "description": "Confidence 0.0-1.0", "default": 1.0 }
                },
                "required": ["content"]
            }),
        },
        mp_llm::types::ToolDef {
            name: "fact_update".into(),
            description: "Update an existing fact when information has changed or been refined.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "The fact ID" },
                    "content": { "type": "string", "description": "Updated fact text" },
                    "summary": { "type": "string", "description": "Updated short summary" },
                    "pointer": { "type": "string", "description": "Updated one-line label" }
                },
                "required": ["id", "content"]
            }),
        },
        mp_llm::types::ToolDef {
            name: "fact_list".into(),
            description: "List all active facts in memory for review or audit.".into(),
            parameters: serde_json::json!({ "type": "object", "properties": {} }),
        },
        mp_llm::types::ToolDef {
            name: "scratch_set".into(),
            description: "Save a value to session working memory for intermediate results and plans. Ephemeral — only lasts this session.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "key": { "type": "string", "description": "Short label (e.g. 'plan', 'findings')" },
                    "content": { "type": "string", "description": "The value to store" }
                },
                "required": ["key", "content"]
            }),
        },
        mp_llm::types::ToolDef {
            name: "scratch_get".into(),
            description: "Retrieve a value from session working memory.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "key": { "type": "string", "description": "The key to look up" }
                },
                "required": ["key"]
            }),
        },
        mp_llm::types::ToolDef {
            name: "knowledge_ingest".into(),
            description: "Ingest a document into the knowledge base. Automatically chunks and indexes for search.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "content": { "type": "string", "description": "Full document text (markdown supported)" },
                    "title": { "type": "string", "description": "Document title" },
                    "path": { "type": "string", "description": "Source file path" }
                },
                "required": ["content"]
            }),
        },
        mp_llm::types::ToolDef {
            name: "knowledge_list".into(),
            description: "List all documents in the knowledge base.".into(),
            parameters: serde_json::json!({ "type": "object", "properties": {} }),
        },
        mp_llm::types::ToolDef {
            name: "job_create".into(),
            description: "Schedule a recurring task with a cron expression.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Human-readable job name" },
                    "schedule": { "type": "string", "description": "Cron expression (e.g. '0 9 * * *' for daily at 9am)" },
                    "job_type": { "type": "string", "enum": ["prompt", "tool", "js", "pipeline"], "default": "prompt" },
                    "payload": { "type": "string", "description": "JSON payload", "default": "{}" },
                    "description": { "type": "string", "description": "What this job does" }
                },
                "required": ["name", "schedule"]
            }),
        },
        mp_llm::types::ToolDef {
            name: "job_list".into(),
            description: "List all scheduled jobs and their status.".into(),
            parameters: serde_json::json!({ "type": "object", "properties": {} }),
        },
        mp_llm::types::ToolDef {
            name: "file_read".into(),
            description: "Read contents of a file from the filesystem.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "File path to read" }
                },
                "required": ["path"]
            }),
        },
        mp_llm::types::ToolDef {
            name: "shell_exec".into(),
            description: "Execute a shell command and return output.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string", "description": "Shell command to execute" },
                    "timeout_ms": { "type": "integer", "description": "Timeout in milliseconds", "default": 30000 }
                },
                "required": ["command"]
            }),
        },
        mp_llm::types::ToolDef {
            name: "delegate_to_agent".into(),
            description: "Delegate a task or question to another agent. The target agent will \
                           handle the request and return a response. Use when a specialized agent \
                           should process part of the work.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "to": {
                        "type": "string",
                        "description": "Name of the target agent to delegate to"
                    },
                    "message": {
                        "type": "string",
                        "description": "The task, question, or instruction for the target agent"
                    }
                },
                "required": ["to", "message"]
            }),
        },
    ]
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|n| haystack.contains(n))
}

/// Prefer text-only responses for explain/plan questions that don't request actions.
fn is_text_first_intent(user_message: &str) -> bool {
    let s = user_message.to_lowercase();
    let asks_explain_or_plan = contains_any(
        &s,
        &[
            "explain",
            "why ",
            "what happened",
            "how does",
            "how do",
            "walk me through",
            "step by step",
            "plan",
            "summarize",
            "summary",
            "what should i do",
            "can you think of",
            "think of a good task",
            "suggest",
            "idea",
            "recommended task",
            "what would be a good task",
        ],
    );
    let asks_action = contains_any(
        &s,
        &[
            "create ",
            "add ",
            "update ",
            "delete ",
            "remove ",
            "ingest ",
            "schedule ",
            "run ",
            "execute ",
            "use tool",
            "call tool",
            "save ",
            "remember ",
            "set ",
        ],
    );
    asks_explain_or_plan && !asks_action
}

/// "Write confirmation" is treated as explicit user intent to perform mutations.
fn has_write_confirmation(user_message: &str) -> bool {
    let s = user_message.to_lowercase();
    contains_any(
        &s,
        &[
            "confirm",
            "approved",
            "go ahead",
            "yes do it",
            "please do it",
            "create ",
            "add ",
            "update ",
            "delete ",
            "remove ",
            "ingest ",
            "schedule ",
            "save ",
            "remember ",
            "set ",
            "run ",
            "execute ",
        ],
    )
}

fn allow_multi_tool_calls(user_message: &str) -> bool {
    let s = user_message.to_lowercase();
    contains_any(
        &s,
        &[
            "use multiple tools",
            "use many tools",
            "run all tools",
            "show off all features",
            "full workflow",
        ],
    )
}

fn is_mutating_tool(name: &str) -> bool {
    matches!(
        name,
        "fact_add"
            | "fact_update"
            | "scratch_set"
            | "knowledge_ingest"
            | "job_create"
            | "job_pause"
            | "job_resume"
            | "job_run"
            | "js_tool_add"
            | "js_tool_delete"
            | "shell_exec"
            | "delegate_to_agent"
    ) || name.starts_with("mcp:")
}

fn is_read_only_tool(name: &str) -> bool {
    matches!(
        name,
        "web_search"
            | "memory_search"
            | "fact_list"
            | "scratch_get"
            | "knowledge_list"
            | "job_list"
            | "file_read"
            | "policy_list"
            | "audit_query"
    )
}

/// Load user-defined JS tools from the skills table as LLM ToolDefs.
fn load_js_tool_defs(conn: &rusqlite::Connection) -> Vec<(String, String, serde_json::Value)> {
    let mut stmt = match conn
        .prepare("SELECT name, description FROM skills WHERE tool_id LIKE 'sqlite_js:%'")
    {
        Ok(s) => s,
        Err(_) => return vec![],
    };
    stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .ok()
        .map(|rows| {
            rows.flatten()
                .map(|(name, desc)| {
                    let schema = serde_json::json!({
                        "type": "object",
                        "properties": {
                            "args": {
                                "type": "object",
                                "description": "Arguments passed to the run(args) function"
                            }
                        }
                    });
                    (name, desc, schema)
                })
                .collect()
        })
        .unwrap_or_default()
}

async fn agent_turn(
    conn: &rusqlite::Connection,
    provider: &dyn LlmProvider,
    embed_provider: Option<&dyn EmbeddingProvider>,
    agent_id: &str,
    session_id: &str,
    persona: Option<&str>,
    user_message: &str,
    policy_mode: mp_core::policy::PolicyMode,
    worker_bus: Option<&std::sync::Arc<WorkerBus>>,
) -> Result<String> {
    mp_core::store::log::append_message(conn, session_id, "user", user_message)?;

    let budget = mp_core::context::TokenBudget::new(128_000);
    let segments = mp_core::context::assemble(
        conn,
        agent_id,
        session_id,
        persona,
        user_message,
        &budget,
        None,
    )?;

    let mut messages: Vec<mp_llm::types::Message> = Vec::new();
    for seg in &segments {
        match seg.label {
            "current_message" => messages.push(mp_llm::types::Message::user(&seg.content)),
            _ => messages.push(mp_llm::types::Message::system(&seg.content)),
        }
    }

    let msg_policy = mp_core::policy::PolicyRequest {
        actor: agent_id,
        action: "respond",
        resource: mp_core::policy::resource::CONVERSATION,
        sql_content: Some(user_message),
        channel: None,
        arguments: None,
    };
    let decision = mp_core::policy::evaluate_with_mode(conn, &msg_policy, policy_mode)?;
    if matches!(decision.effect, mp_core::policy::Effect::Deny) {
        let denial = format!(
            "I'm unable to respond to that: {}",
            decision.reason.as_deref().unwrap_or("blocked by policy")
        );
        mp_core::store::log::append_message(conn, session_id, "assistant", &denial)?;
        return Ok(denial);
    }

    let text_first = is_text_first_intent(user_message);
    let write_confirmed = has_write_confirmation(user_message);
    let multi_tool_opt_in = allow_multi_tool_calls(user_message);

    let mut tools = build_llm_tools();
    // Append dynamically-discovered MCP tools so the LLM can call them.
    for (name, desc, schema) in mp_core::mcp::load_tool_defs(conn) {
        tools.push(mp_llm::types::ToolDef {
            name,
            description: desc,
            parameters: schema,
        });
    }
    // Append user-defined JS tools.
    for (js_name, js_desc, js_schema) in load_js_tool_defs(conn) {
        tools.push(mp_llm::types::ToolDef {
            name: js_name,
            description: js_desc,
            parameters: js_schema,
        });
    }

    if text_first {
        // For explanation/planning requests, force a direct response.
        tools.clear();
        messages.push(mp_llm::types::Message::system(
            "This request is explanatory/planning. Do NOT call tools. Respond directly.",
        ));
    } else if !write_confirmed {
        // Default-safe mode: expose only read-only tools unless user clearly confirms writes.
        tools.retain(|t| is_read_only_tool(&t.name));
        messages.push(mp_llm::types::Message::system(
            "Use read-only tools only unless the user explicitly confirms write actions.",
        ));
    }

    let allowed_tool_names: HashSet<String> = tools.iter().map(|t| t.name.clone()).collect();

    let config = mp_llm::types::GenerateConfig::default();
    let max_rounds = 10;
    let max_tool_calls_total = if multi_tool_opt_in { 8 } else { 2 };
    let mut total_tool_calls = 0usize;
    let mut consecutive_tool_failures = 0usize;
    let mut last_tool_name: Option<String> = None;
    let mut same_tool_streak = 0usize;
    let mut loop_broken = false;

    for _ in 0..max_rounds {
        let response = provider.generate(&messages, &tools, &config).await?;

        if response.tool_calls.is_empty() {
            let text = response.content.unwrap_or_default();
            let redacted = mp_core::store::redact::redact(&text);
            mp_core::store::log::append_message(conn, session_id, "assistant", &redacted)?;
            return Ok(redacted);
        }

        let mut planned_calls = response.tool_calls;
        if total_tool_calls >= max_tool_calls_total {
            loop_broken = true;
            break;
        }
        let remaining = max_tool_calls_total.saturating_sub(total_tool_calls);
        if planned_calls.len() > remaining {
            planned_calls.truncate(remaining);
        }
        if !multi_tool_opt_in && planned_calls.len() > 1 {
            planned_calls.truncate(1);
        }
        if planned_calls.is_empty() {
            loop_broken = true;
            break;
        }

        messages.push(mp_llm::types::Message::assistant_with_tool_calls(
            response.content.clone(),
            planned_calls
                .iter()
                .map(|tc| mp_llm::types::ToolCall {
                    id: tc.id.clone(),
                    name: tc.name.clone(),
                    arguments: tc.arguments.clone(),
                })
                .collect(),
        ));

        for tc in planned_calls {
            total_tool_calls += 1;

            if !allowed_tool_names.contains(&tc.name) {
                consecutive_tool_failures += 1;
                let blocked = format!(
                    "Tool '{}' is not available for this request. Ask explicitly to use it.",
                    tc.name
                );
                messages.push(mp_llm::types::Message::tool(&blocked, &tc.id));
                if consecutive_tool_failures >= 3 {
                    loop_broken = true;
                    break;
                }
                continue;
            }

            if is_mutating_tool(&tc.name) && !write_confirmed {
                consecutive_tool_failures += 1;
                let blocked = format!(
                    "Blocked mutating tool '{}'. Please ask explicitly and confirm the write action.",
                    tc.name
                );
                messages.push(mp_llm::types::Message::tool(&blocked, &tc.id));
                if consecutive_tool_failures >= 3 {
                    loop_broken = true;
                    break;
                }
                continue;
            }

            let msg_id = mp_core::store::log::append_message(
                conn,
                session_id,
                "assistant",
                &format!("[tool: {}]", tc.name),
            )?;
            let mut effective_arguments = tc.arguments.clone();
            if tc.name == "memory_search" {
                effective_arguments =
                    enrich_memory_search_args_with_embedding(&effective_arguments, embed_provider)
                        .await;
            }

            // Delegation tool is handled at the gateway layer (not via the registry).
            if tc.name == "delegate_to_agent" {
                let args: serde_json::Value =
                    serde_json::from_str(&tc.arguments).unwrap_or_default();
                let target = args["to"].as_str().unwrap_or("");
                let msg = args["message"].as_str().unwrap_or("");

                let delegation_result = if let Some(bus) = worker_bus {
                    // Gateway mode: route through the WorkerBus
                    match bus.route(target, msg, None).await {
                        Ok(r) => r,
                        Err(e) => format!("Delegation to '{target}' failed: {e}"),
                    }
                } else {
                    // Standalone mode: delegation not available without a running gateway
                    format!(
                        "Delegation to '{target}' is only available in gateway mode (mp start)."
                    )
                };

                tracing::info!(target, "delegation tool call");
                messages.push(mp_llm::types::Message::tool(&delegation_result, &tc.id));
                continue;
            }

            let result = mp_core::tools::registry::execute(
                conn,
                agent_id,
                session_id,
                &msg_id,
                &tc.name,
                &effective_arguments,
                &|name, args| mp_core::tools::builtins::dispatch(name, args),
                None,
            )?;

            tracing::info!(tool = %tc.name, success = result.success, "tool call");
            if result.success {
                consecutive_tool_failures = 0;
            } else {
                consecutive_tool_failures += 1;
            }

            if last_tool_name.as_deref() == Some(tc.name.as_str()) {
                same_tool_streak += 1;
            } else {
                same_tool_streak = 1;
                last_tool_name = Some(tc.name.clone());
            }

            messages.push(mp_llm::types::Message::tool(&result.output, &tc.id));

            if consecutive_tool_failures >= 3 || same_tool_streak >= 4 {
                loop_broken = true;
                break;
            }
            if total_tool_calls >= max_tool_calls_total {
                loop_broken = true;
                break;
            }
        }

        if loop_broken {
            break;
        }
    }

    if loop_broken {
        // Final best-effort natural-language answer with tools disabled so the
        // user still gets a useful response without needing to rephrase.
        let mut final_messages = messages.clone();
        final_messages.push(mp_llm::types::Message::system(
            "Tool execution was halted. Respond directly in plain language with \
the best possible answer. If a write action is required, clearly mention it \
and ask for explicit confirmation.",
        ));
        if let Ok(final_resp) = provider.generate(&final_messages, &[], &config).await {
            let text = final_resp.content.unwrap_or_default();
            let redacted = mp_core::store::redact::redact(&text);
            mp_core::store::log::append_message(conn, session_id, "assistant", &redacted)?;
            return Ok(redacted);
        }
    }

    let fallback = "I was unable to complete the response after multiple tool call rounds.";
    mp_core::store::log::append_message(conn, session_id, "assistant", fallback)?;
    Ok(fallback.into())
}

async fn enrich_memory_search_args_with_embedding(
    arguments: &str,
    embed_provider: Option<&dyn EmbeddingProvider>,
) -> String {
    let Some(embedder) = embed_provider else {
        return arguments.to_string();
    };

    let mut parsed: serde_json::Value = match serde_json::from_str(arguments) {
        Ok(v) => v,
        Err(_) => return arguments.to_string(),
    };

    let Some(query) = parsed
        .get("query")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|q| !q.is_empty())
    else {
        return arguments.to_string();
    };

    match embedder.embed(query).await {
        Ok(vec) => {
            let embedding = vec
                .into_iter()
                .map(|v| serde_json::Value::from(v as f64))
                .collect::<Vec<_>>();
            parsed["__query_embedding"] = serde_json::Value::Array(embedding);
            serde_json::to_string(&parsed).unwrap_or_else(|_| arguments.to_string())
        }
        Err(e) => {
            tracing::debug!("memory_search embedding generation failed: {e}");
            arguments.to_string()
        }
    }
}

// =========================================================================
// Extraction — the agent's learning brain
// =========================================================================

const EXTRACTION_PROMPT: &str = "\
You are a fact extraction system for an AI agent's long-term memory. \
Analyze the conversation below and extract durable facts worth remembering across sessions.

Rules:
- Extract ONLY facts that are worth remembering in future conversations.
- Each fact must be a self-contained statement — not a sentence fragment.
- Include actionable details (column names, exact values, specific conventions).
- Do NOT extract greetings, pleasantries, or meta-conversation.
- Do NOT extract facts that are already in the existing facts list.
- If nothing is worth extracting, output an empty JSON array: []

Output a JSON array (no markdown fences, no explanation) where each element has:
  {\"content\": \"full fact text\", \"summary\": \"shorter version\", \
\"pointer\": \"2-5 word label\", \"keywords\": \"space separated terms\", \
\"confidence\": 0.0 to 1.0}";

async fn extract_facts(
    conn: &rusqlite::Connection,
    provider: &dyn LlmProvider,
    agent_id: &str,
    session_id: &str,
) -> Result<usize> {
    let recent = mp_core::store::log::get_recent_messages(conn, session_id, 6)?;
    if recent.is_empty() {
        return Ok(0);
    }

    let new_messages: Vec<String> = recent
        .iter()
        .map(|m| format!("{}: {}", m.role, m.content))
        .collect();

    let extraction_ctx = mp_core::extraction::assemble_extraction_context(
        conn,
        agent_id,
        session_id,
        &new_messages,
        30,
    )?;

    let messages = vec![
        mp_llm::types::Message::system(EXTRACTION_PROMPT),
        mp_llm::types::Message::user(&extraction_ctx),
    ];

    let config = mp_llm::types::GenerateConfig {
        temperature: Some(0.2),
        max_tokens: Some(2000),
        stop: Vec::new(),
    };

    let response = provider.generate(&messages, &[], &config).await?;
    let text = response.content.unwrap_or_default();

    let json_text = text
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    let candidates = match mp_core::extraction::parse_candidates(json_text) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("extraction parse failed: {e}");
            return Ok(0);
        }
    };

    if candidates.is_empty() {
        return Ok(0);
    }

    let last_msg_id = recent.last().map(|m| m.id.as_str());
    let outcomes =
        mp_core::extraction::run_pipeline(conn, agent_id, session_id, &candidates, last_msg_id)?;

    let extracted = outcomes.iter().filter(|o| o.policy_allowed).count();
    if extracted > 0 {
        tracing::info!(count = extracted, "facts extracted");
    }
    Ok(extracted)
}

/// Process the durable embedding job queue:
/// - enqueue drifted or missing rows for this embedding model,
/// - claim due jobs with retries/leases,
/// - compute FLOAT32 embeddings and persist provenance metadata,
/// - refresh vector quantization indexes for touched targets.
async fn embed_pending(
    conn: &rusqlite::Connection,
    embed: &dyn mp_llm::provider::EmbeddingProvider,
    agent_id: &str,
    embedding_model_id: &str,
) {
    let stats = match mp_core::store::embedding::process_embedding_jobs(
        conn,
        agent_id,
        embedding_model_id,
        128,
        5,
        8,
        |content| async move {
            let vec = embed.embed(&content).await?;
            Ok::<Vec<u8>, anyhow::Error>(mp_llm::f32_slice_to_blob(&vec))
        },
    )
    .await
    {
        Ok(stats) => stats,
        Err(e) => {
            tracing::warn!("embed_pending: pending query failed: {e}");
            return;
        }
    };

    if stats.failed > 0 {
        tracing::warn!(failed = stats.failed, "some embeddings failed");
    }
    if stats.embedded > 0 || stats.queued > 0 || stats.claimed > 0 {
        tracing::debug!(
            queued = stats.queued,
            claimed = stats.claimed,
            embedded = stats.embedded,
            failed = stats.failed,
            skipped = stats.skipped,
            "embedding queue processed"
        );
    }
}

// =========================================================================
// Rolling summarization
// =========================================================================

/// Summarize after this many messages (10 exchange pairs).
const SUMMARIZE_EVERY: usize = 20;
/// Keep this many recent messages as "live" context outside the summary.
const RECENT_KEEP: usize = 10;

const SUMMARIZE_PROMPT: &str = "\
You are a conversation summarization assistant for an AI agent's long-term memory. \
Given a conversation history (and an optional prior rolling summary), produce a concise \
rolling summary that captures:
- Key facts and topics discussed
- Decisions or conclusions reached
- Important context that would help in future turns

Write in neutral third-person prose. Keep under 200 words. \
If given a prior summary, extend it — do not repeat what is already there.";

/// If the session has accumulated enough messages, summarize the older portion
/// and store the result in `sessions.summary` for use in context assembly.
/// Runs asynchronously after each turn; failures are silently logged.
async fn maybe_summarize_session(
    conn: &rusqlite::Connection,
    provider: &dyn LlmProvider,
    session_id: &str,
) {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM messages WHERE session_id = ?1",
            rusqlite::params![session_id],
            |r| r.get(0),
        )
        .unwrap_or(0);

    // Only run on exact multiples so we don't re-summarize the same window
    if count < SUMMARIZE_EVERY as i64 || count % SUMMARIZE_EVERY as i64 != 0 {
        return;
    }

    let all = match mp_core::store::log::get_messages(conn, session_id) {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!("summarize: failed to load messages: {e}");
            return;
        }
    };

    let keep = RECENT_KEEP.min(all.len());
    let to_summarize = &all[..all.len().saturating_sub(keep)];
    if to_summarize.is_empty() {
        return;
    }

    let existing: Option<String> = conn
        .query_row(
            "SELECT summary FROM sessions WHERE id = ?1",
            rusqlite::params![session_id],
            |r| r.get(0),
        )
        .unwrap_or(None);

    let conv_text = to_summarize
        .iter()
        .map(|m| format!("{}: {}", m.role, m.content))
        .collect::<Vec<_>>()
        .join("\n");

    let user_prompt = match &existing {
        Some(prev) if !prev.trim().is_empty() => {
            format!("Prior summary:\n{prev}\n\nNew conversation to incorporate:\n{conv_text}")
        }
        _ => format!("Conversation:\n{conv_text}"),
    };

    let messages = vec![
        mp_llm::types::Message::system(SUMMARIZE_PROMPT),
        mp_llm::types::Message::user(&user_prompt),
    ];
    let cfg = mp_llm::types::GenerateConfig {
        temperature: Some(0.2),
        max_tokens: Some(600),
        stop: Vec::new(),
    };

    match provider.generate(&messages, &[], &cfg).await {
        Ok(resp) => {
            if let Some(summary) = resp.content {
                if !summary.trim().is_empty() {
                    if let Err(e) = mp_core::store::log::update_summary(conn, session_id, &summary)
                    {
                        tracing::warn!("summarize: failed to save summary: {e}");
                    } else {
                        tracing::debug!(session_id, "rolling session summary updated");
                    }
                }
            }
        }
        Err(e) => tracing::warn!("summarize: LLM call failed: {e}"),
    }
}

// =========================================================================
// Model download
// =========================================================================

fn default_model_url(model_name: &str) -> Option<&'static str> {
    match model_name {
        "nomic-embed-text-v1.5" => Some(
            "https://huggingface.co/nomic-ai/nomic-embed-text-v1.5-GGUF/resolve/main/nomic-embed-text-v1.5.Q4_K_M.gguf",
        ),
        _ => None,
    }
}

async fn download_model(url: &str, dest: &Path) -> Result<()> {
    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;

    if dest.exists() {
        ui::success(format!("Model already present at {}", dest.display()));
        ui::flush();
        return Ok(());
    }

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let fname = dest
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or("model");

    ui::info(format!("↓ Connecting to download {fname}..."));
    ui::flush();

    let resp = reqwest::get(url).await?;
    if !resp.status().is_success() {
        anyhow::bail!("download failed: HTTP {}", resp.status());
    }

    let total = resp.content_length();
    let size_label = match total {
        Some(n) => format!("{} MB", n / 1_000_000),
        None => "unknown size".to_string(),
    };

    ui::info(format!("↓ Downloading {fname} ({size_label})..."));
    ui::detail("This may take a few minutes on slower connections.");
    ui::flush();

    let tmp = dest.with_extension("gguf.tmp");
    let mut file = tokio::fs::File::create(&tmp).await?;
    let mut stream = resp.bytes_stream();
    let mut downloaded: u64 = 0;
    let mut last_mb: u64 = 0;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;
        let current_mb = downloaded / 1_000_000;
        if current_mb >= last_mb + 25 {
            last_mb = current_mb;
            if let Some(t) = total {
                let pct = (downloaded * 100) / t;
                ui::detail(format!("{current_mb} / {} MB ({pct}%)", t / 1_000_000));
            } else {
                ui::detail(format!("{current_mb} MB downloaded..."));
            }
            ui::flush();
        }
    }
    file.flush().await?;
    drop(file);

    tokio::fs::rename(&tmp, dest).await?;
    ui::success(format!("Model saved to {}", dest.display()));
    ui::flush();
    Ok(())
}

/// Download embedding models for all agents with `provider = "local"`.
async fn ensure_embedding_models(config: &Config) {
    for agent in &config.agents {
        if agent.embedding.provider != "local" {
            continue;
        }
        let model_path = agent.embedding.resolve_model_path(&config.models_dir());
        let url = match default_model_url(&agent.embedding.model) {
            Some(u) => u,
            None => {
                ui::warn(format!(
                    "No download URL known for model \"{}\". Place the GGUF file at {:?} manually.",
                    agent.embedding.model, model_path
                ));
                continue;
            }
        };
        if let Err(e) = download_model(url, &model_path).await {
            ui::error(format!(
                "Failed to download model for agent \"{}\": {e}",
                agent.name
            ));
        }
    }
}

// =========================================================================
// Bootstrap seed facts
// =========================================================================

fn seed_bootstrap_facts(conn: &rusqlite::Connection, agent_id: &str) {
    use mp_core::store::facts::{NewFact, add};

    let seeds: &[(&str, &str, &str, &str)] = &[
        (
            // Pointer (always in context, ~10 words)
            "Moneypenny: persistent memory, knowledge, policies, tools, extraction",
            // Summary (expanded via FTS when relevant)
            "Moneypenny is an autonomous AI agent runtime where the database is the runtime. \
             It provides persistent long-term memory (facts), knowledge retrieval from ingested \
             documents, governance policies, scheduled jobs, and conversation history across sessions.",
            // Content (full detail, retrieved via memory_search)
            "Moneypenny is an autonomous AI agent platform where the database is the runtime. \
             Core capabilities:\n\
             - Facts: durable knowledge extracted from conversations, stored with confidence scores. \
               Facts are progressively compacted — full content at Level 0, summaries at Level 1, \
               pointers at Level 2. All fact pointers appear in every context window.\n\
             - Knowledge: documents and URLs ingested into a chunk store with FTS5 search.\n\
             - Policies: allow/deny/audit rules governing what the agent can do.\n\
             - Sessions: conversation history with rolling summaries for long conversations.\n\
             - Jobs: cron-scheduled tasks the agent can run autonomously.\n\
             - Scratch: ephemeral per-session working memory for intermediate results.\n\
             Architecture: SQLite-based, local-first, with optional CRDT sync across agents.",
            // Keywords
            "moneypenny memory facts knowledge policies sessions jobs architecture",
        ),
        (
            "Tools: memory_search, fact_list, web_search, file_read, scratch_set/get",
            "Available tools: memory_search (semantic + FTS search across facts, messages, knowledge), \
             fact_list (enumerate stored facts), web_search (live internet search), \
             file_read (read local files), scratch_set/scratch_get (session working memory), \
             knowledge_list (ingested documents), job_list (scheduled jobs), \
             policy_list (active policies), audit_query (audit trail).",
            "The agent has access to these tools:\n\
             - memory_search: search across facts, conversation history, and knowledge. Supports \
               both keyword (FTS5) and semantic (vector) search when embeddings are available.\n\
             - fact_list: list all stored facts with pointers and confidence scores.\n\
             - web_search: search the internet for current information.\n\
             - file_read: read files from the local filesystem.\n\
             - scratch_set / scratch_get: save and retrieve ephemeral values within the current session. \
               Use for intermediate results, plans, and working state.\n\
             - knowledge_list: list ingested documents in the knowledge store.\n\
             - job_list: list scheduled jobs and their status.\n\
             - policy_list: list active governance policies.\n\
             - audit_query: search the audit trail for past actions.\n\
             When uncertain about what you know, use memory_search before answering. \
             When asked to remember something, the extraction pipeline handles it automatically — \
             just acknowledge the request.",
            "tools memory_search fact_list web_search file_read scratch knowledge jobs",
        ),
        (
            "Learning: facts extracted automatically from conversations",
            "The agent learns by extracting durable facts from conversations. An extraction pipeline \
             runs after each turn, identifying statements worth remembering. Facts are deduplicated \
             against existing knowledge and stored with confidence scores.",
            "How the agent learns:\n\
             1. After each conversation turn, an extraction pipeline analyzes recent messages.\n\
             2. Candidate facts are identified — statements that are durable, non-obvious, and worth \
                remembering across sessions.\n\
             3. Candidates are deduplicated against existing facts to avoid redundancy.\n\
             4. New facts are stored with confidence scores (0.0-1.0) and linked to their source message.\n\
             5. Over time, fact pointers are progressively compacted to fit more knowledge into the \
                context window. The full content is always available via memory_search.\n\
             6. Facts can be manually inserted via the MPQ language: \
                INSERT INTO facts (\"content\", topic=\"value\", confidence=0.9)\n\
             The agent does not need to explicitly \"save\" facts — the pipeline handles it. \
             When a user says \"remember this\", just acknowledge it.",
            "learning extraction facts pipeline confidence deduplication compaction",
        ),
        (
            "MPQ: query language for memory operations (SEARCH, INSERT, DELETE)",
            "MPQ (Moneypenny Query) is the agent's query language. Key operations: \
             SEARCH facts/knowledge/audit with WHERE filters, SINCE duration, SORT, TAKE. \
             INSERT INTO facts with content and metadata. DELETE FROM facts with conditions.",
            "MPQ (Moneypenny Query) syntax reference:\n\
             - SEARCH <store> [WHERE <filters>] [SINCE <duration>] [| SORT field ASC|DESC] [| TAKE n]\n\
             - INSERT INTO facts (\"content\", key=value ...)\n\
             - UPDATE facts SET key=value WHERE id = \"id\"\n\
             - DELETE FROM facts WHERE <filters>\n\
             - INGEST \"url\"\n\
             - SEARCH audit WHERE <filters> [| TAKE n]\n\n\
             Stores: facts, knowledge, log, audit\n\
             Filters: field = value, field > value, field LIKE \"%pattern%\", AND\n\
             Durations: 7d, 24h, 30m\n\
             Pipeline: chain stages with |\n\
             Multi-statement: separate with ;\n\n\
             Examples:\n\
             SEARCH facts WHERE topic = \"auth\" SINCE 7d | SORT confidence DESC | TAKE 10\n\
             INSERT INTO facts (\"Redis preferred for caching\", topic=\"infra\", confidence=0.9)\n\
             SEARCH facts | COUNT",
            "mpq query language search insert delete facts knowledge audit",
        ),
    ];

    for (pointer, summary, content, keywords) in seeds {
        let fact = NewFact {
            agent_id: agent_id.to_string(),
            scope: "shared".to_string(),
            content: content.to_string(),
            summary: summary.to_string(),
            pointer: pointer.to_string(),
            keywords: Some(keywords.to_string()),
            source_message_id: None,
            confidence: 1.0,
        };
        if let Err(e) = add(conn, &fact, Some("bootstrap")) {
            tracing::warn!(agent = agent_id, "failed to seed bootstrap fact: {e}");
        }
    }
}

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
    ui::hint("mp setup cursor --local            # register MCP server, hooks, and agent rules");
    ui::blank();
    ui::info("Then reload Cursor and ask: \"What Moneypenny tools do you have?\"");
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
            ui::hint("- MCP tools: moneypenny.facts, moneypenny.knowledge, moneypenny.policy, moneypenny.activity, moneypenny.execute");
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
    ui::hint("- MCP tools: moneypenny.facts, moneypenny.knowledge, moneypenny.policy, moneypenny.activity, moneypenny.execute");
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

fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max { s } else { &s[..max] }
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
| `moneypenny.facts` | CRUD for durable facts — persistent knowledge across sessions. |
| `moneypenny.knowledge` | Ingest and retrieve documents — long-term reference library. |
| `moneypenny.policy` | Governance — control what agents can and cannot do. |
| `moneypenny.activity` | Query session history and audit trail. |
| `moneypenny.execute` | Escape hatch for any canonical operation. |

**Important:** These tools are MCP tools served by the Moneypenny sidecar
process. They must appear in your callable tool list. If they do not, the MCP
server is not connected — tell the user to run `mp setup claude-code` in the
project directory.

### Tool usage

Each domain tool takes an `action` string and an `input` object.

**moneypenny.facts**: search, add, get, update, delete
**moneypenny.knowledge**: ingest, search, list
**moneypenny.policy**: add, list, disable, evaluate
**moneypenny.activity**: query (source: events | decisions | all)
**moneypenny.execute**: op + args (any canonical operation)

### When to use Moneypenny

- **User says "mp ..."**: Always route through Moneypenny
- **Remembering things**: Use `moneypenny.facts` action `add`
- **Recalling context**: Use `moneypenny.facts` action `search`
- **Ingesting documents**: Use `moneypenny.knowledge` action `ingest`
- **Activity trail**: Use `moneypenny.activity` action `query`
- **Governance**: Use `moneypenny.policy` to manage rules

### Best practices

- Search before inserting facts to avoid duplicates
- Use specific keywords when inserting facts
- Set confidence scores to reflect certainty (0.0 to 1.0)
- Use `moneypenny.execute` only for operations not covered by domain tools
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
| `moneypenny.facts` | CRUD for durable facts — persistent knowledge across sessions. |
| `moneypenny.knowledge` | Ingest and retrieve documents — long-term reference library. |
| `moneypenny.policy` | Governance — control what agents can and cannot do. |
| `moneypenny.activity` | Query session history and audit trail. |
| `moneypenny.execute` | Escape hatch for any canonical operation. |

**Important:** These tools are MCP tools served by the Moneypenny sidecar
process. They must appear in your callable tool list. If they do not, the MCP
server is not connected — tell the user to run `mp setup cursor` and restart
Cursor (or reload the window).

## Tool usage

Each domain tool takes an `action` string and an `input` object.

### moneypenny.facts
- `search`: `{query, limit?}` — hybrid search across facts
- `add`: `{content, summary?, keywords?, confidence?}` — store a new fact
- `get`: `{id}` — retrieve a fact by ID
- `update`: `{id, content, summary?}` — update an existing fact
- `delete`: `{id, reason?}` — remove a fact

### moneypenny.knowledge
- `ingest`: `{path?, content?, title?}` — add a document (pass `path` as an HTTP URL to fetch a webpage, or provide `content` directly)
- `search`: `{query, limit?}` — search ingested documents
- `list`: `{}` — list all documents

### moneypenny.policy
- `add`: `{name, effect?, priority?, action_pattern?, resource_pattern?, sql_pattern?, message?}` — create a policy
- `list`: `{enabled?, effect?, limit?}` — list policies
- `disable`: `{id}` — disable a policy
- `evaluate`: `{actor, action, resource}` — test if action is allowed

### moneypenny.activity
- `query`: `{source?, event?, action?, resource?, query?, limit?}` — query events and decisions

### moneypenny.execute
- `op`: canonical operation name (e.g. `job.create`, `ingest.events`)
- `args`: operation-specific arguments

## When to use Moneypenny

- **User says "mp ..."**: Always route through Moneypenny
- **Remembering things**: Use `moneypenny.facts` action `add`
- **Recalling context**: Use `moneypenny.facts` action `search`
- **Ingesting documents**: Use `moneypenny.knowledge` action `ingest`
- **Activity trail**: Use `moneypenny.activity` action `query`
- **Governance**: Use `moneypenny.policy` to manage rules

## Best practices

- Search before inserting facts to avoid duplicates
- Use specific keywords when inserting facts
- Set confidence scores to reflect certainty (0.0 to 1.0)
- Use `moneypenny.execute` only for operations not covered by domain tools
"#.to_string();

    if let Some(conn) = agent_conn {
        md.push('\n');
        md.push_str(&mp_core::schema::generate_schema_summary(conn));
    }

    md
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

            match agent_turn(
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

                        if let Some(mcp_response) = handle_sidecar_mcp_request(
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

                        let response = match execute_sidecar_operation(
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

// =========================================================================
// Worker subprocess and inter-worker routing bus
// =========================================================================

struct WorkerHandle {
    pid: u32,
    agent_name: String,
    child: tokio::process::Child,
}

impl WorkerHandle {
    async fn shutdown(&mut self) {
        let _ = self.child.kill().await;
        tracing::info!(agent = %self.agent_name, pid = self.pid, "worker stopped");
    }
}

/// Holds the async stdin/stdout channels for one running worker process.
struct WorkerChannel {
    stdin: tokio::process::ChildStdin,
    stdout: tokio::io::BufReader<tokio::process::ChildStdout>,
}

/// Shared router that the gateway uses to send messages to worker processes
/// and read their responses.  Sequential per-worker: one in-flight request
/// at a time (the Mutex enforces this).
struct WorkerBus {
    channels: tokio::sync::Mutex<std::collections::HashMap<String, WorkerChannel>>,
}

impl WorkerBus {
    fn new() -> std::sync::Arc<Self> {
        std::sync::Arc::new(Self {
            channels: tokio::sync::Mutex::new(std::collections::HashMap::new()),
        })
    }

    async fn register(
        &self,
        agent_name: String,
        stdin: tokio::process::ChildStdin,
        stdout: tokio::process::ChildStdout,
    ) {
        let mut ch = self.channels.lock().await;
        ch.insert(
            agent_name,
            WorkerChannel {
                stdin,
                stdout: tokio::io::BufReader::new(stdout),
            },
        );
    }

    /// Send `message` to the named agent's worker and return its response text.
    /// Acquires the channel lock for the full round-trip so callers do not
    /// interleave on the same worker's stdio.
    async fn route(
        &self,
        target: &str,
        message: &str,
        session_id: Option<&str>,
    ) -> anyhow::Result<String> {
        let (response, _) = self.route_full(target, message, session_id).await?;
        Ok(response)
    }

    /// Like `route` but also returns the session_id echoed back by the worker.
    /// Channel adapters use this to maintain per-user session continuity.
    async fn route_full(
        &self,
        target: &str,
        message: &str,
        session_id: Option<&str>,
    ) -> anyhow::Result<(String, String)> {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
        let mut channels = self.channels.lock().await;
        let ch = channels
            .get_mut(target)
            .ok_or_else(|| anyhow::anyhow!("No running worker for agent '{target}'"))?;

        let req = serde_json::json!({"message": message, "session_id": session_id});
        ch.stdin.write_all(format!("{req}\n").as_bytes()).await?;
        ch.stdin.flush().await?;

        let mut line = String::new();
        ch.stdout.read_line(&mut line).await?;

        let resp: serde_json::Value = serde_json::from_str(line.trim())
            .map_err(|e| anyhow::anyhow!("worker response parse error: {e}"))?;
        if let Some(err) = resp["error"].as_str() {
            anyhow::bail!("worker reported error: {err}");
        }
        let response = resp["response"].as_str().unwrap_or("").to_string();
        let sid = resp["session_id"].as_str().unwrap_or("").to_string();
        Ok((response, sid))
    }
}

/// Spawn a worker subprocess for `agent_name`.
/// Returns the handle (for lifecycle management) plus the piped stdio channels
/// (to be registered in a `WorkerBus`).
/// The worker runs with CWD set to the config file's directory so relative
/// data_dir (e.g. mp-data) resolves correctly.
fn spawn_worker(
    _config: &Config,
    config_path: &Path,
    agent_name: &str,
) -> Result<(
    WorkerHandle,
    tokio::process::ChildStdin,
    tokio::process::ChildStdout,
)> {
    let exe = std::env::current_exe()?;
    // Resolve config path to absolute so worker can load it; worker CWD = config dir so data_dir resolves.
    let config_abs = if config_path.is_absolute() {
        config_path.to_path_buf()
    } else {
        std::env::current_dir()?.join(config_path)
    };
    let config_dir = config_abs.parent().unwrap_or_else(|| Path::new("."));
    let mut child = tokio::process::Command::new(&exe)
        .current_dir(config_dir)
        .arg("--config")
        .arg(&config_abs)
        .arg("worker")
        .arg("--agent")
        .arg(agent_name)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit())
        .spawn()?;

    let pid = child.id().unwrap_or(0);
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| anyhow::anyhow!("worker process has no stdin pipe"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("worker process has no stdout pipe"))?;

    Ok((
        WorkerHandle {
            pid,
            agent_name: agent_name.to_string(),
            child,
        },
        stdin,
        stdout,
    ))
}

/// Worker process: owns one agent's DB, processes messages from stdin.
/// Each line on stdin is a JSON message; each response is a JSON line on stdout.
async fn cmd_worker(config: &Config, agent_name: &str) -> Result<()> {
    let agent = resolve_agent(config, Some(agent_name))?;
    let conn = open_agent_db(config, &agent.name)?;
    let provider = build_provider(agent)?;
    let embed = build_embedding_provider(config, agent).ok();

    tracing::info!(agent = agent_name, "worker started");

    let stdin = tokio::io::stdin();
    let reader = tokio::io::BufReader::new(stdin);
    let mut lines = tokio::io::AsyncBufReadExt::lines(reader);
    let mut stdout = tokio::io::stdout();

    while let Ok(Some(line)) = lines.next_line().await {
        let request: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                let err = serde_json::json!({"error": e.to_string()});
                tokio::io::AsyncWriteExt::write_all(&mut stdout, format!("{err}\n").as_bytes())
                    .await?;
                continue;
            }
        };

        let msg = request["message"].as_str().unwrap_or("");
        let session_id = request["session_id"].as_str();

        let sid = if let Some(s) = session_id {
            s.to_string()
        } else {
            mp_core::store::log::create_session(&conn, &agent.name, Some("gateway"))?
        };

        let response = match agent_turn(
            &conn,
            provider.as_ref(),
            embed.as_deref(),
            &agent.name,
            &sid,
            agent.persona.as_deref(),
            msg,
            agent.policy_mode(),
            None,
        )
        .await
        {
            Ok(r) => serde_json::json!({"response": r, "session_id": sid}),
            Err(e) => serde_json::json!({"error": e.to_string(), "session_id": sid}),
        };

        tokio::io::AsyncWriteExt::write_all(&mut stdout, format!("{response}\n").as_bytes())
            .await?;
        tokio::io::AsyncWriteExt::flush(&mut stdout).await?;

        // Post-response extraction and rolling summarization
        let _ = extract_facts(&conn, provider.as_ref(), &agent.name, &sid).await;
        if let Some(ref ep) = embed {
            let model_id = embedding_model_id(agent);
            embed_pending(&conn, ep.as_ref(), &agent.name, &model_id).await;
        }
        maybe_summarize_session(&conn, provider.as_ref(), &sid).await;
    }

    tracing::info!(agent = agent_name, "worker exiting");
    Ok(())
}

/// Resolve an existing session or create a new one.
///
/// Priority: explicit `--session-id` > auto-resume recent > create new.
/// Set `force_new` to skip auto-resume.
fn resolve_or_create_session(
    conn: &rusqlite::Connection,
    agent_name: &str,
    channel: Option<&str>,
    requested_session_id: Option<String>,
    force_new: bool,
) -> Result<(String, bool)> {
    if let Some(sid) = requested_session_id {
        let exists: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sessions WHERE id = ?1 AND agent_id = ?2",
            rusqlite::params![sid, agent_name],
            |r| r.get(0),
        )?;
        if exists > 0 {
            return Ok((sid, true));
        }

        let recent: Vec<String> = conn
            .prepare(
                "SELECT id FROM sessions WHERE agent_id = ?1 ORDER BY started_at DESC LIMIT 3",
            )?
            .query_map([agent_name], |r| r.get(0))?
            .collect::<Result<Vec<_>, _>>()?;

        let hint = if recent.is_empty() {
            "No sessions exist yet. Omit --session-id to create one.".to_string()
        } else {
            format!(
                "Recent sessions: {}\nFix: use one of the IDs above, or omit --session-id.",
                recent.join(", ")
            )
        };
        anyhow::bail!("Session '{sid}' not found for agent '{agent_name}'.\n{hint}");
    }

    if !force_new {
        if let Ok(sid) = find_recent_resumable_session(conn, agent_name, channel) {
            return Ok((sid, true));
        }
    }

    let sid = mp_core::store::log::create_session(conn, agent_name, channel)?;
    Ok((sid, false))
}

fn find_recent_resumable_session(
    conn: &rusqlite::Connection,
    agent_name: &str,
    channel: Option<&str>,
) -> Result<String> {
    let cutoff = chrono::Utc::now().timestamp() - 24 * 3600;
    let sid: String = if let Some(ch) = channel {
        conn.query_row(
            "SELECT s.id
             FROM sessions s
             LEFT JOIN messages m ON m.session_id = s.id
             WHERE s.agent_id = ?1 AND s.channel = ?2
               AND s.ended_at IS NULL
             GROUP BY s.id
             HAVING COALESCE(MAX(m.created_at), s.started_at) >= ?3
             ORDER BY COALESCE(MAX(m.created_at), s.started_at) DESC
             LIMIT 1",
            rusqlite::params![agent_name, ch, cutoff],
            |r| r.get(0),
        )?
    } else {
        conn.query_row(
            "SELECT s.id
             FROM sessions s
             LEFT JOIN messages m ON m.session_id = s.id
             WHERE s.agent_id = ?1
               AND s.ended_at IS NULL
             GROUP BY s.id
             HAVING COALESCE(MAX(m.created_at), s.started_at) >= ?2
             ORDER BY COALESCE(MAX(m.created_at), s.started_at) DESC
             LIMIT 1",
            rusqlite::params![agent_name, cutoff],
            |r| r.get(0),
        )?
    };
    Ok(sid)
}

// =========================================================================
// Scheduler
// =========================================================================

async fn run_scheduler(config: &Config, shutdown: &mut tokio::sync::broadcast::Receiver<()>) {
    loop {
        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {}
            _ = shutdown.recv() => {
                tracing::info!("scheduler shutting down");
                return;
            }
        }

        for agent in &config.agents {
            let conn = match open_agent_db(config, &agent.name) {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!(agent = %agent.name, error = %e, "scheduler: failed to open db");
                    continue;
                }
            };

            let now = chrono::Utc::now().timestamp();
            let due_jobs = match mp_core::scheduler::poll_due_jobs(&conn, &agent.name, now) {
                Ok(jobs) => jobs,
                Err(e) => {
                    tracing::warn!(agent = %agent.name, error = %e, "scheduler: poll failed");
                    continue;
                }
            };

            for job in &due_jobs {
                tracing::info!(agent = %agent.name, job = %job.name, "scheduler: dispatching");
                let result = mp_core::scheduler::dispatch_job(&conn, job, &|j| {
                    mp_core::scheduler::execute_job_payload(&conn, j)
                });
                match result {
                    Ok(run) => {
                        tracing::info!(
                            agent = %agent.name, job = %job.name,
                            status = %run.status, "scheduler: job completed"
                        );
                    }
                    Err(e) => {
                        tracing::warn!(
                            agent = %agent.name, job = %job.name,
                            error = %e, "scheduler: dispatch failed"
                        );
                    }
                }
            }
        }
    }
}

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

        match agent_turn(
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

    let response = agent_turn(
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

fn op_request(
    agent_id: &str,
    op: &str,
    args: serde_json::Value,
) -> mp_core::operations::OperationRequest {
    let request_id = uuid::Uuid::new_v4().to_string();
    mp_core::operations::OperationRequest {
        op: op.to_string(),
        op_version: Some("v1".into()),
        request_id: Some(request_id.clone()),
        idempotency_key: None,
        actor: mp_core::operations::ActorContext {
            agent_id: agent_id.to_string(),
            tenant_id: None,
            user_id: None,
            channel: Some("cli".into()),
        },
        context: mp_core::operations::OperationContext {
            session_id: None,
            trace_id: Some(request_id),
            timestamp: Some(chrono::Utc::now().timestamp()),
        },
        args,
    }
}

#[derive(Debug, Deserialize)]
struct SidecarOperationInput {
    op: String,
    #[serde(default)]
    op_version: Option<String>,
    #[serde(default)]
    request_id: Option<String>,
    #[serde(default)]
    idempotency_key: Option<String>,
    #[serde(default)]
    actor: Option<mp_core::operations::ActorContext>,
    #[serde(default)]
    context: Option<mp_core::operations::OperationContext>,
    #[serde(default)]
    agent_id: Option<String>,
    #[serde(default)]
    tenant_id: Option<String>,
    #[serde(default)]
    user_id: Option<String>,
    #[serde(default)]
    channel: Option<String>,
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    trace_id: Option<String>,
    #[serde(default = "default_sidecar_args")]
    args: serde_json::Value,
}

fn default_sidecar_args() -> serde_json::Value {
    serde_json::json!({})
}

fn sidecar_error_response(code: &str, message: impl Into<String>) -> serde_json::Value {
    serde_json::json!({
        "ok": false,
        "code": code,
        "message": message.into(),
        "data": {},
        "policy": null,
        "audit": { "recorded": false }
    })
}

fn mcp_tools_list_result() -> serde_json::Value {
    domain_tools::tools_list()
}

fn jsonrpc_result(id: Option<serde_json::Value>, result: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id.unwrap_or(serde_json::Value::Null),
        "result": result
    })
}

fn jsonrpc_error(
    id: Option<serde_json::Value>,
    code: i64,
    message: impl Into<String>,
) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id.unwrap_or(serde_json::Value::Null),
        "error": {
            "code": code,
            "message": message.into()
        }
    })
}

fn request_id_from_jsonrpc(input: &serde_json::Value) -> Option<String> {
    let id = input.get("id")?;
    match id {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        serde_json::Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct SidecarToolStats {
    selection_count: u64,
    success_count: u64,
    error_count: u64,
    fallback_count: u64,
    invalid_action_count: u64,
}

static SIDECAR_TOOL_STATS: LazyLock<Mutex<HashMap<String, SidecarToolStats>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn record_sidecar_tool_event(tool: &str, success: bool, fallback: bool, invalid_action: bool) {
    if let Ok(mut guard) = SIDECAR_TOOL_STATS.lock() {
        let entry = guard.entry(tool.to_string()).or_default();
        entry.selection_count += 1;
        if success {
            entry.success_count += 1;
        } else {
            entry.error_count += 1;
        }
        if fallback {
            entry.fallback_count += 1;
        }
        if invalid_action {
            entry.invalid_action_count += 1;
        }
    }
}

fn sidecar_tool_stats_snapshot() -> serde_json::Value {
    if let Ok(guard) = SIDECAR_TOOL_STATS.lock() {
        let mut rows = Vec::new();
        for (tool, s) in guard.iter() {
            let selection = s.selection_count.max(1) as f64;
            rows.push(serde_json::json!({
                "tool": tool,
                "selection_rate": s.selection_count,
                "success_rate": (s.success_count as f64) / selection,
                "fallback_rate": (s.fallback_count as f64) / selection,
                "invalid_action_rate": (s.invalid_action_count as f64) / selection,
                "errors": s.error_count
            }));
        }
        serde_json::json!(rows)
    } else {
        serde_json::json!([])
    }
}

enum ParsedMcpToolCall {
    Operation {
        request: mp_core::operations::OperationRequest,
        tool: String,
        action: String,
        fallback: bool,
    },
    DirectResponse {
        payload: serde_json::Value,
        tool: String,
    },
    MpqQuery {
        expression: String,
        dry_run: bool,
        agent_id: String,
        channel: Option<String>,
        session_id: Option<String>,
        trace_id: Option<String>,
    },
}

fn build_sidecar_request_from_mcp_call(
    input: &serde_json::Value,
    default_agent_id: &str,
) -> anyhow::Result<ParsedMcpToolCall> {
    let params = input
        .get("params")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| anyhow::anyhow!("missing object params for tools/call"))?;
    let tool_name = params
        .get("name")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("missing params.name"))?;
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    let request_id = params
        .get("request_id")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .or_else(|| request_id_from_jsonrpc(input));

    let tool_specific = if tool_name.starts_with("moneypenny.") || tool_name.starts_with("moneypenny_") {
        Some(domain_tools::route_tool_call(tool_name, &arguments)?)
    } else {
        None
    };

    let (op, args, tool_label, action, fallback) = match tool_specific {
        Some(domain_tools::RoutedToolCall::MpqQuery {
            expression,
            dry_run,
        }) => {
            let agent_id = params
                .get("agent_id")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(default_agent_id)
                .to_string();
            let channel = params
                .get("channel")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string);
            let session_id = params
                .get("session_id")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string);
            let trace_id = params
                .get("trace_id")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
                .or_else(|| request_id.clone());
            return Ok(ParsedMcpToolCall::MpqQuery {
                expression,
                dry_run,
                agent_id,
                channel,
                session_id,
                trace_id,
            });
        }
        Some(domain_tools::RoutedToolCall::Capabilities { payload }) => {
            return Ok(ParsedMcpToolCall::DirectResponse {
                payload,
                tool: tool_name.to_string(),
            });
        }
        Some(domain_tools::RoutedToolCall::Operation {
            domain_tool,
            action,
            op,
            args,
            execute_fallback,
        }) => (op, args, domain_tool, action, execute_fallback),
        None => (
            tool_name.to_string(),
            arguments,
            tool_name.to_string(),
            "legacy".to_string(),
            false,
        ),
    };
    let request_id = params
        .get("request_id")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .or(request_id);

    let request = build_sidecar_request(
        serde_json::json!({
            "op": op,
            "request_id": request_id,
            "idempotency_key": params.get("idempotency_key").cloned(),
            "agent_id": params.get("agent_id").cloned(),
            "tenant_id": params.get("tenant_id").cloned(),
            "user_id": params.get("user_id").cloned(),
            "channel": params.get("channel").cloned(),
            "session_id": params.get("session_id").cloned(),
            "trace_id": params.get("trace_id").cloned(),
            "args": args
        }),
        default_agent_id,
    )?;

    Ok(ParsedMcpToolCall::Operation {
        request,
        tool: tool_label,
        action,
        fallback,
    })
}

async fn handle_sidecar_mcp_request(
    conn: &rusqlite::Connection,
    input: &serde_json::Value,
    default_agent_id: &str,
    embed_provider: Option<&dyn EmbeddingProvider>,
    embedding_model_id: &str,
) -> anyhow::Result<Option<serde_json::Value>> {
    let method = match input.get("method").and_then(serde_json::Value::as_str) {
        Some(m) => m,
        None => return Ok(None),
    };
    let id = input.get("id").cloned();
    if id.is_none() {
        return Ok(None);
    }

    let response = match method {
        "initialize" => jsonrpc_result(
            id,
            serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": { "listChanged": false }
                },
                "serverInfo": {
                    "name": "moneypenny-sidecar",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }),
        ),
        "tools/list" => jsonrpc_result(id, mcp_tools_list_result()),
        "tools/call" => {
            let parsed_call = match build_sidecar_request_from_mcp_call(input, default_agent_id) {
                Ok(r) => r,
                Err(e) => {
                    // likely invalid action/shape for domain tools
                    record_sidecar_tool_event("moneypenny.invalid", false, false, true);
                    return Ok(Some(jsonrpc_error(
                        id,
                        -32602,
                        format!("invalid tools/call params: {e}"),
                    )));
                }
            };
            match parsed_call {
                ParsedMcpToolCall::MpqQuery {
                    expression,
                    dry_run,
                    agent_id,
                    channel,
                    session_id,
                    trace_id,
                } => {
                    let ctx = mp_core::dsl::ExecuteContext {
                        agent_id,
                        channel,
                        session_id,
                        trace_id,
                    };
                    let resp = mp_core::dsl::run(conn, &expression, dry_run, &ctx);
                    record_sidecar_tool_event(domain_tools::TOOL_QUERY, resp.ok, false, false);
                    let text = serde_json::to_string(&serde_json::json!({
                        "ok": resp.ok,
                        "code": resp.code,
                        "message": resp.message,
                        "data": resp.data,
                    }))
                    .unwrap_or_else(|_| "{}".to_string());
                    jsonrpc_result(
                        id,
                        serde_json::json!({
                            "content": [{
                                "type": "text",
                                "text": text
                            }],
                            "isError": !resp.ok
                        }),
                    )
                }
                ParsedMcpToolCall::DirectResponse { payload, tool } => {
                    let payload = if tool == domain_tools::TOOL_CAPABILITIES {
                        let mut p = payload;
                        if let Some(obj) = p.as_object_mut() {
                            obj.insert("telemetry".to_string(), sidecar_tool_stats_snapshot());
                        }
                        p
                    } else {
                        payload
                    };
                    record_sidecar_tool_event(&tool, true, false, false);
                    jsonrpc_result(
                        id,
                        serde_json::json!({
                            "content": [{
                                "type": "text",
                                "text": serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string())
                            }],
                            "isError": false
                        }),
                    )
                }
                ParsedMcpToolCall::Operation {
                    request,
                    tool,
                    action,
                    fallback,
                } => {
                    if fallback && domain_tools::covered_ops().contains(&request.op.as_str()) {
                        record_sidecar_tool_event(&tool, false, true, false);
                    }

                    let op_resp = match execute_sidecar_operation(
                        conn,
                        &request,
                        embed_provider,
                        embedding_model_id,
                    )
                    .await
                    {
                        Ok(mut resp) => {
                            if let Some(obj) = resp.data.as_object_mut() {
                                obj.insert(
                                    "next_actions".to_string(),
                                    serde_json::Value::Array(domain_tools::next_actions(
                                        &tool, &action,
                                    )),
                                );
                            }
                            record_sidecar_tool_event(&tool, resp.ok, fallback, false);
                            resp
                        }
                        Err(e) => {
                            record_sidecar_tool_event(&tool, false, fallback, false);
                            let err =
                                sidecar_error_response("sidecar_execute_error", e.to_string());
                            return Ok(Some(jsonrpc_result(
                                id,
                                serde_json::json!({
                                    "content": [{ "type": "text", "text": err.to_string() }],
                                    "isError": true
                                }),
                            )));
                        }
                    };

                    jsonrpc_result(
                        id,
                        serde_json::json!({
                            "content": [{
                                "type": "text",
                                "text": serde_json::to_string(&op_resp).unwrap_or_else(|_| "{}".to_string())
                            }],
                            "isError": !op_resp.ok
                        }),
                    )
                }
            }
        }
        unknown if unknown.starts_with("notifications/") && id.is_none() => return Ok(None),
        _ => jsonrpc_error(id, -32601, format!("method not found: {method}")),
    };

    Ok(Some(response))
}

async fn execute_sidecar_operation(
    conn: &rusqlite::Connection,
    req: &mp_core::operations::OperationRequest,
    embed_provider: Option<&dyn EmbeddingProvider>,
    embedding_model_id: &str,
) -> anyhow::Result<mp_core::operations::OperationResponse> {
    if req.op == "embedding.process" || req.op == "embedding.backfill.process" {
        return execute_embedding_process_operation(conn, req, embed_provider, embedding_model_id)
            .await;
    }
    let maybe_enriched = enrich_memory_search_request_with_embedding(req, embed_provider).await;
    mp_core::operations::execute(conn, &maybe_enriched)
}

async fn enrich_memory_search_request_with_embedding(
    req: &mp_core::operations::OperationRequest,
    embed_provider: Option<&dyn EmbeddingProvider>,
) -> mp_core::operations::OperationRequest {
    if req.op != "memory.search" {
        return req.clone();
    }
    let Some(embedder) = embed_provider else {
        return req.clone();
    };
    if req.args.get("__query_embedding").is_some() || req.args.get("query_embedding").is_some() {
        return req.clone();
    }

    let mut enriched = req.clone();
    let Some(query) = enriched
        .args
        .get("query")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|q| !q.is_empty())
    else {
        return req.clone();
    };

    match embedder.embed(query).await {
        Ok(vec) => {
            let embedding = vec
                .into_iter()
                .map(|v| serde_json::Value::from(v as f64))
                .collect::<Vec<_>>();
            if let Some(obj) = enriched.args.as_object_mut() {
                obj.insert(
                    "__query_embedding".to_string(),
                    serde_json::Value::Array(embedding),
                );
            }
            enriched
        }
        Err(e) => {
            tracing::debug!("sidecar memory.search embedding generation failed: {e}");
            req.clone()
        }
    }
}

async fn execute_embedding_process_operation(
    conn: &rusqlite::Connection,
    req: &mp_core::operations::OperationRequest,
    embed_provider: Option<&dyn EmbeddingProvider>,
    default_model_id: &str,
) -> anyhow::Result<mp_core::operations::OperationResponse> {
    let Some(embed) = embed_provider else {
        return Ok(mp_core::operations::OperationResponse {
            ok: false,
            code: "embedding_provider_unavailable".into(),
            message: "embedding provider is not configured or failed to initialize".into(),
            data: serde_json::json!({}),
            policy: None,
            audit: mp_core::operations::AuditMeta { recorded: false },
        });
    };

    let agent_id = req.args["agent_id"]
        .as_str()
        .unwrap_or(&req.actor.agent_id)
        .to_string();
    let model_id = req.args["model_id"]
        .as_str()
        .unwrap_or(default_model_id)
        .to_string();
    let limit_per_target = req.args["limit"].as_u64().unwrap_or(10_000) as usize;
    let max_batches = req.args["max_batches"].as_u64().unwrap_or(200) as usize;
    let batch_size = req.args["batch_size"].as_u64().unwrap_or(128) as usize;
    let retry_base_seconds = req.args["retry_base_seconds"].as_i64().unwrap_or(5);
    let max_attempts = req.args["max_attempts"].as_i64().unwrap_or(8);
    let enqueue_drift = req.op == "embedding.backfill.process"
        || req.args["enqueue_drift"].as_bool().unwrap_or(false);

    let mut total_queued = 0usize;
    if enqueue_drift {
        total_queued = mp_core::store::embedding::enqueue_drift_jobs(
            conn,
            &agent_id,
            &model_id,
            limit_per_target,
        )?;
    }

    let mut total_claimed = 0usize;
    let mut total_embedded = 0usize;
    let mut total_failed = 0usize;
    let mut total_skipped = 0usize;
    let mut rounds = 0usize;

    loop {
        rounds += 1;
        let stats = mp_core::store::embedding::process_embedding_jobs(
            conn,
            &agent_id,
            &model_id,
            batch_size.max(1),
            retry_base_seconds.max(1),
            max_attempts.max(1),
            |content| async move {
                let vec = embed.embed(&content).await?;
                Ok::<Vec<u8>, anyhow::Error>(mp_llm::f32_slice_to_blob(&vec))
            },
        )
        .await?;
        total_claimed += stats.claimed;
        total_embedded += stats.embedded;
        total_failed += stats.failed;
        total_skipped += stats.skipped;

        if stats.claimed == 0 || rounds >= max_batches.max(1) {
            break;
        }
    }

    let queue = mp_core::store::embedding::queue_stats(conn)?;
    Ok(mp_core::operations::OperationResponse {
        ok: true,
        code: "ok".into(),
        message: "embedding queue processed".into(),
        data: serde_json::json!({
            "agent_id": agent_id,
            "model_id": model_id,
            "enqueue_drift": enqueue_drift,
            "queued": total_queued,
            "rounds": rounds,
            "claimed": total_claimed,
            "embedded": total_embedded,
            "failed": total_failed,
            "skipped": total_skipped,
            "queue": {
                "total": queue.total,
                "pending": queue.pending,
                "retry": queue.retry,
                "processing": queue.processing,
                "dead": queue.dead,
            }
        }),
        policy: None,
        audit: mp_core::operations::AuditMeta { recorded: true },
    })
}

fn build_sidecar_request(
    input: serde_json::Value,
    default_agent_id: &str,
) -> anyhow::Result<mp_core::operations::OperationRequest> {
    if let Ok(req) = serde_json::from_value::<mp_core::operations::OperationRequest>(input.clone())
    {
        return Ok(req);
    }

    let compact: SidecarOperationInput = serde_json::from_value(input)?;
    let request_id = compact
        .request_id
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let actor = compact.actor.unwrap_or(mp_core::operations::ActorContext {
        agent_id: compact
            .agent_id
            .unwrap_or_else(|| default_agent_id.to_string()),
        tenant_id: compact.tenant_id,
        user_id: compact.user_id,
        channel: compact.channel.or(Some("mcp-stdio".into())),
    });

    let mut context = compact.context.unwrap_or_default();
    if context.session_id.is_none() {
        context.session_id = compact.session_id;
    }
    if context.trace_id.is_none() {
        context.trace_id = compact.trace_id.or(Some(request_id.clone()));
    }
    if context.timestamp.is_none() {
        context.timestamp = Some(chrono::Utc::now().timestamp());
    }

    Ok(mp_core::operations::OperationRequest {
        op: compact.op,
        op_version: compact.op_version.or(Some("v1".into())),
        request_id: Some(request_id),
        idempotency_key: compact.idempotency_key,
        actor,
        context,
        args: compact.args,
    })
}

async fn cmd_sidecar(config: &Config, agent: Option<String>) -> Result<()> {
    let ag = resolve_agent(config, agent.as_deref())?;
    let conn = open_agent_db(config, &ag.name)?;
    let embed_provider = build_embedding_provider(config, ag).ok();
    let sidecar_embedding_model_id = embedding_model_id(ag);

    let stdin = tokio::io::stdin();
    let reader = tokio::io::BufReader::new(stdin);
    let mut lines = tokio::io::AsyncBufReadExt::lines(reader);
    let mut stdout = tokio::io::stdout();

    while let Ok(Some(line)) = lines.next_line().await {
        let parsed: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                let err = sidecar_error_response("invalid_json", e.to_string());
                tokio::io::AsyncWriteExt::write_all(&mut stdout, format!("{err}\n").as_bytes())
                    .await?;
                tokio::io::AsyncWriteExt::flush(&mut stdout).await?;
                continue;
            }
        };

        if let Some(mcp_response) = handle_sidecar_mcp_request(
            &conn,
            &parsed,
            &ag.name,
            embed_provider.as_deref(),
            &sidecar_embedding_model_id,
        )
        .await?
        {
            tokio::io::AsyncWriteExt::write_all(
                &mut stdout,
                format!("{mcp_response}\n").as_bytes(),
            )
            .await?;
            tokio::io::AsyncWriteExt::flush(&mut stdout).await?;
            continue;
        }

        // JSON-RPC notifications (method present, no id) were already handled
        // above — don't fall through to the sidecar-op path.
        if parsed.get("method").is_some() && parsed.get("id").is_none() {
            continue;
        }

        let request = match build_sidecar_request(parsed, &ag.name) {
            Ok(r) => r,
            Err(e) => {
                let err = sidecar_error_response("invalid_request", e.to_string());
                tokio::io::AsyncWriteExt::write_all(&mut stdout, format!("{err}\n").as_bytes())
                    .await?;
                tokio::io::AsyncWriteExt::flush(&mut stdout).await?;
                continue;
            }
        };

        let response = match execute_sidecar_operation(
            &conn,
            &request,
            embed_provider.as_deref(),
            &sidecar_embedding_model_id,
        )
        .await
        {
            Ok(resp) => serde_json::to_value(resp)
                .unwrap_or_else(|e| sidecar_error_response("serialization_error", e.to_string())),
            Err(e) => sidecar_error_response("sidecar_execute_error", e.to_string()),
        };

        tokio::io::AsyncWriteExt::write_all(&mut stdout, format!("{response}\n").as_bytes())
            .await?;
        tokio::io::AsyncWriteExt::flush(&mut stdout).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        ParsedMcpToolCall, build_sidecar_request, build_sidecar_request_from_mcp_call,
        mcp_tools_list_result,
    };

    #[test]
    fn sidecar_compact_request_uses_defaults() {
        let req = build_sidecar_request(
            serde_json::json!({
                "op": "job.list",
                "args": { "agent_id": "main" }
            }),
            "default-agent",
        )
        .expect("build sidecar request");
        assert_eq!(req.op, "job.list");
        assert_eq!(req.op_version.as_deref(), Some("v1"));
        assert_eq!(req.actor.agent_id, "default-agent");
        assert_eq!(req.actor.channel.as_deref(), Some("mcp-stdio"));
        assert!(req.request_id.is_some());
        assert!(req.context.trace_id.is_some());
    }

    #[test]
    fn sidecar_full_operation_request_passes_through() {
        let req = build_sidecar_request(
            serde_json::json!({
                "op": "session.list",
                "op_version": "v1",
                "request_id": "rid-1",
                "idempotency_key": null,
                "actor": {
                    "agent_id": "main",
                    "tenant_id": null,
                    "user_id": null,
                    "channel": "cli"
                },
                "context": {
                    "session_id": null,
                    "trace_id": "trace-1",
                    "timestamp": 123
                },
                "args": { "limit": 3 }
            }),
            "default-agent",
        )
        .expect("parse full canonical request");
        assert_eq!(req.request_id.as_deref(), Some("rid-1"));
        assert_eq!(req.context.trace_id.as_deref(), Some("trace-1"));
        assert_eq!(req.actor.channel.as_deref(), Some("cli"));
        assert_eq!(req.args["limit"], 3);
    }

    #[test]
    fn sidecar_mcp_tools_call_translates_to_canonical_request() {
        let parsed = build_sidecar_request_from_mcp_call(
            &serde_json::json!({
                "jsonrpc": "2.0",
                "id": "rpc-42",
                "method": "tools/call",
                "params": {
                    "name": "moneypenny.ingest",
                    "arguments": { "action": "status", "input": { "limit": 5 } },
                    "agent_id": "main"
                }
            }),
            "default-agent",
        )
        .expect("translate tools/call to operation request");
        let req = match parsed {
            ParsedMcpToolCall::Operation { request, .. } => request,
            _ => panic!("expected operation"),
        };
        assert_eq!(req.op, "ingest.status");
        assert_eq!(req.request_id.as_deref(), Some("rpc-42"));
        assert_eq!(req.actor.agent_id, "main");
        assert_eq!(req.args["limit"], 5);
    }

    #[test]
    fn sidecar_mcp_prefixed_tool_name_maps_to_operation() {
        let parsed = build_sidecar_request_from_mcp_call(
            &serde_json::json!({
                "jsonrpc": "2.0",
                "id": "rpc-43",
                "method": "tools/call",
                "params": {
                    "name": "moneypenny.jobs",
                    "arguments": { "action": "list", "input": { "agent_id": "main" } }
                }
            }),
            "default-agent",
        )
        .expect("translate prefixed tools/call to operation request");
        let req = match parsed {
            ParsedMcpToolCall::Operation { request, .. } => request,
            _ => panic!("expected operation"),
        };
        assert_eq!(req.op, "job.list");
    }

    #[test]
    fn sidecar_mcp_tools_list_exposes_domain_tools() {
        let result = mcp_tools_list_result();
        let tools = result["tools"].as_array().cloned().unwrap_or_default();
        assert_eq!(tools.len(), 5, "MCP surface: facts + knowledge + policy + activity + execute");
        assert!(tools.iter().any(|t| t["name"] == "moneypenny.facts"));
        assert!(tools.iter().any(|t| t["name"] == "moneypenny.knowledge"));
        assert!(tools.iter().any(|t| t["name"] == "moneypenny.policy"));
        assert!(tools.iter().any(|t| t["name"] == "moneypenny.activity"));
        assert!(tools.iter().any(|t| t["name"] == "moneypenny.execute"));
        // DSL/query tool should NOT be on the MCP surface
        assert!(!tools.iter().any(|t| t["name"] == "moneypenny.query"));
    }

    #[test]
    fn sidecar_mcp_capabilities_returns_direct_payload() {
        let parsed = build_sidecar_request_from_mcp_call(
            &serde_json::json!({
                "jsonrpc": "2.0",
                "id": "rpc-44",
                "method": "tools/call",
                "params": {
                    "name": "moneypenny.capabilities",
                    "arguments": {}
                }
            }),
            "default-agent",
        )
        .expect("build capabilities call");
        match parsed {
            ParsedMcpToolCall::DirectResponse { payload, .. } => {
                assert!(payload["domains"].is_array());
            }
            _ => panic!("expected direct response"),
        }
    }
}

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

fn parse_duration_hours(s: &str) -> i64 {
    if let Some(d) = s.strip_suffix('d') {
        d.parse::<i64>().unwrap_or(7) * 24
    } else if let Some(h) = s.strip_suffix('h') {
        h.parse::<i64>().unwrap_or(24)
    } else {
        168 // default: 7 days
    }
}

fn normalize_embedding_target(value: &str) -> Option<&'static str> {
    match value.trim().to_ascii_lowercase().as_str() {
        "facts" | "fact" => Some("facts"),
        "messages" | "message" | "msg" => Some("messages"),
        "tool_calls" | "tool-calls" | "toolcalls" | "tool_call" => Some("tool_calls"),
        "policy_audit" | "policy-audit" | "policyaudit" | "policy" => Some("policy_audit"),
        "chunks" | "chunk" | "knowledge" => Some("chunks"),
        _ => None,
    }
}

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
    if project_cursor_mcp.exists() {
        ui::success(format!("Cursor MCP config found: {}", project_cursor_mcp.display()));
    }
    if project_claude_mcp.exists() {
        ui::success(format!(
            "Claude Code MCP config found: {}",
            project_claude_mcp.display()
        ));
    }
    if !project_cursor_mcp.exists() && !project_claude_mcp.exists() {
        warnings += 1;
        ui::warn("No local MCP config found in this project.");
        ui::hint("Run one of:");
        ui::hint("- mp setup cursor --local");
        ui::hint("- mp setup claude-code");
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

fn toml_to_json(v: &toml::Value) -> serde_json::Value {
    match v {
        toml::Value::String(s) => serde_json::Value::String(s.clone()),
        toml::Value::Integer(i) => serde_json::json!(i),
        toml::Value::Float(f) => serde_json::json!(f),
        toml::Value::Boolean(b) => serde_json::json!(b),
        toml::Value::Datetime(d) => serde_json::Value::String(d.to_string()),
        toml::Value::Array(a) => serde_json::Value::Array(a.iter().map(toml_to_json).collect()),
        toml::Value::Table(t) => {
            let mut map = serde_json::Map::new();
            for (k, val) in t {
                map.insert(k.clone(), toml_to_json(val));
            }
            serde_json::Value::Object(map)
        }
    }
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn sql_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}
