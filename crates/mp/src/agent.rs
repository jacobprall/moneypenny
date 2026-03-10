use crate::worker::WorkerBus;
use anyhow::Result;
use mp_llm::provider::{EmbeddingProvider, LlmProvider};
use std::collections::HashSet;

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

pub async fn agent_turn(
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
    for (name, desc, schema) in mp_core::mcp::load_tool_defs(conn) {
        tools.push(mp_llm::types::ToolDef {
            name,
            description: desc,
            parameters: schema,
        });
    }
    for (js_name, js_desc, js_schema) in load_js_tool_defs(conn) {
        tools.push(mp_llm::types::ToolDef {
            name: js_name,
            description: js_desc,
            parameters: js_schema,
        });
    }

    if text_first {
        tools.clear();
        messages.push(mp_llm::types::Message::system(
            "This request is explanatory/planning. Do NOT call tools. Respond directly.",
        ));
    } else if !write_confirmed {
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

            if tc.name == "delegate_to_agent" {
                let args: serde_json::Value =
                    serde_json::from_str(&tc.arguments).unwrap_or_default();
                let target = args["to"].as_str().unwrap_or("");
                let msg = args["message"].as_str().unwrap_or("");

                let delegation_result = if let Some(bus) = worker_bus {
                    match bus.route(target, msg, None).await {
                        Ok(r) => r,
                        Err(e) => format!("Delegation to '{target}' failed: {e}"),
                    }
                } else {
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

    let parsed: serde_json::Value = match serde_json::from_str(arguments) {
        Ok(v) => v,
        Err(_) => return arguments.to_string(),
    };
    let query = match parsed.get("query").and_then(|v| v.as_str()) {
        Some(q) if !q.is_empty() => q,
        _ => return arguments.to_string(),
    };

    match embedder.embed(query).await {
        Ok(vec) => {
            let embedding: Vec<serde_json::Value> = vec
                .into_iter()
                .map(|v| serde_json::Value::from(v as f64))
                .collect();
            let mut obj = parsed;
            if let Some(m) = obj.as_object_mut() {
                m.insert(
                    "__query_embedding".to_string(),
                    serde_json::Value::Array(embedding),
                );
            }
            serde_json::to_string(&obj).unwrap_or_else(|_| arguments.to_string())
        }
        Err(e) => {
            tracing::debug!("memory_search embedding generation failed: {e}");
            arguments.to_string()
        }
    }
}
