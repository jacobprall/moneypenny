mod cli;
mod adapters;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Command};
use mp_core::config::Config;
use mp_llm::provider::{EmbeddingProvider, LlmProvider};
use serde::Deserialize;
use std::collections::HashSet;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use tracing_subscriber::{EnvFilter, fmt};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    if matches!(cli.command, Command::Init) {
        return cmd_init(&cli.config).await;
    }

    let config_path = Path::new(&cli.config);
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
        Command::Stop => cmd_stop(&config).await,
        Command::Agent(cmd) => cmd_agent(&config, cmd).await,
        Command::Chat { agent, session_id } => cmd_chat(&config, agent, session_id).await,
        Command::Send { agent, message, session_id } => {
            cmd_send(&config, &agent, &message, session_id).await
        },
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
            )
            .await
        },
        Command::Session(cmd) => cmd_session(&config, cmd).await,
        Command::Knowledge(cmd) => cmd_knowledge(&config, cmd).await,
        Command::Skill(cmd) => cmd_skill(&config, cmd).await,
        Command::Policy(cmd) => cmd_policy(&config, cmd).await,
        Command::Job(cmd) => cmd_job(&config, cmd).await,
        Command::Audit { agent, command } => cmd_audit(&config, agent, command).await,
        Command::Sync(cmd) => cmd_sync(&config, cmd).await,
        Command::Db(cmd) => cmd_db(&config, cmd).await,
        Command::Health => cmd_health(&config).await,
        Command::Worker { agent } => cmd_worker(&config, &agent).await,
        Command::Sidecar { agent } => cmd_sidecar(&config, agent).await,
    }
}

fn init_logging(level: &str) {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(level));

    fmt()
        .with_env_filter(filter)
        .with_target(true)
        .init();
}

fn resolve_agent<'a>(config: &'a Config, name: Option<&str>) -> Result<&'a mp_core::config::AgentConfig> {
    match name {
        Some(n) => config.agents.iter().find(|a| a.name == n)
            .ok_or_else(|| anyhow::anyhow!("Agent '{n}' not found in config")),
        None => config.agents.first()
            .ok_or_else(|| anyhow::anyhow!("No agents configured")),
    }
}

fn open_agent_db(config: &Config, agent_name: &str) -> Result<rusqlite::Connection> {
    let db_path = config.agent_db_path(agent_name);
    let conn = mp_core::db::open(&db_path)?;
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
                Ok(n) if n > 0 => tracing::info!(agent = agent_name, tools = n, "MCP tools registered"),
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
    let mut stmt = match conn.prepare(
        "SELECT name, description FROM skills WHERE tool_id LIKE 'sqlite_js:%'"
    ) {
        Ok(s) => s,
        Err(_) => return vec![],
    };
    stmt.query_map([], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    })
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
        conn, agent_id, session_id, persona, user_message, &budget, None,
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
        resource: "conversation",
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
            planned_calls.iter().map(|tc| mp_llm::types::ToolCall {
                id: tc.id.clone(),
                name: tc.name.clone(),
                arguments: tc.arguments.clone(),
            }).collect(),
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
                conn, session_id, "assistant",
                &format!("[tool: {}]", tc.name),
            )?;
            let mut effective_arguments = tc.arguments.clone();
            if tc.name == "memory_search" {
                effective_arguments = enrich_memory_search_args_with_embedding(
                    &effective_arguments,
                    embed_provider,
                ).await;
            }

            // Delegation tool is handled at the gateway layer (not via the registry).
            if tc.name == "delegate_to_agent" {
                let args: serde_json::Value = serde_json::from_str(&tc.arguments)
                    .unwrap_or_default();
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
                    format!("Delegation to '{target}' is only available in gateway mode (mp start).")
                };

                tracing::info!(target, "delegation tool call");
                messages.push(mp_llm::types::Message::tool(&delegation_result, &tc.id));
                continue;
            }

            let result = mp_core::tools::registry::execute(
                conn, agent_id, session_id, &msg_id,
                &tc.name, &effective_arguments,
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

    let new_messages: Vec<String> = recent.iter()
        .map(|m| format!("{}: {}", m.role, m.content))
        .collect();

    let extraction_ctx = mp_core::extraction::assemble_extraction_context(
        conn, agent_id, session_id, &new_messages, 30,
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

    let json_text = text.trim()
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
    let outcomes = mp_core::extraction::run_pipeline(
        conn, agent_id, session_id, &candidates, last_msg_id,
    )?;

    let extracted = outcomes.iter().filter(|o| o.policy_allowed).count();
    if extracted > 0 {
        tracing::info!(count = extracted, "facts extracted");
    }
    Ok(extracted)
}

/// Compute and store FLOAT32 embeddings for any facts/messages/chunks that are missing them,
/// then rebuild the vector quantized index so `vector_quantize_scan` stays fresh.
///
/// Runs after each extraction pass. Idempotent — only processes NULL-embedding rows.
async fn embed_pending(
    conn: &rusqlite::Connection,
    embed: &dyn mp_llm::provider::EmbeddingProvider,
    agent_id: &str,
) {
    // --- Facts ---
    let ids = match mp_core::store::facts::ids_without_embedding(conn, agent_id) {
        Ok(v) => v,
        Err(e) => { tracing::warn!("embed_pending: facts query failed: {e}"); return; }
    };

    let mut embedded = 0usize;
    for id in &ids {
        let content: String = match conn.query_row(
            "SELECT content FROM facts WHERE id = ?1",
            rusqlite::params![id],
            |r| r.get(0),
        ) {
            Ok(c) => c,
            Err(_) => continue,
        };

        match embed.embed(&content).await {
            Ok(vec) => {
                let blob = mp_llm::f32_slice_to_blob(&vec);
                if mp_core::store::facts::set_content_embedding(conn, id, &blob).is_ok() {
                    embedded += 1;
                }
            }
            Err(e) => tracing::warn!(fact_id = %id, "embedding failed: {e}"),
        }
    }

    // --- Messages (session/log data) ---
    let messages = match mp_core::store::log::messages_without_embedding(conn, agent_id) {
        Ok(v) => v,
        Err(e) => { tracing::warn!("embed_pending: messages query failed: {e}"); return; }
    };

    for (message_id, content) in &messages {
        match embed.embed(content).await {
            Ok(vec) => {
                let blob = mp_llm::f32_slice_to_blob(&vec);
                if mp_core::store::log::set_message_embedding(conn, message_id, &blob).is_ok() {
                    embedded += 1;
                }
            }
            Err(e) => tracing::warn!(message_id = %message_id, "embedding failed: {e}"),
        }
    }

    // --- Tool calls ---
    let tool_calls = match mp_core::store::log::tool_calls_without_embedding(conn, agent_id) {
        Ok(v) => v,
        Err(e) => { tracing::warn!("embed_pending: tool_calls query failed: {e}"); return; }
    };

    for (tool_call_id, content) in &tool_calls {
        match embed.embed(content).await {
            Ok(vec) => {
                let blob = mp_llm::f32_slice_to_blob(&vec);
                if mp_core::store::log::set_tool_call_embedding(conn, tool_call_id, &blob).is_ok() {
                    embedded += 1;
                }
            }
            Err(e) => tracing::warn!(tool_call_id = %tool_call_id, "embedding failed: {e}"),
        }
    }

    // --- Policy audit ---
    let policy_audit_rows = match mp_core::store::log::policy_audit_without_embedding(conn, agent_id) {
        Ok(v) => v,
        Err(e) => { tracing::warn!("embed_pending: policy_audit query failed: {e}"); return; }
    };

    for (audit_id, content) in &policy_audit_rows {
        match embed.embed(content).await {
            Ok(vec) => {
                let blob = mp_llm::f32_slice_to_blob(&vec);
                if mp_core::store::log::set_policy_audit_embedding(conn, audit_id, &blob).is_ok() {
                    embedded += 1;
                }
            }
            Err(e) => tracing::warn!(audit_id = %audit_id, "embedding failed: {e}"),
        }
    }

    // --- Chunks ---
    let chunks = match mp_core::store::knowledge::chunks_without_embedding(conn) {
        Ok(v) => v,
        Err(e) => { tracing::warn!("embed_pending: chunks query failed: {e}"); return; }
    };

    for (chunk_id, content) in &chunks {
        match embed.embed(content).await {
            Ok(vec) => {
                let blob = mp_llm::f32_slice_to_blob(&vec);
                if mp_core::store::knowledge::set_chunk_embedding(conn, chunk_id, &blob).is_ok() {
                    embedded += 1;
                }
            }
            Err(e) => tracing::warn!(chunk_id = %chunk_id, "embedding failed: {e}"),
        }
    }

    if embedded > 0 {
        // Rebuild the quantized vector index so new embeddings are searchable.
        for (table, col) in &[
            ("facts", "content_embedding"),
            ("messages", "content_embedding"),
            ("tool_calls", "content_embedding"),
            ("policy_audit", "content_embedding"),
            ("chunks", "content_embedding"),
        ] {
            let _ = conn.execute("SELECT vector_quantize(?1, ?2)", rusqlite::params![table, col]);
        }
        tracing::debug!(count = embedded, "embeddings updated and indexes rebuilt");
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
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM messages WHERE session_id = ?1",
        rusqlite::params![session_id],
        |r| r.get(0),
    ).unwrap_or(0);

    // Only run on exact multiples so we don't re-summarize the same window
    if count < SUMMARIZE_EVERY as i64 || count % SUMMARIZE_EVERY as i64 != 0 {
        return;
    }

    let all = match mp_core::store::log::get_messages(conn, session_id) {
        Ok(m) => m,
        Err(e) => { tracing::warn!("summarize: failed to load messages: {e}"); return; }
    };

    let keep = RECENT_KEEP.min(all.len());
    let to_summarize = &all[..all.len().saturating_sub(keep)];
    if to_summarize.is_empty() {
        return;
    }

    let existing: Option<String> = conn.query_row(
        "SELECT summary FROM sessions WHERE id = ?1",
        rusqlite::params![session_id],
        |r| r.get(0),
    ).unwrap_or(None);

    let conv_text = to_summarize.iter()
        .map(|m| format!("{}: {}", m.role, m.content))
        .collect::<Vec<_>>()
        .join("\n");

    let user_prompt = match &existing {
        Some(prev) if !prev.trim().is_empty() =>
            format!("Prior summary:\n{prev}\n\nNew conversation to incorporate:\n{conv_text}"),
        _ =>
            format!("Conversation:\n{conv_text}"),
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
                    if let Err(e) = mp_core::store::log::update_summary(conn, session_id, &summary) {
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
            anyhow::bail!("failed to initialize agent '{}': {}", agent.name, resp.message);
        }
    }

    println!();
    println!("  Moneypenny v{}", env!("CARGO_PKG_VERSION"));
    println!();
    println!("  Creating project in {}", config.data_dir.display());
    println!();
    println!("  \u{2713} Created {config_path}");
    println!("  \u{2713} Created data directory");
    println!("  \u{2713} Created models directory");
    for agent in &config.agents {
        println!("  \u{2713} Initialized agent \"{}\"", agent.name);
        println!("      LLM:       {} ({})",
            agent.llm.provider,
            agent.llm.model.as_deref().unwrap_or("default"));
        println!("      Embedding: {} ({}, {}D)",
            agent.embedding.provider,
            agent.embedding.model,
            agent.embedding.dimensions);
    }
    println!();
    println!("  Ready. Run `mp start` to begin.");
    println!();

    Ok(())
}

// =========================================================================
// Start / Stop
// =========================================================================

async fn cmd_start(config: &Config, config_path: &Path) -> Result<()> {
    println!();
    println!("  Moneypenny v{}", env!("CARGO_PKG_VERSION"));
    println!();

    let shutdown = tokio::sync::broadcast::channel::<()>(1).0;

    // Spawn one worker subprocess per agent and register each in the WorkerBus
    let bus = WorkerBus::new();
    let mut workers: Vec<WorkerHandle> = Vec::new();
    for agent in &config.agents {
        let (handle, w_stdin, w_stdout) = spawn_worker(config, config_path, &agent.name)?;
        println!("  Worker \"{}\" started (pid {})", agent.name, handle.pid);
        bus.register(agent.name.clone(), w_stdin, w_stdout).await;
        workers.push(handle);
    }

    // Spawn the scheduler loop
    let sched_config = config.clone();
    let mut sched_shutdown = shutdown.subscribe();
    let scheduler_handle = tokio::spawn(async move {
        run_scheduler(&sched_config, &mut sched_shutdown).await
    });

    // Build the shared dispatcher used by all channel adapters.
    // It routes (agent, message, session_id) through the WorkerBus.
    let bus_for_dispatch = Arc::clone(&bus);
    let dispatch: adapters::DispatchFn = Arc::new(move |agent, message, session_id| {
        let bus = Arc::clone(&bus_for_dispatch);
        Box::pin(async move {
            bus.route_full(&agent, &message, session_id.as_deref()).await
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
                Err(e) => return Ok(sidecar_error_response("http_ops_execute_error", e.to_string())),
            };

            Ok(
                serde_json::to_value(resp)
                    .unwrap_or_else(|e| sidecar_error_response("serialization_error", e.to_string())),
            )
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
        let default_agent = config.agents.first()
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
        let default_agent = config.agents.first()
            .map(|a| a.name.clone())
            .unwrap_or_else(|| "main".into());
        let dispatch_clone = Arc::clone(&dispatch);
        let tg_shutdown = shutdown.subscribe();
        tokio::spawn(async move {
            adapters::run_telegram_polling(tg_cfg, default_agent, dispatch_clone, tg_shutdown).await;
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
                        Err(e) => { tracing::warn!("sync: cannot open {agent_name}: {e}"); continue; }
                    };
                    if let Err(e) = mp_ext::init_all_extensions(&conn) {
                        tracing::warn!("sync: ext init for {agent_name}: {e}");
                        continue;
                    }
                    let _ = mp_core::sync::init_sync_tables(&conn, &tables);
                    for peer in &sync_config.peers {
                        let peer_path = if std::path::Path::new(peer).is_absolute() || peer.ends_with(".db") {
                            std::path::PathBuf::from(peer)
                        } else {
                            sync_data_dir.join(format!("{peer}.db"))
                        };
                        if !peer_path.exists() { continue; }
                        let peer_conn = match rusqlite::Connection::open(&peer_path)
                            .and_then(|c| { mp_ext::init_all_extensions(&c).ok(); Ok(c) })
                        {
                            Ok(c) => c,
                            Err(e) => { tracing::warn!("auto-sync: cannot open peer {peer}: {e}"); continue; }
                        };
                        let _ = mp_core::sync::init_sync_tables(&peer_conn, &tables);
                        match mp_core::sync::local_sync_bidirectional(&conn, &peer_conn, &tables) {
                            Ok(r) => tracing::debug!(agent = %agent_name, peer = %peer, sent = r.sent, received = r.received, "auto-sync"),
                            Err(e) => tracing::warn!(agent = %agent_name, peer = %peer, "auto-sync error: {e}"),
                        }
                    }
                    if let Some(ref url) = sync_config.cloud_url {
                        match mp_core::sync::cloud_sync(&conn, url) {
                            Ok(r) => tracing::debug!(agent = %agent_name, batches = r.sent, "cloud auto-sync"),
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

    println!();
    println!("  Gateway ready. {} agent(s) running.", config.agents.len());
    if has_http_channel {
        let port = config.channels.http.as_ref().map(|h| h.port).unwrap_or(8080);
        println!("  HTTP API listening on port {port}  (POST /v1/chat, POST /v1/ops, WS /v1/ws, GET /health)");
    }
    if config.channels.slack.is_some() {
        println!("  Slack Events API endpoint: POST /slack/events");
    }
    if config.channels.discord.is_some() {
        println!("  Discord Interactions endpoint: POST /discord/interactions");
    }
    if config.channels.telegram.is_some() {
        println!("  Telegram long-polling active");
    }
    if has_sync {
        println!("  Auto-sync every {}s ({} peer(s){})", config.sync.interval_secs,
            config.sync.peers.len(),
            if config.sync.cloud_url.is_some() { " + cloud" } else { "" });
    }
    println!("  Press Ctrl-C to shut down.");
    println!();

    // If CLI channel is enabled, run interactive chat on the default agent
    if config.channels.cli {
        let default_agent = config.agents.first()
            .map(|a| a.name.clone())
            .unwrap_or_else(|| "main".into());
        let ag = resolve_agent(config, Some(&default_agent))?;
        let conn = open_agent_db(config, &ag.name)?;
        let provider = build_provider(ag)?;
        let embed = build_embedding_provider(config, ag).ok();
        let sid = mp_core::store::log::create_session(&conn, &ag.name, Some("cli"))?;

        println!("  CLI channel active — agent: {}", ag.name);
        println!("  Type /help for commands, Ctrl-C to shut down.");
        println!();

        let mut shutdown_rx = shutdown.subscribe();
        let stdin = tokio::io::stdin();
        let mut reader = tokio::io::BufReader::new(stdin);

        loop {
            print!("  > ");
            std::io::stdout().flush()?;

            let mut line = String::new();
            let read = tokio::select! {
                r = tokio::io::AsyncBufReadExt::read_line(&mut reader, &mut line) => r?,
                _ = shutdown_rx.recv() => break,
            };

            if read == 0 { break; }
            let trimmed = line.trim();
            if trimmed.is_empty() { continue; }
            if trimmed == "/quit" || trimmed == "/exit" { break; }

            if trimmed == "/help" {
                println!("  /facts    — list stored facts");
                println!("  /scratch  — list scratch entries");
                println!("  /quit     — exit");
                println!();
                continue;
            }
            if trimmed == "/facts" {
                let facts = mp_core::store::facts::list_active(&conn, &ag.name)?;
                if facts.is_empty() {
                    println!("  No facts stored.");
                } else {
                    for f in &facts { println!("  [{:.1}] {}", f.confidence, f.pointer); }
                }
                println!();
                continue;
            }

            match agent_turn(&conn, provider.as_ref(), embed.as_deref(), &ag.name, &sid, ag.persona.as_deref(), trimmed, ag.policy_mode(), Some(&bus)).await {
                Ok(response) => {
                    println!();
                    for l in response.lines() { println!("  {l}"); }
                    println!();
                    if let Ok(n) = extract_facts(&conn, provider.as_ref(), &ag.name, &sid).await {
                        if n > 0 { println!("  ({n} fact{} learned)\n", if n == 1 { "" } else { "s" }); }
                    }
                    if let Some(ref ep) = embed {
                        embed_pending(&conn, ep.as_ref(), &ag.name).await;
                    }
                    maybe_summarize_session(&conn, provider.as_ref(), &sid).await;
                }
                Err(e) => { eprintln!("  Error: {e}\n"); }
            }
        }
    } else {
        // No CLI channel — just wait for Ctrl-C
        tokio::signal::ctrl_c().await?;
    }

    // Graceful shutdown
    println!("\n  Shutting down...");
    let _ = shutdown.send(());
    scheduler_handle.abort();

    for mut w in workers {
        w.shutdown().await;
    }

    let _ = std::fs::remove_file(&pid_path);
    println!("  Goodbye.");
    Ok(())
}

async fn cmd_stop(config: &Config) -> Result<()> {
    let pid_path = config.data_dir.join("mp.pid");
    if !pid_path.exists() {
        println!("  No running gateway found (no PID file at {}).", pid_path.display());
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
        ch.insert(agent_name, WorkerChannel {
            stdin,
            stdout: tokio::io::BufReader::new(stdout),
        });
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
        let ch = channels.get_mut(target)
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
) -> Result<(WorkerHandle, tokio::process::ChildStdin, tokio::process::ChildStdout)> {
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
    let stdin = child.stdin.take()
        .ok_or_else(|| anyhow::anyhow!("worker process has no stdin pipe"))?;
    let stdout = child.stdout.take()
        .ok_or_else(|| anyhow::anyhow!("worker process has no stdout pipe"))?;

    Ok((WorkerHandle { pid, agent_name: agent_name.to_string(), child }, stdin, stdout))
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
                tokio::io::AsyncWriteExt::write_all(&mut stdout, format!("{err}\n").as_bytes()).await?;
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
            &conn, provider.as_ref(), embed.as_deref(), &agent.name, &sid,
            agent.persona.as_deref(), msg, agent.policy_mode(), None,
        ).await {
            Ok(r) => serde_json::json!({"response": r, "session_id": sid}),
            Err(e) => serde_json::json!({"error": e.to_string(), "session_id": sid}),
        };

        tokio::io::AsyncWriteExt::write_all(&mut stdout, format!("{response}\n").as_bytes()).await?;
        tokio::io::AsyncWriteExt::flush(&mut stdout).await?;

        // Post-response extraction and rolling summarization
        let _ = extract_facts(&conn, provider.as_ref(), &agent.name, &sid).await;
        if let Some(ref ep) = embed {
            embed_pending(&conn, ep.as_ref(), &agent.name).await;
        }
        maybe_summarize_session(&conn, provider.as_ref(), &sid).await;
    }

    tracing::info!(agent = agent_name, "worker exiting");
    Ok(())
}

fn resolve_or_create_session(
    conn: &rusqlite::Connection,
    agent_name: &str,
    channel: Option<&str>,
    requested_session_id: Option<String>,
) -> Result<String> {
    if let Some(sid) = requested_session_id {
        let exists: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sessions WHERE id = ?1 AND agent_id = ?2",
            rusqlite::params![sid, agent_name],
            |r| r.get(0),
        )?;
        if exists > 0 {
            return Ok(sid);
        }
        anyhow::bail!(
            "Session '{sid}' not found for agent '{agent_name}'. Use /session in chat to copy a valid ID, or omit --session-id to create a new session."
        );
    }
    mp_core::store::log::create_session(conn, agent_name, channel)
}

// =========================================================================
// Scheduler
// =========================================================================

async fn run_scheduler(
    config: &Config,
    shutdown: &mut tokio::sync::broadcast::Receiver<()>,
) {
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
            let due_jobs = match mp_core::scheduler::poll_due_jobs(&conn, now) {
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
            println!("  {:20} {:15} {:15} {:10}", "NAME", "TRUST", "LLM", "SOURCE");
            println!("  {:20} {:15} {:15} {:10}", "----", "-----", "---", "------");
            let mut listed: std::collections::HashSet<String> = std::collections::HashSet::new();
            for agent in &config.agents {
                println!("  {:20} {:15} {:15} {:10}",
                    agent.name, agent.trust_level, agent.llm.provider, "config");
                listed.insert(agent.name.clone());
            }
            let meta_path = config.metadata_db_path();
            if meta_path.exists() {
                if let Ok(meta_conn) = mp_core::db::open(&meta_path) {
                    if let Ok(db_agents) = mp_core::gateway::list_agents(&meta_conn) {
                        for a in db_agents {
                            if !listed.contains(&a.name) {
                                println!("  {:20} {:15} {:15} {:10}",
                                    a.name, a.trust_level, a.llm_provider, "runtime");
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
            println!("  Agent {} created.", resp.data["name"].as_str().unwrap_or("-"));
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
            println!("  Agent {} deleted.", resp.data["name"].as_str().unwrap_or("-"));
        }
        cli::AgentCommand::Status { name } => {
            let agent = resolve_agent(config, name.as_deref())?;
            let conn = open_agent_db(config, &agent.name)?;

            let fact_count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM facts WHERE superseded_at IS NULL", [], |r| r.get(0)
            ).unwrap_or(0);
            let session_count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM sessions", [], |r| r.get(0)
            ).unwrap_or(0);
            let doc_count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM documents", [], |r| r.get(0)
            ).unwrap_or(0);
            let skill_count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM skills", [], |r| r.get(0)
            ).unwrap_or(0);

            println!();
            println!("  Agent: {}", agent.name);
            println!("  Trust: {}", agent.trust_level);
            println!("  LLM:       {} ({})", agent.llm.provider, agent.llm.model.as_deref().unwrap_or("default"));
            println!("  Embedding: {} ({}, {}D)", agent.embedding.provider, agent.embedding.model, agent.embedding.dimensions);
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

async fn cmd_chat(config: &Config, agent: Option<String>, session_id: Option<String>) -> Result<()> {
    let ag = resolve_agent(config, agent.as_deref())?;
    let conn = open_agent_db(config, &ag.name)?;
    let provider = build_provider(ag)?;
    let embed = build_embedding_provider(config, ag).ok();
    let sid = resolve_or_create_session(&conn, &ag.name, Some("cli"), session_id)?;

    println!();
    println!("  Moneypenny v{} — agent: {}", env!("CARGO_PKG_VERSION"), ag.name);
    println!("  LLM:       {} ({})", ag.llm.provider, ag.llm.model.as_deref().unwrap_or("default"));
    println!("  Embedding: {} ({}, {}D)", ag.embedding.provider, ag.embedding.model, ag.embedding.dimensions);
    println!("  Type /help for commands, Ctrl-C to exit.");
    println!();

    let stdin = std::io::stdin();
    loop {
        print!("  > ");
        std::io::stdout().flush()?;

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
                println!("  /facts    — list stored facts");
                println!("  /scratch  — list scratch entries");
                println!("  /session  — show session info");
                println!("  /quit     — exit chat");
                println!();
                continue;
            }
            "/facts" => {
                let facts = mp_core::store::facts::list_active(&conn, &ag.name)?;
                if facts.is_empty() {
                    println!("  No facts stored.");
                } else {
                    for f in &facts {
                        println!("  [{:.1}] {}", f.confidence, f.pointer);
                    }
                }
                println!();
                continue;
            }
            "/scratch" => {
                let entries = mp_core::store::scratch::list(&conn, &sid)?;
                if entries.is_empty() {
                    println!("  Scratch is empty.");
                } else {
                    for e in &entries {
                        let preview: String = e.content.chars().take(60).collect();
                        println!("  [{}] {}", e.key, preview);
                    }
                }
                println!();
                continue;
            }
            "/session" => {
                let msgs = mp_core::store::log::get_messages(&conn, &sid)?;
                println!("  Session: {sid}");
                println!("  Messages: {}", msgs.len());
                println!();
                continue;
            }
            _ => {}
        }

        match agent_turn(&conn, provider.as_ref(), embed.as_deref(), &ag.name, &sid, ag.persona.as_deref(), line, ag.policy_mode(), None).await {
            Ok(response) => {
                println!();
                for resp_line in response.lines() {
                    println!("  {resp_line}");
                }
                println!();

                match extract_facts(&conn, provider.as_ref(), &ag.name, &sid).await {
                    Ok(n) if n > 0 => {
                        println!("  ({n} fact{} learned)", if n == 1 { "" } else { "s" });
                        println!();
                    }
                    Err(e) => tracing::debug!("extraction error: {e}"),
                    _ => {}
                }
                if let Some(ref ep) = embed {
                    embed_pending(&conn, ep.as_ref(), &ag.name).await;
                }
                maybe_summarize_session(&conn, provider.as_ref(), &sid).await;
            }
            Err(e) => {
                eprintln!("  Error: {e}");
                eprintln!();
            }
        }
    }

    println!("  Session {sid} ended.");
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
    let sid = resolve_or_create_session(&conn, &agent.name, Some("cli"), session_id)?;

    let response = agent_turn(
        &conn, provider.as_ref(), embed.as_deref(), &agent.name, &sid,
        agent.persona.as_deref(), message, agent.policy_mode(), None,
    ).await?;

    println!();
    for line in response.lines() {
        println!("  {line}");
    }
    println!();

    if let Ok(n) = extract_facts(&conn, provider.as_ref(), &agent.name, &sid).await {
        if n > 0 {
            println!("  ({n} fact{} learned)", if n == 1 { "" } else { "s" });
            println!();
        }
    }
    if let Some(ref ep) = embed {
        embed_pending(&conn, ep.as_ref(), &agent.name).await;
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

            println!();
            if facts.is_empty() {
                println!("  No facts found for agent \"{}\".", ag.name);
            } else {
                println!("  {:36} {:6} {:6} {:50}", "ID", "CONF", "CMPCT", "POINTER");
                println!("  {:36} {:6} {:6} {:50}", "--", "----", "-----", "-------");
                for f in &facts {
                    println!("  {:36} {:<6.1} {:<6} {}", f.id, f.confidence, f.compaction_level, f.pointer);
                }
                println!();
                println!("  {} active facts", facts.len());
            }
            println!();
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
                println!("  Memory search denied: {}", resp.message);
                return Ok(());
            }
            let results = resp.data.as_array().cloned().unwrap_or_default();

            println!();
            if results.is_empty() {
                println!("  No results for \"{query}\".");
            } else {
                for r in &results {
                    let preview: String = r["content"].as_str().unwrap_or("").chars().take(80).collect();
                    println!(
                        "  [{}] {:.4}  {}",
                        r["store"].as_str().unwrap_or("-"),
                        r["score"].as_f64().unwrap_or(0.0),
                        preview
                    );
                }
                println!();
                println!("  {} results", results.len());
            }
            println!();
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
                println!("  {}", resp.message);
                return Ok(());
            }

            println!();
            println!("  ID:         {}", resp.data["id"].as_str().unwrap_or("-"));
            println!("  Pointer:    {}", resp.data["pointer"].as_str().unwrap_or("-"));
            println!("  Summary:    {}", resp.data["summary"].as_str().unwrap_or("-"));
            println!("  Confidence: {:.1}", resp.data["confidence"].as_f64().unwrap_or(0.0));
            println!("  Version:    {}", resp.data["version"].as_i64().unwrap_or(1));
            println!("  Compact Lv: {}", resp.data["compaction_level"].as_i64().unwrap_or(0));
            if let Some(compact) = resp.data["context_compact"].as_str() {
                println!("  Compact:    {}", compact);
            }
            println!();
            println!("  Content:");
            println!("  {}", resp.data["content"].as_str().unwrap_or(""));
            println!();

            let audit = mp_core::store::facts::get_audit(&conn, &id)?;
            if !audit.is_empty() {
                println!("  Audit trail:");
                for a in &audit {
                    println!("    {} — {}", a.operation, a.reason.as_deref().unwrap_or(""));
                }
            }
            println!();
        }
        cli::FactsCommand::Expand { id } => {
            let ag = resolve_agent(config, None)?;
            let conn = open_agent_db(config, &ag.name)?;
            let req = op_request(
                &ag.name,
                "memory.fact.get",
                serde_json::json!({ "id": id }),
            );
            let resp = mp_core::operations::execute(&conn, &req)?;
            if !resp.ok {
                println!("  {}", resp.message);
                return Ok(());
            }
            println!();
            println!("  [{}] {}", resp.data["id"].as_str().unwrap_or("-"), resp.data["pointer"].as_str().unwrap_or("-"));
            println!("  Full content:");
            println!("  {}", resp.data["content"].as_str().unwrap_or(""));
            println!();
        }
        cli::FactsCommand::ResetCompaction { id, all, agent, confirm } => {
            let ag = resolve_agent(config, agent.as_deref())?;
            let conn = open_agent_db(config, &ag.name)?;

            if all {
                if !confirm {
                    println!("  Use --confirm with --all to reset compaction for every active fact.");
                    return Ok(());
                }
                let facts = mp_core::store::facts::list_active(&conn, &ag.name)?;
                if facts.is_empty() {
                    println!("  No active facts found for agent \"{}\".", ag.name);
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
                println!("  Reset compaction for {reset_count}/{} facts.", facts.len());
                return Ok(());
            }

            let fact_id = match id {
                Some(v) => v,
                None => {
                    println!("  Provide a fact ID, or use --all --confirm.");
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
                println!("  Fact reset failed: {}", resp.message);
                return Ok(());
            }
            println!("  Fact {} compaction reset.", resp.data["id"].as_str().unwrap_or("-"));
        }
        cli::FactsCommand::Promote { id, scope } => {
            println!("  [mp facts promote {id} --scope {scope} — requires sync (M13)]");
        }
        cli::FactsCommand::Delete { id, confirm } => {
            let ag = resolve_agent(config, None)?;
            let conn = open_agent_db(config, &ag.name)?;

            if !confirm {
                println!("  Use --confirm to delete fact {id}");
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
                    println!("  Fact delete failed: {}", resp.message);
                    return Ok(());
                }
                println!("  Fact {} deleted.", resp.data["id"].as_str().unwrap_or("-"));
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
        println!();
        if rows.is_empty() {
            println!("  No ingest runs found.");
        } else {
            println!("  {:36} {:10} {:22} {:8} {:8} {:8} {:8} {:8}", "RUN_ID", "SOURCE", "STATUS", "PROC", "INS", "DEDUP", "PROJ", "ERR");
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
        println!();
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
            println!(
                "  Replay preview {}: processed={}, would_insert={}, would_dedupe={}, parse_errors={}, lines={}..{} (use --apply to execute)",
                resp.data["run_id"].as_str().unwrap_or("-"),
                resp.data["processed_count"].as_i64().unwrap_or(0),
                resp.data["would_insert_count"].as_i64().unwrap_or(0),
                resp.data["would_dedupe_count"].as_i64().unwrap_or(0),
                resp.data["parse_error_count"].as_i64().unwrap_or(0),
                resp.data["from_line"].as_i64().unwrap_or(0),
                resp.data["to_line"].as_i64().unwrap_or(0),
            );
        } else {
            println!(
                "  Replay run {}: processed={}, inserted={}, deduped={}, projected={}, errors={}",
                resp.data["run_id"].as_str().unwrap_or("-"),
                resp.data["processed_count"].as_i64().unwrap_or(0),
                resp.data["inserted_count"].as_i64().unwrap_or(0),
                resp.data["deduped_count"].as_i64().unwrap_or(0),
                resp.data["projected_count"].as_i64().unwrap_or(0),
                resp.data["error_count"].as_i64().unwrap_or(0),
            );
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
        println!(
            "  Ingest run {}: processed={}, inserted={}, deduped={}, projected={}, errors={}",
            resp.data["run_id"].as_str().unwrap_or("-"),
            resp.data["processed_count"].as_i64().unwrap_or(0),
            resp.data["inserted_count"].as_i64().unwrap_or(0),
            resp.data["deduped_count"].as_i64().unwrap_or(0),
            resp.data["projected_count"].as_i64().unwrap_or(0),
            resp.data["error_count"].as_i64().unwrap_or(0),
        );
    } else if let Some(p) = path {
        let content = std::fs::read_to_string(&p)?;
        let title = Path::new(&p).file_name()
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
        println!("  Ingested {p}: {chunks} chunks (doc {doc_id})");
        // Embed new chunks in the background; fails gracefully if model is missing.
        if let Ok(ep) = build_embedding_provider(config, ag) {
            embed_pending(&conn, ep.as_ref(), &ag.name).await;
        }
    } else if let Some(u) = url {
        println!("  Fetching {u} …");
        let response = reqwest::get(&u).await
            .map_err(|e| anyhow::anyhow!("HTTP fetch failed for {u}: {e}"))?;
        let status_code = response.status();
        if !status_code.is_success() {
            anyhow::bail!("HTTP {status_code} for {u}");
        }
        let content_type = response.headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        let body = response.text().await
            .map_err(|e| anyhow::anyhow!("failed to read response body from {u}: {e}"))?;

        let is_html = content_type.contains("text/html");
        let title = if is_html {
            extract_html_title(&body)
        } else {
            None
        }.unwrap_or_else(|| {
            u.rsplit('/').find(|s| !s.is_empty())
                .unwrap_or(&u)
                .to_string()
        });

        let content = if is_html {
            strip_html_tags(&body)
        } else {
            body
        };

        if content.trim().is_empty() {
            anyhow::bail!("fetched URL returned empty content: {u}");
        }

        let req = op_request(
            &ag.name,
            "knowledge.ingest",
            serde_json::json!({
                "path": u,
                "title": title,
                "content": content,
                "metadata": format!("{{\"source_url\":\"{u}\",\"content_type\":\"{content_type}\"}}"),
            }),
        );
        let resp = mp_core::operations::execute(&conn, &req)?;
        if !resp.ok {
            anyhow::bail!("ingest denied: {}", resp.message);
        }
        let doc_id = resp.data["document_id"].as_str().unwrap_or("-");
        let chunks = resp.data["chunks_created"].as_u64().unwrap_or(0);
        println!("  Ingested {u}: {chunks} chunks (doc {doc_id})");
        if let Ok(ep) = build_embedding_provider(config, ag) {
            embed_pending(&conn, ep.as_ref(), &ag.name).await;
        }
    } else {
        anyhow::bail!("Provide a path, --openclaw-file, or --url to ingest.");
    }
    Ok(())
}

/// Minimal HTML tag stripper that collapses whitespace and drops script/style blocks.
fn strip_html_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len() / 2);
    let mut in_tag = false;
    let mut in_skip_block = false;
    let mut tag_buf = String::new();

    let mut chars = html.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '<' {
            in_tag = true;
            tag_buf.clear();
            continue;
        }
        if in_tag {
            if ch == '>' {
                in_tag = false;
                let tag_lower = tag_buf.to_ascii_lowercase();
                let tag_name = tag_lower.split_whitespace().next().unwrap_or("");
                if tag_name == "script" || tag_name == "style" || tag_name == "noscript" {
                    in_skip_block = true;
                } else if tag_name == "/script" || tag_name == "/style" || tag_name == "/noscript" {
                    in_skip_block = false;
                }
                if matches!(tag_name, "br" | "br/" | "p" | "/p" | "div" | "/div"
                    | "h1" | "/h1" | "h2" | "/h2" | "h3" | "/h3"
                    | "h4" | "/h4" | "h5" | "/h5" | "h6" | "/h6"
                    | "li" | "/li" | "tr" | "/tr" | "blockquote" | "/blockquote") {
                    out.push('\n');
                }
            } else {
                tag_buf.push(ch);
            }
            continue;
        }
        if in_skip_block {
            continue;
        }
        if ch == '&' {
            let mut entity = String::new();
            while let Some(&next) = chars.peek() {
                if next == ';' || entity.len() > 8 {
                    chars.next();
                    break;
                }
                entity.push(next);
                chars.next();
            }
            match entity.as_str() {
                "amp" => out.push('&'),
                "lt" => out.push('<'),
                "gt" => out.push('>'),
                "quot" => out.push('"'),
                "apos" => out.push('\''),
                "nbsp" => out.push(' '),
                _ => { out.push(' '); }
            }
            continue;
        }
        out.push(ch);
    }

    collapse_whitespace(&out)
}

fn collapse_whitespace(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut prev_blank = false;
    for line in s.lines() {
        let trimmed = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if trimmed.is_empty() {
            if !prev_blank && !result.is_empty() {
                result.push('\n');
                prev_blank = true;
            }
        } else {
            result.push_str(&trimmed);
            result.push('\n');
            prev_blank = false;
        }
    }
    result.trim().to_string()
}

/// Extract the <title> text from an HTML document.
fn extract_html_title(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let start = lower.find("<title")?.checked_add(6)?;
    let after_tag = lower[start..].find('>')?.checked_add(1)?;
    let content_start = start + after_tag;
    let end = lower[content_start..].find("</title")?;
    let title = html[content_start..content_start + end].trim().to_string();
    if title.is_empty() { None } else { Some(title) }
}

fn op_request(agent_id: &str, op: &str, args: serde_json::Value) -> mp_core::operations::OperationRequest {
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

fn canonical_operation_catalog() -> &'static [(&'static str, &'static str)] {
    &[
        ("job.create", "Create a scheduled job"),
        ("job.list", "List scheduled jobs"),
        ("job.run", "Run a job immediately"),
        ("job.pause", "Pause a scheduled job"),
        ("job.history", "List job run history"),
        ("job.spec.plan", "Plan an agent-generated job spec"),
        ("job.spec.confirm", "Confirm a planned job spec"),
        ("job.spec.apply", "Apply a confirmed job spec into jobs"),
        ("policy.add", "Add a policy rule"),
        ("policy.evaluate", "Evaluate policy decision"),
        ("policy.explain", "Explain policy decision"),
        ("knowledge.ingest", "Ingest knowledge content"),
        ("memory.search", "Search memory across stores"),
        ("memory.fact.add", "Create a fact"),
        ("memory.fact.update", "Update a fact"),
        ("memory.fact.get", "Get full fact content"),
        ("memory.fact.compaction.reset", "Reset fact compaction state"),
        ("skill.add", "Add a skill"),
        ("skill.promote", "Promote a skill"),
        ("fact.delete", "Delete a fact"),
        ("audit.query", "Query audit records"),
        ("audit.append", "Append an audit record"),
        ("session.resolve", "Resolve or create session"),
        ("session.list", "List sessions"),
        ("js.tool.add", "Add JavaScript tool"),
        ("js.tool.list", "List JavaScript tools"),
        ("js.tool.delete", "Delete JavaScript tool"),
        ("agent.create", "Create an agent"),
        ("agent.delete", "Delete an agent"),
        ("agent.config", "Update agent config"),
        ("ingest.events", "Ingest external events"),
        ("ingest.status", "List ingest runs"),
        ("ingest.replay", "Replay an ingest run"),
    ]
}

fn mcp_tools_list_result() -> serde_json::Value {
    let tools: Vec<serde_json::Value> = canonical_operation_catalog()
        .iter()
        .map(|(name, description)| {
            serde_json::json!({
                "name": name,
                "description": description,
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "args": { "type": "object", "default": {} },
                        "request_id": { "type": "string" },
                        "idempotency_key": { "type": "string" },
                        "agent_id": { "type": "string" },
                        "tenant_id": { "type": "string" },
                        "user_id": { "type": "string" },
                        "channel": { "type": "string" },
                        "session_id": { "type": "string" },
                        "trace_id": { "type": "string" }
                    },
                    "additionalProperties": true
                }
            })
        })
        .collect();
    serde_json::json!({ "tools": tools })
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

fn build_sidecar_request_from_mcp_call(
    input: &serde_json::Value,
    default_agent_id: &str,
) -> anyhow::Result<mp_core::operations::OperationRequest> {
    let params = input
        .get("params")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| anyhow::anyhow!("missing object params for tools/call"))?;
    let op = params
        .get("name")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("missing params.name"))?;
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    let request_id = params
        .get("request_id")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .or_else(|| request_id_from_jsonrpc(input));

    build_sidecar_request(
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
    )
}

fn handle_sidecar_mcp_request(
    conn: &rusqlite::Connection,
    input: &serde_json::Value,
    default_agent_id: &str,
) -> anyhow::Result<Option<serde_json::Value>> {
    let method = match input.get("method").and_then(serde_json::Value::as_str) {
        Some(m) => m,
        None => return Ok(None),
    };
    let id = input.get("id").cloned();
    if method == "notifications/initialized" && id.is_none() {
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
            let req = match build_sidecar_request_from_mcp_call(input, default_agent_id) {
                Ok(r) => r,
                Err(e) => {
                    return Ok(Some(jsonrpc_error(
                        id,
                        -32602,
                        format!("invalid tools/call params: {e}"),
                    )));
                }
            };
            let op_resp = match mp_core::operations::execute(conn, &req) {
                Ok(resp) => resp,
                Err(e) => {
                    let err = sidecar_error_response("sidecar_execute_error", e.to_string());
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
        unknown if unknown.starts_with("notifications/") && id.is_none() => return Ok(None),
        _ => jsonrpc_error(id, -32601, format!("method not found: {method}")),
    };

    Ok(Some(response))
}

fn build_sidecar_request(
    input: serde_json::Value,
    default_agent_id: &str,
) -> anyhow::Result<mp_core::operations::OperationRequest> {
    if let Ok(req) = serde_json::from_value::<mp_core::operations::OperationRequest>(input.clone()) {
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

    let stdin = tokio::io::stdin();
    let reader = tokio::io::BufReader::new(stdin);
    let mut lines = tokio::io::AsyncBufReadExt::lines(reader);
    let mut stdout = tokio::io::stdout();

    while let Ok(Some(line)) = lines.next_line().await {
        let parsed: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                let err = sidecar_error_response("invalid_json", e.to_string());
                tokio::io::AsyncWriteExt::write_all(&mut stdout, format!("{err}\n").as_bytes()).await?;
                tokio::io::AsyncWriteExt::flush(&mut stdout).await?;
                continue;
            }
        };

        if let Some(mcp_response) = handle_sidecar_mcp_request(&conn, &parsed, &ag.name)? {
            tokio::io::AsyncWriteExt::write_all(&mut stdout, format!("{mcp_response}\n").as_bytes()).await?;
            tokio::io::AsyncWriteExt::flush(&mut stdout).await?;
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

        let response = match mp_core::operations::execute(&conn, &request) {
            Ok(resp) => serde_json::to_value(resp)
                .unwrap_or_else(|e| sidecar_error_response("serialization_error", e.to_string())),
            Err(e) => sidecar_error_response("sidecar_execute_error", e.to_string()),
        };

        tokio::io::AsyncWriteExt::write_all(&mut stdout, format!("{response}\n").as_bytes()).await?;
        tokio::io::AsyncWriteExt::flush(&mut stdout).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{build_sidecar_request, build_sidecar_request_from_mcp_call, mcp_tools_list_result};

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
        let req = build_sidecar_request_from_mcp_call(
            &serde_json::json!({
                "jsonrpc": "2.0",
                "id": "rpc-42",
                "method": "tools/call",
                "params": {
                    "name": "ingest.status",
                    "arguments": { "limit": 5 },
                    "agent_id": "main"
                }
            }),
            "default-agent",
        )
        .expect("translate tools/call to operation request");
        assert_eq!(req.op, "ingest.status");
        assert_eq!(req.request_id.as_deref(), Some("rpc-42"));
        assert_eq!(req.actor.agent_id, "main");
        assert_eq!(req.args["limit"], 5);
    }

    #[test]
    fn sidecar_mcp_tools_list_exposes_canonical_ops() {
        let result = mcp_tools_list_result();
        let tools = result["tools"].as_array().cloned().unwrap_or_default();
        assert!(!tools.is_empty());
        assert!(tools.iter().any(|t| t["name"] == "job.create"));
        assert!(tools.iter().any(|t| t["name"] == "ingest.replay"));
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
            println!();
            if results.is_empty() {
                println!("  No knowledge results for \"{query}\".");
            } else {
                for (id, content, _score) in &results {
                    let preview: String = content.chars().take(80).collect();
                    println!("  {id}: {preview}");
                }
            }
            println!();
        }
        cli::KnowledgeCommand::List => {
            let docs = mp_core::store::knowledge::list_documents(&conn)?;
            println!();
            if docs.is_empty() {
                println!("  No documents ingested.");
            } else {
                println!("  {:36} {:30} {:20}", "ID", "TITLE", "PATH");
                println!("  {:36} {:30} {:20}", "--", "-----", "----");
                for d in &docs {
                    println!("  {:36} {:30} {:20}",
                        d.id,
                        d.title.as_deref().unwrap_or("-"),
                        d.path.as_deref().unwrap_or("-"),
                    );
                }
            }
            println!();
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
            let name = Path::new(&path).file_stem()
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
                println!("  Skill add denied: {}", resp.message);
                return Ok(());
            }
            let id = resp.data["id"].as_str().unwrap_or("-");
            let printed_name = resp.data["name"].as_str().unwrap_or("skill");
            println!("  Added skill \"{printed_name}\" ({id})");
        }
        cli::SkillCommand::List { .. } => {
            let mut stmt = conn.prepare(
                "SELECT id, name, usage_count, success_rate, promoted FROM skills ORDER BY usage_count DESC"
            )?;
            let skills: Vec<(String, String, i64, Option<f64>, bool)> = stmt.query_map([], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get::<_, i64>(4)? != 0))
            })?.collect::<Result<Vec<_>, _>>()?;

            println!();
            if skills.is_empty() {
                println!("  No skills registered.");
            } else {
                println!("  {:36} {:20} {:6} {:8} {:8}", "ID", "NAME", "USES", "RATE", "PROMO");
                println!("  {:36} {:20} {:6} {:8} {:8}", "--", "----", "----", "----", "-----");
                for (id, name, uses, rate, promoted) in &skills {
                    let rate_str = rate.map(|r| format!("{:.0}%", r * 100.0)).unwrap_or("-".into());
                    println!("  {:36} {:20} {:6} {:8} {:8}",
                        id, name, uses, rate_str, if *promoted { "yes" } else { "" });
                }
            }
            println!();
        }
        cli::SkillCommand::Promote { id } => {
            let req = op_request(
                &ag.name,
                "skill.promote",
                serde_json::json!({ "id": id }),
            );
            let resp = mp_core::operations::execute(&conn, &req)?;
            if !resp.ok {
                println!("  Skill promote failed: {}", resp.message);
                return Ok(());
            }
            println!("  Skill {} promoted.", resp.data["id"].as_str().unwrap_or("-"));
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
            let policies: Vec<(String, String, i64, String, Option<String>, Option<String>, Option<String>, bool)> =
                stmt.query_map([], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?,
                        r.get(4)?, r.get(5)?, r.get(6)?, r.get::<_, i64>(7)? != 0))
                })?.collect::<Result<Vec<_>, _>>()?;

            println!();
            if policies.is_empty() {
                println!("  No policies configured.");
            } else {
                println!("  {:36} {:20} {:4} {:6} {:10} {:10} {:15}",
                    "ID", "NAME", "PRI", "EFFECT", "ACTOR", "ACTION", "RESOURCE");
                for (id, name, pri, effect, actor, action, resource, _) in &policies {
                    println!("  {:36} {:20} {:4} {:6} {:10} {:10} {:15}",
                        id, name, pri, effect,
                        actor.as_deref().unwrap_or("*"),
                        action.as_deref().unwrap_or("*"),
                        resource.as_deref().unwrap_or("*"),
                    );
                }
            }
            println!();
        }
        cli::PolicyCommand::Add { name, effect, priority, actor, action, resource, argument, channel, sql, rule_type, rule_config, message } => {
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
                println!("  Policy add denied: {}", resp.message);
                return Ok(());
            }
            let id = resp.data["id"].as_str().unwrap_or("-");
            let printed_name = resp.data["name"].as_str().unwrap_or("policy");
            let pri = resp.data["priority"].as_i64().unwrap_or(0);
            println!("  Policy \"{printed_name}\" added ({id}, priority={pri})");
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
                println!("  Policy explain denied: {}", resp.message);
                return Ok(());
            }
            println!("  Effect: {}", resp.data["effect"].as_str().unwrap_or("unknown"));
            if let Some(reason) = resp.data["reason"].as_str() {
                println!("  Reason: {reason}");
            }
            if let Some(policy_id) = resp.data["policy_id"].as_str() {
                println!("  Policy ID: {policy_id}");
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
                println!("  Audit query denied: {}", resp.message);
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

            println!();
            if violations.is_empty() {
                println!("  No policy violations in the last {last}.");
            } else {
                for v in &violations {
                    println!("  [{effect}] {actor} → {action} on {resource}: {}",
                        v["reason"].as_str().unwrap_or(""),
                        effect = v["effect"].as_str().unwrap_or(""),
                        actor = v["actor"].as_str().unwrap_or(""),
                        action = v["action"].as_str().unwrap_or(""),
                        resource = v["resource"].as_str().unwrap_or(""),
                    );
                }
            }
            println!();
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
                        eprintln!("  Skipping policy without 'name' field");
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
                        println!("  Loaded policy \"{name}\" ({id})");
                        loaded += 1;
                    }
                    Ok(resp) => {
                        eprintln!("  Failed to load \"{name}\": {}", resp.message);
                        errors += 1;
                    }
                    Err(e) => {
                        eprintln!("  Error loading \"{name}\": {e}");
                        errors += 1;
                    }
                }
            }
            println!();
            println!("  Loaded {loaded} policies ({errors} errors) from {file}");
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
                println!("  Job list denied: {}", resp.message);
                return Ok(());
            }
            let jobs: Vec<serde_json::Value> = serde_json::from_value(resp.data).unwrap_or_default();
            println!();
            if jobs.is_empty() {
                println!("  No jobs scheduled.");
            } else {
                println!("  {:36} {:20} {:8} {:10} {:8}", "ID", "NAME", "TYPE", "STATUS", "SCHED");
                for j in &jobs {
                    println!("  {:36} {:20} {:8} {:10} {:8}",
                        j["id"].as_str().unwrap_or("-"),
                        j["name"].as_str().unwrap_or("-"),
                        j["job_type"].as_str().unwrap_or("-"),
                        j["status"].as_str().unwrap_or("-"),
                        j["schedule"].as_str().unwrap_or("-"));
                }
            }
            println!();
        }
        cli::JobCommand::Create { name, schedule, job_type, payload, agent } => {
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
                println!("  Job create denied: {}", resp.message);
                return Ok(());
            }
            let id = resp.data["id"].as_str().unwrap_or("-");
            let printed_name = resp.data["name"].as_str().unwrap_or("job");
            println!("  Job \"{printed_name}\" created ({id})");
        }
        cli::JobCommand::Run { id } => {
            let req = op_request(&ag.name, "job.run", serde_json::json!({ "id": id }));
            let resp = mp_core::operations::execute(&conn, &req)?;
            if !resp.ok {
                println!("  Job run failed: {}", resp.message);
                return Ok(());
            }
            println!("  Run {}: {}",
                resp.data["run_id"].as_str().unwrap_or("-"),
                resp.data["status"].as_str().unwrap_or("-")
            );
            if let Some(result) = resp.data["result"].as_str() {
                println!("  Result: {result}");
            }
        }
        cli::JobCommand::Pause { id } => {
            let req = op_request(&ag.name, "job.pause", serde_json::json!({ "id": id }));
            let resp = mp_core::operations::execute(&conn, &req)?;
            if !resp.ok {
                println!("  Job pause failed: {}", resp.message);
                return Ok(());
            }
            println!("  Job {} paused.", resp.data["id"].as_str().unwrap_or("-"));
        }
        cli::JobCommand::History { id } => {
            let mut args = serde_json::json!({ "limit": 20 });
            if let Some(ref job_id) = id {
                args["id"] = serde_json::json!(job_id);
            }
            let req = op_request(&ag.name, "job.history", args);
            let resp = mp_core::operations::execute(&conn, &req)?;
            if !resp.ok {
                println!("  Job history denied: {}", resp.message);
                return Ok(());
            }
            let runs = resp.data.as_array().cloned().unwrap_or_default();
            println!();
            if runs.is_empty() {
                println!("  No job runs found.");
            } else {
                for r in &runs {
                    println!("  {}  job:{}  {}  {}",
                        r["id"].as_str().unwrap_or("-"),
                        r["job_id"].as_str().unwrap_or("-"),
                        r["status"].as_str().unwrap_or("-"),
                        r["result"].as_str().unwrap_or("-"),
                    );
                }
            }
            println!();
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
                println!("  Audit query denied: {}", resp.message);
                return Ok(());
            }
            let entries = resp.data.as_array().cloned().unwrap_or_default();

            println!();
            if entries.is_empty() {
                println!("  No audit entries.");
            } else {
                for e in &entries {
                    println!("  [{effect}] {actor} → {action} on {resource}: {}",
                        e["reason"].as_str().unwrap_or(""),
                        effect = e["effect"].as_str().unwrap_or(""),
                        actor = e["actor"].as_str().unwrap_or(""),
                        action = e["action"].as_str().unwrap_or(""),
                        resource = e["resource"].as_str().unwrap_or(""),
                    );
                }
            }
            println!();
        }
        Some(cli::AuditCommand::Search { query }) => {
            let req = op_request(
                &ag.name,
                "audit.query",
                serde_json::json!({
                    "query": query,
                    "limit": 20
                }),
            );
            let resp = mp_core::operations::execute(&conn, &req)?;
            if !resp.ok {
                println!("  Audit query denied: {}", resp.message);
                return Ok(());
            }
            let entries = resp.data.as_array().cloned().unwrap_or_default();

            println!();
            for e in &entries {
                println!("  [{effect}] {actor} → {action} on {resource}: {}",
                    e["reason"].as_str().unwrap_or(""),
                    effect = e["effect"].as_str().unwrap_or(""),
                    actor = e["actor"].as_str().unwrap_or(""),
                    action = e["action"].as_str().unwrap_or(""),
                    resource = e["resource"].as_str().unwrap_or(""),
                );
            }
            println!();
        }
        Some(cli::AuditCommand::Export { format }) => {
            let req = op_request(
                &ag.name,
                "audit.query",
                serde_json::json!({ "limit": 10000 }),
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
                    println!("id,actor,action,resource,effect,reason,session_id,created_at,correlation_id");
                    for e in &entries {
                        println!("{},{},{},{},{},{},{},{},{}",
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
            println!();
            println!("  Sync status for agent \"{}\"", ag.name);
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
                    eprintln!("  Peer DB not found: {}", peer_path.display());
                    continue;
                }
                print!("  Syncing with peer \"{}\"… ", peer);
                std::io::stdout().flush()?;
                let peer_conn = match open_peer_db(&peer_path, &sync_tables) {
                    Ok(c) => c,
                    Err(e) => { eprintln!("error opening peer: {e}"); continue; }
                };
                match mp_core::sync::local_sync_bidirectional(&conn, &peer_conn, &sync_tables) {
                    Ok(r) => {
                        println!("sent {}B, received {}B", r.sent, r.received);
                        total_sent += r.sent;
                        total_received += r.received;
                    }
                    Err(e) => eprintln!("error: {e}"),
                }
            }

            // Cloud sync
            if let Some(ref url) = config.sync.cloud_url {
                print!("  Cloud sync… ");
                std::io::stdout().flush()?;
                match mp_core::sync::cloud_sync(&conn, url) {
                    Ok(r) => {
                        println!("{} batch(es)", r.sent);
                        total_sent += r.sent;
                    }
                    Err(e) => eprintln!("error: {e}"),
                }
            }

            if config.sync.peers.is_empty() && config.sync.cloud_url.is_none() {
                println!("  No peers or cloud URL configured.");
                println!("  Add [sync] peers = [\"other-agent\"] or cloud_url = \"…\" to moneypenny.toml");
            } else {
                println!();
                println!("  Sync complete. Sent {}B, received {}B.", total_sent, total_received);
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
            std::io::stdout().flush()?;
            let peer_conn = open_peer_db(&peer_path, &sync_tables)?;
            let r = mp_core::sync::local_sync_push(&conn, &peer_conn, &sync_tables)?;
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
            std::io::stdout().flush()?;
            let peer_conn = open_peer_db(&peer_path, &sync_tables)?;
            let r = mp_core::sync::local_sync_pull(&conn, &peer_conn, &sync_tables)?;
            println!("received {}B", r.received);
        }

        // ------------------------------------------------------------------
        // Connect — store cloud URL in the live config file
        // ------------------------------------------------------------------
        cli::SyncCommand::Connect { url, agent: _ } => {
            // Find the config file path from the CLI args (already resolved by main)
            // and update the [sync] cloud_url key.
            println!("  Cloud sync URL set to: {url}");
            println!("  Add this to your moneypenny.toml:");
            println!();
            println!("    [sync]");
            println!("    cloud_url = \"{url}\"");
            println!();
            println!("  Then run `mp sync now` to trigger an initial sync.");
        }
    }
    Ok(())
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
fn open_peer_db(
    db_path: &std::path::Path,
    tables: &[&str],
) -> Result<rusqlite::Connection> {
    let conn = rusqlite::Connection::open(db_path)?;
    mp_ext::init_all_extensions(&conn)?;
    mp_core::sync::init_sync_tables(&conn, tables)?;
    Ok(conn)
}

// =========================================================================
// Db
// =========================================================================

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

            println!();
            println!("  {}", col_names.join(" | "));
            println!("  {}", col_names.iter().map(|n| "-".repeat(n.len())).collect::<Vec<_>>().join("-+-"));

            let mut rows = stmt.query([])?;
            while let Some(row) = rows.next()? {
                let vals: Vec<String> = (0..col_count).map(|i| {
                    row.get::<_, String>(i).unwrap_or_else(|_| "NULL".into())
                }).collect();
                println!("  {}", vals.join(" | "));
            }
            println!();
        }
        cli::DbCommand::Schema { .. } => {
            let mut stmt = conn.prepare(
                "SELECT name, sql FROM sqlite_master WHERE type='table' ORDER BY name"
            )?;
            let tables: Vec<(String, Option<String>)> = stmt.query_map([], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })?.collect::<Result<Vec<_>, _>>()?;

            println!();
            for (name, sql) in &tables {
                println!("  -- {name}");
                if let Some(s) = sql {
                    for line in s.lines() {
                        println!("  {line}");
                    }
                }
                println!();
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
                println!("  Session list denied: {}", resp.message);
                return Ok(());
            }
            let rows = resp.data.as_array().cloned().unwrap_or_default();

            if rows.is_empty() {
                println!("  No sessions found for agent '{}'.", ag.name);
                return Ok(());
            }

            let fmt_ts = |ts: i64| -> String {
                chrono::DateTime::<chrono::Utc>::from_timestamp(ts, 0)
                    .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                    .unwrap_or_else(|| ts.to_string())
            };

            println!();
            println!("  Recent sessions for agent '{}':", ag.name);
            println!();
            for row in rows {
                let id = row["id"].as_str().unwrap_or("-");
                let channel = row["channel"].as_str().unwrap_or("unknown");
                let started_at = row["started_at"].as_i64().unwrap_or(0);
                let ended_at = row["ended_at"].as_i64();
                let message_count = row["message_count"].as_i64().unwrap_or(0);
                let last_activity = row["last_activity"].as_i64().unwrap_or(started_at);
                println!("  Session: {}", id);
                println!("    Channel:      {}", channel);
                println!("    Started:      {}", fmt_ts(started_at));
                println!("    Last activity: {}", fmt_ts(last_activity));
                println!("    Messages:     {}", message_count);
                println!("    Ended:        {}", ended_at.map(fmt_ts).unwrap_or_else(|| "active".into()));
                println!();
            }
        }
    }
    Ok(())
}

// =========================================================================
// Health
// =========================================================================

async fn cmd_health(config: &Config) -> Result<()> {
    println!();
    println!("  Moneypenny v{}", env!("CARGO_PKG_VERSION"));
    println!();

    let meta_path = config.metadata_db_path();
    if meta_path.exists() {
        println!("  Gateway:  data dir exists at {}", config.data_dir.display());
    } else {
        println!("  Gateway:  not initialized (run `mp init`)");
    }

    for agent in &config.agents {
        let db_path = config.agent_db_path(&agent.name);
        if db_path.exists() {
            let conn = mp_core::db::open(&db_path)?;
            let fact_count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM facts WHERE superseded_at IS NULL", [], |r| r.get(0)
            ).unwrap_or(0);
            let session_count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM sessions", [], |r| r.get(0)
            ).unwrap_or(0);

            let metadata = std::fs::metadata(&db_path)?;
            let size_kb = metadata.len() / 1024;
            println!("  Agent \"{}\": {size_kb} KB, {fact_count} facts, {session_count} sessions",
                agent.name);
        } else {
            println!("  Agent \"{}\": not initialized", agent.name);
        }
    }

    println!();
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
