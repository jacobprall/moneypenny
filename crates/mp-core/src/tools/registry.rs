use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

/// Tool source type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolSource {
    Builtin,
    Mcp,
    SqliteJs,
    Runtime,
}

impl std::fmt::Display for ToolSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ToolSource::Builtin => write!(f, "builtin"),
            ToolSource::Mcp => write!(f, "mcp"),
            ToolSource::SqliteJs => write!(f, "sqlite_js"),
            ToolSource::Runtime => write!(f, "runtime"),
        }
    }
}

impl std::str::FromStr for ToolSource {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "builtin" => Ok(ToolSource::Builtin),
            "mcp" => Ok(ToolSource::Mcp),
            "sqlite_js" => Ok(ToolSource::SqliteJs),
            "runtime" => Ok(ToolSource::Runtime),
            _ => anyhow::bail!("unknown tool source: {s}"),
        }
    }
}

/// A registered tool definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub source: ToolSource,
    pub parameters_schema: Option<String>,
    pub enabled: bool,
}

/// Result of a tool execution.
#[derive(Debug, Clone)]
pub struct ToolResult {
    pub output: String,
    pub success: bool,
    pub duration_ms: u64,
}

impl ToolResult {
    pub fn success(output: impl Into<String>) -> Self {
        Self {
            output: output.into(),
            success: true,
            duration_ms: 0,
        }
    }

    pub fn failure(output: impl Into<String>) -> Self {
        Self {
            output: output.into(),
            success: false,
            duration_ms: 0,
        }
    }
}

/// Register a tool in the skills table for RAG discoverability.
pub fn register(conn: &Connection, tool: &ToolDef) -> anyhow::Result<String> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp();

    conn.execute(
        "INSERT OR REPLACE INTO skills (id, name, description, content, tool_id, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            id,
            tool.name,
            tool.description,
            tool.parameters_schema.as_deref().unwrap_or("{}"),
            format!("{}:{}", tool.source, tool.name),
            now, now,
        ],
    )?;
    Ok(id)
}

/// Look up a tool definition by name.
pub fn lookup(conn: &Connection, name: &str) -> anyhow::Result<Option<ToolDef>> {
    let result = conn.query_row(
        "SELECT name, description, tool_id, content FROM skills WHERE name = ?1",
        [name],
        |r| {
            let name: String = r.get(0)?;
            let description: String = r.get(1)?;
            let tool_id: Option<String> = r.get(2)?;
            let content: String = r.get(3)?;
            Ok((name, description, tool_id, content))
        },
    );

    match result {
        Ok((name, description, tool_id, content)) => {
            let source = tool_id
                .as_deref()
                .and_then(|tid| tid.split(':').next())
                .and_then(|s| s.parse::<ToolSource>().ok())
                .unwrap_or(ToolSource::Builtin);

            Ok(Some(ToolDef {
                name,
                description,
                source,
                parameters_schema: Some(content),
                enabled: true,
            }))
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// List all registered tools.
pub fn list_tools(conn: &Connection) -> anyhow::Result<Vec<ToolDef>> {
    let mut stmt =
        conn.prepare("SELECT name, description, tool_id, content FROM skills ORDER BY name")?;
    let tools = stmt
        .query_map([], |r| {
            let name: String = r.get(0)?;
            let description: String = r.get(1)?;
            let tool_id: Option<String> = r.get(2)?;
            let content: String = r.get(3)?;
            Ok((name, description, tool_id, content))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(tools
        .into_iter()
        .map(|(name, description, tool_id, content)| {
            let source = tool_id
                .as_deref()
                .and_then(|tid| tid.split(':').next())
                .and_then(|s| s.parse::<ToolSource>().ok())
                .unwrap_or(ToolSource::Builtin);
            ToolDef {
                name,
                description,
                source,
                parameters_schema: Some(content),
                enabled: true,
            }
        })
        .collect())
}

/// Search tools by intent description (delegates to knowledge search).
pub fn discover(conn: &Connection, intent: &str, limit: usize) -> anyhow::Result<Vec<ToolDef>> {
    let pattern = format!("%{intent}%");
    let mut stmt = conn.prepare(
        "SELECT name, description, tool_id, content FROM skills
         WHERE description LIKE ?1 OR name LIKE ?1
         ORDER BY usage_count DESC
         LIMIT ?2",
    )?;
    let tools = stmt
        .query_map(params![pattern, limit], |r| {
            let name: String = r.get(0)?;
            let description: String = r.get(1)?;
            let tool_id: Option<String> = r.get(2)?;
            let content: String = r.get(3)?;
            Ok((name, description, tool_id, content))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(tools
        .into_iter()
        .map(|(name, description, tool_id, content)| {
            let source = tool_id
                .as_deref()
                .and_then(|tid| tid.split(':').next())
                .and_then(|s| s.parse::<ToolSource>().ok())
                .unwrap_or(ToolSource::Builtin);
            ToolDef {
                name,
                description,
                source,
                parameters_schema: Some(content),
                enabled: true,
            }
        })
        .collect())
}

/// Execute a tool with policy gating, hook callbacks, secret redaction, and audit logging.
///
/// # Execution order
///
/// 1. **Policy check** — deny immediately if the policy engine blocks this call.
/// 2. **Pre-hooks** — any hook that aborts produces an immediate denied-style result.
///    A hook that overrides args substitutes them for this execution only.
/// 3. **Tool dispatch** — runtime tools or the provided `executor` closure.
/// 4. **Post-hooks** — may transform the output (e.g. truncate, reformat, enrich).
/// 5. **Secret redaction** — scrubs known secret patterns from the output.
/// 6. **Audit log** — appends to `tool_calls`.
pub fn execute(
    conn: &Connection,
    agent_id: &str,
    session_id: &str,
    message_id: &str,
    tool_name: &str,
    arguments: &str,
    executor: &dyn Fn(&str, &str) -> anyhow::Result<ToolResult>,
    hooks: Option<&super::hooks::ToolHooks>,
) -> anyhow::Result<ToolResult> {
    let start = std::time::Instant::now();

    // 1. Policy check
    let tool_resource = crate::policy::resource::tool(tool_name);
    let request = crate::policy::PolicyRequest {
        actor: agent_id,
        action: "call",
        resource: &tool_resource,
        sql_content: None,
        channel: None,
        arguments: None,
    };
    let decision = crate::policy::evaluate(conn, &request)?;

    if matches!(decision.effect, crate::policy::Effect::Deny) {
        let duration_ms = start.elapsed().as_millis() as u64;
        let reason = decision.reason.as_deref().unwrap_or("policy denied");
        let policy_ref = decision
            .policy_id
            .as_deref()
            .map(|id| format!(" (policy: {id})"))
            .unwrap_or_default();
        let result = ToolResult {
            output: format!(
                "Tool '{tool_name}' denied: {reason}{policy_ref}. \
                 Review policies with `mp policy list`."
            ),
            success: false,
            duration_ms,
        };

        crate::store::log::record_tool_call(
            conn,
            message_id,
            session_id,
            tool_name,
            Some(arguments),
            Some(&result.output),
            Some("denied"),
            Some("deny"),
            Some(duration_ms as i64),
        )?;

        return Ok(result);
    }

    // 2. Pre-hooks
    let effective_args: std::borrow::Cow<str> = if let Some(h) = hooks {
        let hook_ctx = super::hooks::HookContext {
            tool_name: tool_name.into(),
            agent_id: agent_id.into(),
            session_id: session_id.into(),
        };
        match h.run_pre(&hook_ctx, arguments) {
            Ok(None) => std::borrow::Cow::Borrowed(arguments),
            Ok(Some(overridden)) => std::borrow::Cow::Owned(overridden),
            Err(abort_msg) => {
                let duration_ms = start.elapsed().as_millis() as u64;
                let result = ToolResult {
                    output: abort_msg.clone(),
                    success: false,
                    duration_ms,
                };
                crate::store::log::record_tool_call(
                    conn,
                    message_id,
                    session_id,
                    tool_name,
                    Some(arguments),
                    Some(&abort_msg),
                    Some("hook_aborted"),
                    Some("deny"),
                    Some(duration_ms as i64),
                )?;
                return Ok(result);
            }
        }
    } else {
        std::borrow::Cow::Borrowed(arguments)
    };

    // 3. Execute — dispatch in priority order:
    //    a) runtime tools (need the DB connection)
    //    b) MCP tools (spawn the registered server subprocess)
    //    c) sqlite-js tools (execute via node/deno)
    //    d) built-in / external tools via the caller-supplied executor closure
    let exec_result = if super::runtime::is_runtime_tool(tool_name) {
        super::runtime::dispatch(conn, agent_id, session_id, tool_name, &effective_args)
    } else if crate::mcp::is_mcp_tool(conn, tool_name) {
        crate::mcp::dispatch(conn, tool_name, &effective_args)
    } else if super::runtime::is_js_tool(conn, tool_name) {
        super::runtime::dispatch_js(conn, tool_name, &effective_args)
    } else {
        executor(tool_name, &effective_args)
    };
    let duration_ms = start.elapsed().as_millis() as u64;

    match exec_result {
        Ok(mut result) => {
            result.duration_ms = duration_ms;

            // 4. Post-hooks
            if let Some(h) = hooks {
                let hook_ctx = super::hooks::HookContext {
                    tool_name: tool_name.into(),
                    agent_id: agent_id.into(),
                    session_id: session_id.into(),
                };
                if let Some(overridden) = h.run_post(&hook_ctx, &result) {
                    result.output = overridden;
                }
            }

            // 5. Redact secrets from output
            result.output = crate::store::redact::redact(&result.output);

            // 6. Audit log
            let effect_str = format!("{:?}", decision.effect).to_lowercase();
            crate::store::log::record_tool_call(
                conn,
                message_id,
                session_id,
                tool_name,
                Some(arguments),
                Some(&result.output),
                Some(if result.success { "success" } else { "error" }),
                Some(&effect_str),
                Some(duration_ms as i64),
            )?;

            Ok(result)
        }
        Err(e) => {
            let result = ToolResult {
                output: crate::store::redact::redact(&e.to_string()),
                success: false,
                duration_ms,
            };

            let effect_str = format!("{:?}", decision.effect).to_lowercase();
            crate::store::log::record_tool_call(
                conn,
                message_id,
                session_id,
                tool_name,
                Some(arguments),
                Some(&result.output),
                Some("error"),
                Some(&effect_str),
                Some(duration_ms as i64),
            )?;

            Ok(result)
        }
    }
}

/// Register all built-in tools.
pub fn register_builtins(conn: &Connection) -> anyhow::Result<()> {
    let builtins = vec![
        ToolDef {
            name: "file_read".into(),
            description: "Read contents of a file from the filesystem".into(),
            source: ToolSource::Builtin,
            parameters_schema: Some(r#"{"path": "string"}"#.into()),
            enabled: true,
        },
        ToolDef {
            name: "file_write".into(),
            description: "Write contents to a file on the filesystem".into(),
            source: ToolSource::Builtin,
            parameters_schema: Some(r#"{"path": "string", "content": "string"}"#.into()),
            enabled: true,
        },
        ToolDef {
            name: "shell_exec".into(),
            description: "Execute a shell command and return output".into(),
            source: ToolSource::Builtin,
            parameters_schema: Some(r#"{"command": "string", "timeout_ms": "number"}"#.into()),
            enabled: true,
        },
        ToolDef {
            name: "http_request".into(),
            description: "Make an HTTP request to a URL".into(),
            source: ToolSource::Builtin,
            parameters_schema: Some(
                r#"{"method": "string", "url": "string", "headers": "object", "body": "string"}"#
                    .into(),
            ),
            enabled: true,
        },
        ToolDef {
            name: "sql_query".into(),
            description: "Execute a SQL query against the agent's database".into(),
            source: ToolSource::Builtin,
            parameters_schema: Some(r#"{"query": "string"}"#.into()),
            enabled: true,
        },
    ];

    for tool in &builtins {
        register(conn, tool)?;
    }
    Ok(())
}

/// Register all runtime skills — tools that give the agent self-awareness
/// over its own memory, knowledge, scheduling, and governance.
/// Each skill includes a full document (not just a schema) so the agent
/// can reason about when and how to use it.
pub fn register_runtime_skills(conn: &Connection) -> anyhow::Result<()> {
    let runtime_tools = vec![
        ToolDef {
            name: "web_search".into(),
            description: "Search the public web for current information and cite sources.".into(),
            source: ToolSource::Runtime,
            parameters_schema: Some(concat!(
                "# web_search\n\n",
                "Search the public web when current or external information is required.\n\n",
                "## When to use\n",
                "- The user asks for recent events, version updates, docs links, or external facts\n",
                "- You need a source URL to support an answer\n\n",
                "## When NOT to use\n",
                "- The answer can be derived from memory or current conversation context\n",
                "- The user asked for local project analysis only\n\n",
                "## Parameters\n",
                "| Name  | Type   | Required | Default | Description |\n",
                "|-------|--------|----------|---------|-------------|\n",
                "| query | string | yes      |         | Search query |\n",
                "| limit | number | no       | 5       | Max results (1-20) |\n\n",
                "## Example\n",
                "Call: web_search({\"query\": \"SQLite 3.49 release notes\", \"limit\": 3})\n\n",
                "## Returns\n",
                "A JSON array of {title, snippet, url, source} objects."
            ).into()),
            enabled: true,
        },
        // =================================================================
        // Memory
        // =================================================================
        ToolDef {
            name: "memory_search".into(),
            description: "Search the agent's memory across facts, conversation history, and knowledge base.".into(),
            source: ToolSource::Runtime,
            parameters_schema: Some(concat!(
                "# memory_search\n\n",
                "Search across all memory stores — facts, conversation history, and the knowledge base — in a single query.\n\n",
                "## When to use\n",
                "- The user asks \"what do I/we know about X?\"\n",
                "- You need to recall prior context before answering\n",
                "- You want to check whether a fact already exists before adding a new one\n",
                "- You need to find a document or runbook\n\n",
                "## When NOT to use\n",
                "- You already have the answer in the current conversation context\n",
                "- The user is asking you to remember something new (use fact_add)\n\n",
                "## Parameters\n",
                "| Name  | Type   | Required | Default | Description |\n",
                "|-------|--------|----------|---------|-------------|\n",
                "| query | string | yes      |         | Natural language search query |\n",
                "| limit | number | no       | 10      | Max results to return |\n\n",
                "## Example\n",
                "User: \"What do we know about the billing system?\"\n",
                "Call: memory_search({\"query\": \"billing system\"})\n\n",
                "## Notes\n",
                "Results include a score and source store (Facts, Log, or Knowledge). ",
                "Higher-scored results are more relevant. ",
                "The search uses hybrid ranking — it finds both semantic matches and exact keyword matches."
            ).into()),
            enabled: true,
        },
        ToolDef {
            name: "fact_add".into(),
            description: "Store a new fact in long-term memory. Use when the user tells you something important to remember.".into(),
            source: ToolSource::Runtime,
            parameters_schema: Some(concat!(
                "# fact_add\n\n",
                "Persist a durable piece of knowledge to long-term memory. Facts survive across sessions and sync to other agents.\n\n",
                "## When to use\n",
                "- The user explicitly asks you to remember something\n",
                "- You discover a rule, pattern, or convention worth persisting (\"we always deploy on Tuesdays\")\n",
                "- You learn a preference (\"use dark mode\", \"prefer concise answers\")\n",
                "- You extract a stable fact from a conversation that will be useful later\n\n",
                "## When NOT to use\n",
                "- The information is temporary or session-specific (use scratch_set)\n",
                "- The information is a large document (use knowledge_ingest)\n",
                "- A similar fact already exists (use fact_update instead, or search first with memory_search)\n\n",
                "## Parameters\n",
                "| Name       | Type   | Required | Default    | Description |\n",
                "|------------|--------|----------|------------|-------------|\n",
                "| content    | string | yes      |            | The full fact text |\n",
                "| summary    | string | no       | = content  | A shorter version for quick scanning |\n",
                "| pointer    | string | no       | = content  | A one-line label for context windows |\n",
                "| keywords   | string | no       |            | Space-separated keywords for search |\n",
                "| confidence | number | no       | 1.0        | How confident you are (0.0 to 10.0) |\n\n",
                "## Example\n",
                "User: \"Remember that the ORDERS table uses soft deletes.\"\n",
                "Call: fact_add({\"content\": \"The ORDERS table uses soft deletes via a deleted_at column. Always filter WHERE deleted_at IS NULL.\", ",
                "\"summary\": \"ORDERS uses soft deletes\", \"pointer\": \"ORDERS: soft-delete filter\", ",
                "\"keywords\": \"orders soft-delete deleted_at\"})\n\n",
                "## Best practices\n",
                "- Write the content as a self-contained statement, not a sentence fragment\n",
                "- Include actionable details — not just \"uses soft deletes\" but the column name and filter\n",
                "- Set keywords that cover synonyms and related terms\n",
                "- Search with memory_search first to avoid duplicates"
            ).into()),
            enabled: true,
        },
        ToolDef {
            name: "fact_update".into(),
            description: "Update an existing fact when information has changed or been refined.".into(),
            source: ToolSource::Runtime,
            parameters_schema: Some(concat!(
                "# fact_update\n\n",
                "Modify an existing fact in long-term memory. Preserves the fact's history in the audit trail.\n\n",
                "## When to use\n",
                "- A previously stored fact is now outdated or incorrect\n",
                "- You have more detail to add to an existing fact\n",
                "- The user corrects something you remembered\n\n",
                "## When NOT to use\n",
                "- The fact doesn't exist yet (use fact_add)\n",
                "- You're not sure which fact to update (use memory_search to find it first)\n\n",
                "## Parameters\n",
                "| Name    | Type   | Required | Default   | Description |\n",
                "|---------|--------|----------|-----------|-------------|\n",
                "| id      | string | yes      |           | The fact ID (from fact_list or memory_search) |\n",
                "| content | string | yes      |           | The updated fact text |\n",
                "| summary | string | no       | = content | Updated short summary |\n",
                "| pointer | string | no       | = content | Updated one-line label |\n\n",
                "## Example\n",
                "Call: fact_update({\"id\": \"abc-123\", \"content\": \"Deploys moved from Tuesday to Thursday as of March 2026.\"})\n\n",
                "## Notes\n",
                "The old content is preserved in the audit trail. The fact's version number is bumped automatically."
            ).into()),
            enabled: true,
        },
        ToolDef {
            name: "fact_list".into(),
            description: "List all active facts in memory for review or audit.".into(),
            source: ToolSource::Runtime,
            parameters_schema: Some(concat!(
                "# fact_list\n\n",
                "Retrieve all active (non-superseded) facts for this agent.\n\n",
                "## When to use\n",
                "- The user asks \"what do you know?\" or \"show me your facts\"\n",
                "- You need to audit or review stored knowledge\n",
                "- You want to find a fact ID for fact_update\n\n",
                "## Parameters\n",
                "None.\n\n",
                "## Returns\n",
                "A JSON array of facts, each with: id, content, summary, confidence, version.\n\n",
                "## Notes\n",
                "For targeted retrieval, prefer memory_search over fact_list — it's faster and returns ranked results."
            ).into()),
            enabled: true,
        },
        // =================================================================
        // Scratch (session working memory)
        // =================================================================
        ToolDef {
            name: "scratch_set".into(),
            description: "Save a value to session working memory for intermediate results and plans.".into(),
            source: ToolSource::Runtime,
            parameters_schema: Some(concat!(
                "# scratch_set\n\n",
                "Write a key-value pair to session-scoped working memory. Scratch data is ephemeral — it exists only for the current session.\n\n",
                "## When to use\n",
                "- You're building up a multi-step plan and need to track progress\n",
                "- You want to store intermediate results from tool calls\n",
                "- You need a scratchpad for analysis before committing findings to facts\n",
                "- You want to cache something expensive to recompute within the session\n\n",
                "## When NOT to use\n",
                "- The information should persist across sessions (use fact_add)\n",
                "- The information is a large document (use knowledge_ingest)\n\n",
                "## Parameters\n",
                "| Name    | Type   | Required | Description |\n",
                "|---------|--------|----------|-------------|\n",
                "| key     | string | yes      | A short label (e.g. \"plan\", \"findings\", \"step_3_result\") |\n",
                "| content | string | yes      | The value to store |\n\n",
                "## Example\n",
                "Call: scratch_set({\"key\": \"investigation_plan\", \"content\": \"1. Check error logs\\n2. Query recent deploys\\n3. Compare with baseline\"})\n\n",
                "## Notes\n",
                "Writing the same key again overwrites the previous value. At session end, consider promoting durable findings to facts."
            ).into()),
            enabled: true,
        },
        ToolDef {
            name: "scratch_get".into(),
            description: "Retrieve a value from session working memory.".into(),
            source: ToolSource::Runtime,
            parameters_schema: Some(concat!(
                "# scratch_get\n\n",
                "Read a value previously saved with scratch_set in this session.\n\n",
                "## Parameters\n",
                "| Name | Type   | Required | Description |\n",
                "|------|--------|----------|-------------|\n",
                "| key  | string | yes      | The key to look up |\n\n",
                "## Returns\n",
                "The stored content, or null if the key doesn't exist.\n\n",
                "## Example\n",
                "Call: scratch_get({\"key\": \"investigation_plan\"})"
            ).into()),
            enabled: true,
        },
        // =================================================================
        // Knowledge
        // =================================================================
        ToolDef {
            name: "knowledge_ingest".into(),
            description: "Ingest a document into the knowledge base. Automatically chunks and indexes for search.".into(),
            source: ToolSource::Runtime,
            parameters_schema: Some(concat!(
                "# knowledge_ingest\n\n",
                "Add a document to the knowledge base. The content is automatically split into chunks at heading boundaries (~2000 chars) and indexed for search.\n\n",
                "## When to use\n",
                "- The user provides a runbook, guide, or reference document\n",
                "- You need to import documentation for later retrieval\n",
                "- The user pastes a large block of structured content\n\n",
                "## When NOT to use\n",
                "- The content is a single short fact (use fact_add)\n",
                "- The content is session-specific working notes (use scratch_set)\n\n",
                "## Parameters\n",
                "| Name    | Type   | Required | Description |\n",
                "|---------|--------|----------|-------------|\n",
                "| content | string | yes      | The full document text (markdown supported) |\n",
                "| title   | string | no       | Document title for display |\n",
                "| path    | string | no       | Source file path (if ingesting from a file) |\n\n",
                "## Example\n",
                "Call: knowledge_ingest({\"title\": \"Deploy Runbook\", \"content\": \"# Pre-deploy\\n1. Run tests...\\n# Deploy\\n2. Push to staging...\"})\n\n",
                "## Notes\n",
                "After ingestion, the document's chunks are searchable via memory_search. ",
                "Returns the document ID and number of chunks created."
            ).into()),
            enabled: true,
        },
        ToolDef {
            name: "knowledge_list".into(),
            description: "List all documents in the knowledge base.".into(),
            source: ToolSource::Runtime,
            parameters_schema: Some(concat!(
                "# knowledge_list\n\n",
                "List all documents that have been ingested into the knowledge base.\n\n",
                "## When to use\n",
                "- The user asks what reference material is available\n",
                "- You need to check if a document has already been ingested\n\n",
                "## Parameters\n",
                "None.\n\n",
                "## Returns\n",
                "A JSON array of documents with: id, title, path."
            ).into()),
            enabled: true,
        },
        // =================================================================
        // Scheduling
        // =================================================================
        ToolDef {
            name: "job_create".into(),
            description: "Schedule a recurring task with a cron expression.".into(),
            source: ToolSource::Runtime,
            parameters_schema: Some(concat!(
                "# job_create\n\n",
                "Schedule a recurring task that runs automatically on a cron schedule.\n\n",
                "## When to use\n",
                "- The user says \"every morning\", \"daily\", \"every hour\", or any recurring pattern\n",
                "- The user asks you to do something \"at 9am\" or \"on Mondays\"\n",
                "- You need to set up a periodic check, digest, or report\n\n",
                "## Parameters\n",
                "| Name        | Type   | Required | Default  | Description |\n",
                "|-------------|--------|----------|----------|-------------|\n",
                "| name        | string | yes      |          | Human-readable job name |\n",
                "| schedule    | string | yes      |          | Cron expression (see below) |\n",
                "| job_type    | string | no       | prompt   | One of: prompt, tool, js, pipeline |\n",
                "| payload     | string | no       | {}       | JSON payload passed to the job |\n",
                "| description | string | no       |          | What this job does |\n\n",
                "## Cron format\n",
                "```\n",
                "minute  hour  day-of-month  month  day-of-week\n",
                "  0       9       *           *        *        = every day at 9:00 AM\n",
                "  */15     *       *           *        *        = every 15 minutes\n",
                "  0       9       *           *        1-5      = weekdays at 9:00 AM\n",
                "  0       0       1           *        *        = first of every month at midnight\n",
                "```\n\n",
                "## Example\n",
                "User: \"Summarize my facts every morning at 9am.\"\n",
                "Call: job_create({\"name\": \"daily-fact-digest\", \"schedule\": \"0 9 * * *\", ",
                "\"description\": \"Generate a daily summary of recently added facts\", ",
                "\"payload\": \"{\\\"message\\\": \\\"Summarize all facts added in the last 24 hours\\\"}\"})\n\n",
                "## Job types\n",
                "- **prompt**: Sends the payload message to the agent as if a user said it\n",
                "- **tool**: Calls a specific tool with the payload as arguments\n",
                "- **js**: Executes a sqlite-js function\n",
                "- **pipeline**: Runs a multi-step extraction pipeline\n\n",
                "## Notes\n",
                "Jobs are governed by policy — they can be restricted by agent trust level. ",
                "Jobs sync across agents via CRDTs."
            ).into()),
            enabled: true,
        },
        ToolDef {
            name: "job_list".into(),
            description: "List all scheduled jobs and their status.".into(),
            source: ToolSource::Runtime,
            parameters_schema: Some(concat!(
                "# job_list\n\n",
                "Show all scheduled jobs for this agent.\n\n",
                "## When to use\n",
                "- The user asks \"what's scheduled?\" or \"show my jobs\"\n",
                "- You need a job ID to pause or resume a job\n\n",
                "## Parameters\n",
                "None.\n\n",
                "## Returns\n",
                "A JSON array of jobs with: id, name, schedule, status, enabled, job_type."
            ).into()),
            enabled: true,
        },
        ToolDef {
            name: "job_pause".into(),
            description: "Pause a scheduled job without deleting it.".into(),
            source: ToolSource::Runtime,
            parameters_schema: Some(concat!(
                "# job_pause\n\n",
                "Temporarily stop a scheduled job. The job's configuration is preserved and can be resumed later.\n\n",
                "## Parameters\n",
                "| Name | Type   | Required | Description |\n",
                "|------|--------|----------|-------------|\n",
                "| id   | string | yes      | The job ID (from job_list) |\n\n",
                "## Example\n",
                "User: \"Pause the daily digest.\"\n",
                "1. Call job_list to find the job ID\n",
                "2. Call job_pause({\"id\": \"abc-123\"})"
            ).into()),
            enabled: true,
        },
        ToolDef {
            name: "job_resume".into(),
            description: "Resume a paused scheduled job.".into(),
            source: ToolSource::Runtime,
            parameters_schema: Some(concat!(
                "# job_resume\n\n",
                "Restart a previously paused job.\n\n",
                "## Parameters\n",
                "| Name | Type   | Required | Description |\n",
                "|------|--------|----------|-------------|\n",
                "| id   | string | yes      | The job ID (from job_list) |\n\n",
                "## Example\n",
                "Call: job_resume({\"id\": \"abc-123\"})"
            ).into()),
            enabled: true,
        },
        // =================================================================
        // Governance
        // =================================================================
        ToolDef {
            name: "policy_list".into(),
            description: "List all active policies governing this agent's behavior.".into(),
            source: ToolSource::Runtime,
            parameters_schema: Some(concat!(
                "# policy_list\n\n",
                "Show all policies that govern this agent — what it can and cannot do.\n\n",
                "## When to use\n",
                "- The user asks \"what are your rules?\" or \"what can't you do?\"\n",
                "- A tool call was denied and you want to explain why\n",
                "- You need to understand what restrictions are in place\n\n",
                "## Parameters\n",
                "None.\n\n",
                "## Returns\n",
                "A JSON array of policies with: id, name, priority, effect (allow/deny/audit), ",
                "actor_pattern, action_pattern, resource_pattern, message, enabled.\n\n",
                "## Notes\n",
                "Policies are evaluated highest-priority-first. If no policy matches, the default is deny."
            ).into()),
            enabled: true,
        },
        ToolDef {
            name: "policy_add".into(),
            description: "Propose a new policy rule governing this agent's behavior. Requires user confirmation before activation.".into(),
            source: ToolSource::Runtime,
            parameters_schema: Some(concat!(
                "# policy_add\n\n",
                "Propose a new policy rule. This creates a **draft** (policy spec) that the user must confirm.\n",
                "Policies control what this agent can and cannot do.\n\n",
                "## When to use\n",
                "- The user says \"only allow URLs from docs.example.com\"\n",
                "- The user says \"block shell access\" or \"don't let agents call shell_exec\"\n",
                "- The user says \"add a rate limit\" or \"restrict tool usage\"\n",
                "- The user wants to whitelist or blacklist specific actions or resources\n\n",
                "## Parameters\n",
                "| Name | Type | Required | Default | Description |\n",
                "|------|------|----------|---------|-------------|\n",
                "| name | string | **yes** | — | Human-readable policy name |\n",
                "| intent | string | no | — | Natural-language description of what this policy does |\n",
                "| effect | string | no | deny | \"allow\", \"deny\", or \"audit\" |\n",
                "| priority | integer | no | 0 | Higher priority rules are evaluated first |\n",
                "| actor_pattern | string | no | null | Glob pattern for who (e.g. \"agent:*\") |\n",
                "| action_pattern | string | no | null | Glob pattern for what action (e.g. \"ingest\", \"call\") |\n",
                "| resource_pattern | string | no | null | Glob pattern for the resource (e.g. \"knowledge:url\", \"tool:shell_*\") |\n",
                "| argument_pattern | string | no | null | Glob pattern for arguments (e.g. URL patterns like \"https://docs.example.com/*\") |\n",
                "| channel_pattern | string | no | null | Glob pattern for channel (e.g. \"slack:*\") |\n",
                "| message | string | no | null | Message shown when this policy triggers |\n\n",
                "## How policies work\n",
                "- Policies are evaluated **highest priority first**\n",
                "- The first matching policy wins\n",
                "- Glob patterns: `*` matches any characters, `?` is not supported\n",
                "- `null` pattern matches everything (wildcard)\n\n",
                "## Common patterns\n\n",
                "### URL whitelist (allow specific domains, deny the rest)\n",
                "1. Call policy_add with: name=\"allow docs.example.com\", effect=\"allow\", priority=100, ",
                "action_pattern=\"ingest\", resource_pattern=\"knowledge:url\", argument_pattern=\"https://docs.example.com/*\"\n",
                "2. Call policy_add with: name=\"deny other URLs\", effect=\"deny\", priority=10, ",
                "action_pattern=\"ingest\", resource_pattern=\"knowledge:url\", message=\"URL not whitelisted\"\n\n",
                "### Block a tool\n",
                "Call policy_add with: name=\"block shell\", effect=\"deny\", priority=100, ",
                "action_pattern=\"call\", resource_pattern=\"tool:shell_*\", message=\"Shell access blocked\"\n\n",
                "### Audit all actions on a channel\n",
                "Call policy_add with: name=\"audit slack\", effect=\"audit\", priority=50, ",
                "channel_pattern=\"slack:*\"\n\n",
                "## Returns\n",
                "A confirmation message with the spec_id. The policy is NOT yet active — ",
                "tell the user what you proposed and ask them to confirm."
            ).into()),
            enabled: true,
        },
        ToolDef {
            name: "audit_query".into(),
            description: "Query the audit trail of policy decisions and agent actions.".into(),
            source: ToolSource::Runtime,
            parameters_schema: Some(concat!(
                "# audit_query\n\n",
                "Query the audit trail to see what actions were taken and which policy decisions were made.\n\n",
                "## When to use\n",
                "- The user asks \"what happened?\" or \"why was that denied?\"\n",
                "- You need to review recent actions for debugging or accountability\n",
                "- The user wants a report on agent activity\n\n",
                "## Parameters\n",
                "| Name  | Type   | Required | Default | Description |\n",
                "|-------|--------|----------|---------|-------------|\n",
                "| limit | number | no       | 20      | Max entries to return |\n",
                "| scope | string | no       | session | \"session\" for current session, \"all\" for everything |\n\n",
                "## Example\n",
                "User: \"Why couldn't you run that shell command?\"\n",
                "Call: audit_query({\"scope\": \"session\", \"limit\": 5})\n\n",
                "## Returns\n",
                "A JSON array of audit entries with: id, policy_id, actor, action, resource, effect, reason, created_at."
            ).into()),
            enabled: true,
        },
        // =================================================================
        // JS tools
        // =================================================================
        ToolDef {
            name: "js_tool_add".into(),
            description: "Define and persist a custom JavaScript tool callable in future turns.".into(),
            source: ToolSource::SqliteJs,
            parameters_schema: Some(concat!(
                "# js_tool_add\n\n",
                "Persist a JavaScript function as a named tool. Once added, the tool can be called \n",
                "by name in any future agent turn — it behaves exactly like a built-in tool.\n\n",
                "## Script contract\n",
                "The script must define a function named `run` that:\n",
                "- Accepts one argument: `args` (the parsed JSON object passed when the tool is called)\n",
                "- Returns a value that will be JSON-serialised and returned as the tool output\n\n",
                "```js\n",
                "function run(args) {\n",
                "    return { result: args.a + args.b };\n",
                "}\n",
                "```\n\n",
                "## Parameters\n",
                "| Name        | Type   | Required | Description |\n",
                "|-------------|--------|----------|-------------|\n",
                "| name        | string | yes      | Tool name (letters, digits, underscores, hyphens) |\n",
                "| description | string | no       | What this tool does |\n",
                "| script      | string | yes      | JavaScript source containing a `run(args)` function |\n\n",
                "## Example\n",
                "Call: js_tool_add({\"name\": \"add_numbers\", \"description\": \"Add two numbers\", ",
                "\"script\": \"function run(args) { return { result: args.a + args.b }; }\"})\n\n",
                "## Notes\n",
                "Requires Node.js (`node`) or Deno (`deno`) on PATH for execution. ",
                "Calling the same name again overwrites the previous script."
            ).into()),
            enabled: true,
        },
        ToolDef {
            name: "js_tool_list".into(),
            description: "List all user-defined JavaScript tools.".into(),
            source: ToolSource::SqliteJs,
            parameters_schema: Some(concat!(
                "# js_tool_list\n\n",
                "Show all JavaScript tools that have been defined with js_tool_add.\n\n",
                "## Parameters\n",
                "None.\n\n",
                "## Returns\n",
                "A JSON array of {name, description, updated_at} objects."
            ).into()),
            enabled: true,
        },
        ToolDef {
            name: "js_tool_delete".into(),
            description: "Delete a user-defined JavaScript tool by name.".into(),
            source: ToolSource::SqliteJs,
            parameters_schema: Some(concat!(
                "# js_tool_delete\n\n",
                "Remove a JavaScript tool that was created with js_tool_add.\n\n",
                "## Parameters\n",
                "| Name | Type   | Required | Description |\n",
                "|------|--------|----------|-------------|\n",
                "| name | string | yes      | Name of the tool to delete |\n\n",
                "## Example\n",
                "Call: js_tool_delete({\"name\": \"add_numbers\"})"
            ).into()),
            enabled: true,
        },
    ];

    for tool in &runtime_tools {
        register(conn, tool)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db, schema, store};
    use rusqlite::Connection;

    fn setup() -> Connection {
        let conn = db::open_memory().unwrap();
        schema::init_agent_db(&conn).unwrap();
        conn
    }

    // ========================================================================
    // Registration
    // ========================================================================

    #[test]
    fn register_and_lookup_tool() {
        let conn = setup();
        let tool = ToolDef {
            name: "test_tool".into(),
            description: "A test tool".into(),
            source: ToolSource::Builtin,
            parameters_schema: Some(r#"{"arg": "string"}"#.into()),
            enabled: true,
        };
        register(&conn, &tool).unwrap();

        let found = lookup(&conn, "test_tool").unwrap().unwrap();
        assert_eq!(found.name, "test_tool");
        assert_eq!(found.description, "A test tool");
        assert_eq!(found.source, ToolSource::Builtin);
    }

    #[test]
    fn lookup_nonexistent_returns_none() {
        let conn = setup();
        assert!(lookup(&conn, "nope").unwrap().is_none());
    }

    #[test]
    fn register_builtins_creates_five_tools() {
        let conn = setup();
        register_builtins(&conn).unwrap();
        let tools = list_tools(&conn).unwrap();
        assert_eq!(tools.len(), 5);
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"file_read"));
        assert!(names.contains(&"file_write"));
        assert!(names.contains(&"shell_exec"));
        assert!(names.contains(&"http_request"));
        assert!(names.contains(&"sql_query"));
    }

    #[test]
    fn list_tools_empty_initially() {
        let conn = setup();
        let tools = list_tools(&conn).unwrap();
        assert!(tools.is_empty());
    }

    // ========================================================================
    // Discovery
    // ========================================================================

    #[test]
    fn discover_finds_by_name() {
        let conn = setup();
        register_builtins(&conn).unwrap();
        let found = discover(&conn, "shell", 10).unwrap();
        assert!(!found.is_empty());
        assert!(found.iter().any(|t| t.name == "shell_exec"));
    }

    #[test]
    fn discover_finds_by_description() {
        let conn = setup();
        register_builtins(&conn).unwrap();
        let found = discover(&conn, "HTTP", 10).unwrap();
        assert!(found.iter().any(|t| t.name == "http_request"));
    }

    #[test]
    fn discover_returns_empty_for_no_match() {
        let conn = setup();
        register_builtins(&conn).unwrap();
        let found = discover(&conn, "quantum_computing", 10).unwrap();
        assert!(found.is_empty());
    }

    #[test]
    fn discover_respects_limit() {
        let conn = setup();
        register_builtins(&conn).unwrap();
        let found = discover(&conn, "file", 1).unwrap();
        assert!(found.len() <= 1);
    }

    // ========================================================================
    // Tool source parsing
    // ========================================================================

    #[test]
    fn tool_source_roundtrip() {
        for src in [
            ToolSource::Builtin,
            ToolSource::Mcp,
            ToolSource::SqliteJs,
            ToolSource::Runtime,
        ] {
            let s = src.to_string();
            let parsed: ToolSource = s.parse().unwrap();
            assert_eq!(parsed, src);
        }
    }

    #[test]
    fn tool_source_invalid() {
        assert!("garbage".parse::<ToolSource>().is_err());
    }

    // ========================================================================
    // Execution with policy gating
    // ========================================================================

    fn insert_allow_all(conn: &Connection) {
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, created_at)
             VALUES ('allow-all', 'allow-all', 0, 'allow', '*', '*', '*', 1)",
            [],
        ).unwrap();
    }

    #[test]
    fn execute_allowed_tool() {
        let conn = setup();
        insert_allow_all(&conn);
        register_builtins(&conn).unwrap();
        let sid = store::log::create_session(&conn, "a", None).unwrap();
        let mid = store::log::append_message(&conn, &sid, "assistant", "calling tool").unwrap();

        let result = execute(
            &conn,
            "a",
            &sid,
            &mid,
            "shell_exec",
            r#"{"command":"echo hi"}"#,
            &|_name, _args| {
                Ok(ToolResult {
                    output: "hi\n".into(),
                    success: true,
                    duration_ms: 0,
                })
            },
            None,
        )
        .unwrap();

        assert!(result.success);
        assert_eq!(result.output, "hi\n");

        let calls = store::log::get_tool_calls(&conn, &sid).unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].status.as_deref(), Some("success"));
    }

    #[test]
    fn execute_denied_tool() {
        let conn = setup();
        insert_allow_all(&conn);
        register_builtins(&conn).unwrap();
        let sid = store::log::create_session(&conn, "a", None).unwrap();
        let mid = store::log::append_message(&conn, &sid, "assistant", "calling tool").unwrap();

        // Insert a deny policy for shell_exec at higher priority than allow-all
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, message, created_at)
             VALUES ('p1', 'no-shell', 100, 'deny', '*', 'call', 'tool:shell_exec', 'shell access denied', 1)",
            [],
        ).unwrap();

        let result = execute(
            &conn,
            "a",
            &sid,
            &mid,
            "shell_exec",
            r#"{"command":"rm -rf /"}"#,
            &|_name, _args| panic!("should not be called"),
            None,
        )
        .unwrap();

        assert!(!result.success);
        assert!(result.output.contains("denied"));

        let calls = store::log::get_tool_calls(&conn, &sid).unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].status.as_deref(), Some("denied"));
    }

    #[test]
    fn execute_redacts_secrets_from_output() {
        let conn = setup();
        insert_allow_all(&conn);
        let sid = store::log::create_session(&conn, "a", None).unwrap();
        let mid = store::log::append_message(&conn, &sid, "assistant", "calling tool").unwrap();

        let result = execute(
            &conn,
            "a",
            &sid,
            &mid,
            "sql_query",
            "{}",
            &|_name, _args| {
                Ok(ToolResult {
                    output: "connection: postgres://user:password123@host/db".into(),
                    success: true,
                    duration_ms: 0,
                })
            },
            None,
        )
        .unwrap();

        assert!(result.success);
        assert!(
            result.output.contains("[REDACTED]"),
            "secrets should be redacted: {}",
            result.output
        );
        assert!(!result.output.contains("password123"));
    }

    #[test]
    fn execute_handles_tool_error() {
        let conn = setup();
        insert_allow_all(&conn);
        let sid = store::log::create_session(&conn, "a", None).unwrap();
        let mid = store::log::append_message(&conn, &sid, "assistant", "calling tool").unwrap();

        let result = execute(
            &conn,
            "a",
            &sid,
            &mid,
            "file_read",
            "{}",
            &|_name, _args| anyhow::bail!("file not found"),
            None,
        )
        .unwrap();

        assert!(!result.success);
        assert!(result.output.contains("file not found"));

        let calls = store::log::get_tool_calls(&conn, &sid).unwrap();
        assert_eq!(calls[0].status.as_deref(), Some("error"));
    }

    #[test]
    fn execute_logs_audit_trail() {
        let conn = setup();
        insert_allow_all(&conn);
        let sid = store::log::create_session(&conn, "a", None).unwrap();
        let mid = store::log::append_message(&conn, &sid, "assistant", "tool call").unwrap();

        execute(
            &conn,
            "a",
            &sid,
            &mid,
            "sql_query",
            r#"{"query":"SELECT 1"}"#,
            &|_name, _args| {
                Ok(ToolResult {
                    output: "1".into(),
                    success: true,
                    duration_ms: 0,
                })
            },
            None,
        )
        .unwrap();

        let calls = store::log::get_tool_calls(&conn, &sid).unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].tool_name, "sql_query");
        assert!(calls[0].arguments.as_deref().unwrap().contains("SELECT 1"));
        assert_eq!(calls[0].result.as_deref(), Some("1"));
        assert!(calls[0].duration_ms.is_some());
    }

    // ========================================================================
    // Runtime skills registration
    // ========================================================================

    #[test]
    fn register_runtime_skills_creates_nineteen_tools() {
        let conn = setup();
        register_runtime_skills(&conn).unwrap();
        let tools = list_tools(&conn).unwrap();
        assert_eq!(tools.len(), 19);
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"web_search"));
        assert!(names.contains(&"memory_search"));
        assert!(names.contains(&"fact_add"));
        assert!(names.contains(&"fact_update"));
        assert!(names.contains(&"fact_list"));
        assert!(names.contains(&"scratch_set"));
        assert!(names.contains(&"scratch_get"));
        assert!(names.contains(&"knowledge_ingest"));
        assert!(names.contains(&"knowledge_list"));
        assert!(names.contains(&"job_create"));
        assert!(names.contains(&"job_list"));
        assert!(names.contains(&"job_pause"));
        assert!(names.contains(&"job_resume"));
        assert!(names.contains(&"policy_list"));
        assert!(names.contains(&"audit_query"));
        assert!(names.contains(&"js_tool_add"));
        assert!(names.contains(&"js_tool_list"));
        assert!(names.contains(&"js_tool_delete"));
    }

    #[test]
    fn runtime_tools_have_runtime_source() {
        let conn = setup();
        register_runtime_skills(&conn).unwrap();
        let tool = lookup(&conn, "memory_search").unwrap().unwrap();
        assert_eq!(tool.source, ToolSource::Runtime);
    }

    #[test]
    fn register_both_builtins_and_runtime() {
        let conn = setup();
        register_builtins(&conn).unwrap();
        register_runtime_skills(&conn).unwrap();
        let tools = list_tools(&conn).unwrap();
        assert_eq!(tools.len(), 24); // 5 builtins + 19 runtime (16 + 3 JS tools)
    }

    #[test]
    fn discover_finds_runtime_tools_by_description() {
        let conn = setup();
        register_runtime_skills(&conn).unwrap();
        let found = discover(&conn, "memory", 10).unwrap();
        assert!(found.iter().any(|t| t.name == "memory_search"));
    }

    #[test]
    fn discover_finds_runtime_tools_by_schedule() {
        let conn = setup();
        register_runtime_skills(&conn).unwrap();
        let found = discover(&conn, "schedule", 10).unwrap();
        assert!(found.iter().any(|t| t.name == "job_create"));
    }

    // ========================================================================
    // Runtime tool execution via registry::execute (integration)
    // ========================================================================

    #[test]
    fn execute_runtime_tool_fact_add() {
        let conn = setup();
        insert_allow_all(&conn);
        register_runtime_skills(&conn).unwrap();
        let sid = store::log::create_session(&conn, "agent-1", None).unwrap();
        let mid = store::log::append_message(&conn, &sid, "assistant", "adding fact").unwrap();

        let result = execute(
            &conn,
            "agent-1",
            &sid,
            &mid,
            "fact_add",
            r#"{"content": "Test fact via execute", "summary": "test", "pointer": "test"}"#,
            &|_name, _args| panic!("runtime tools should not use the builtin executor"),
            None,
        )
        .unwrap();

        assert!(result.success);
        assert!(result.output.contains("created"));

        let calls = store::log::get_tool_calls(&conn, &sid).unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].tool_name, "fact_add");
        assert_eq!(calls[0].status.as_deref(), Some("success"));
    }

    #[test]
    fn execute_runtime_tool_denied_by_policy() {
        let conn = setup();
        insert_allow_all(&conn);
        register_runtime_skills(&conn).unwrap();
        let sid = store::log::create_session(&conn, "agent-1", None).unwrap();
        let mid = store::log::append_message(&conn, &sid, "assistant", "trying fact_add").unwrap();

        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, message, created_at)
             VALUES ('deny-facts', 'no fact writes', 100, 'deny', '*', 'call', 'tool:fact_add', 'fact writes disabled', 1)",
            [],
        ).unwrap();

        let result = execute(
            &conn,
            "agent-1",
            &sid,
            &mid,
            "fact_add",
            r#"{"content": "should not persist"}"#,
            &|_name, _args| panic!("should not reach executor"),
            None,
        )
        .unwrap();

        assert!(!result.success);
        assert!(result.output.contains("denied"));

        let facts = store::facts::list_active(&conn, "agent-1").unwrap();
        assert!(facts.is_empty());
    }

    #[test]
    fn execute_runtime_tool_memory_search() {
        let conn = setup();
        insert_allow_all(&conn);
        register_runtime_skills(&conn).unwrap();
        let sid = store::log::create_session(&conn, "a", None).unwrap();
        let mid = store::log::append_message(&conn, &sid, "assistant", "searching").unwrap();

        let result = execute(
            &conn,
            "a",
            &sid,
            &mid,
            "memory_search",
            r#"{"query": "test"}"#,
            &|_name, _args| panic!("should not use builtin executor"),
            None,
        )
        .unwrap();

        assert!(result.success);
    }

    // ========================================================================
    // Hook integration
    // ========================================================================

    #[test]
    fn pre_hook_abort_blocks_execution_in_execute() {
        use super::super::hooks::{PreOutcome, ToolHooks};

        let conn = setup();
        insert_allow_all(&conn);
        let sid = store::log::create_session(&conn, "a", None).unwrap();
        let mid = store::log::append_message(&conn, &sid, "assistant", "tool").unwrap();

        let mut hooks = ToolHooks::new();
        hooks.add_pre("block-all", "*", |_, _| {
            PreOutcome::Abort("pre-hook blocked this call".into())
        });

        let result = execute(
            &conn,
            "a",
            &sid,
            &mid,
            "shell_exec",
            "{}",
            &|_, _| panic!("should not reach executor"),
            Some(&hooks),
        )
        .unwrap();

        assert!(!result.success);
        assert!(result.output.contains("pre-hook blocked"));

        let calls = store::log::get_tool_calls(&conn, &sid).unwrap();
        assert_eq!(calls[0].status.as_deref(), Some("hook_aborted"));
    }

    #[test]
    fn post_hook_transforms_output_in_execute() {
        use super::super::hooks::{PostOutcome, ToolHooks};

        let conn = setup();
        insert_allow_all(&conn);
        let sid = store::log::create_session(&conn, "a", None).unwrap();
        let mid = store::log::append_message(&conn, &sid, "assistant", "tool").unwrap();

        let mut hooks = ToolHooks::new();
        hooks.add_post("upper", "*", |_, r| {
            PostOutcome::OverrideOutput(r.output.to_uppercase())
        });

        let result = execute(
            &conn,
            "a",
            &sid,
            &mid,
            "shell_exec",
            "{}",
            &|_, _| {
                Ok(ToolResult {
                    output: "hello".into(),
                    success: true,
                    duration_ms: 0,
                })
            },
            Some(&hooks),
        )
        .unwrap();

        assert!(result.success);
        assert_eq!(result.output, "HELLO");
    }

    #[test]
    fn pre_hook_overrides_args_before_execution() {
        use super::super::hooks::{PreOutcome, ToolHooks};
        use std::sync::{Arc, Mutex};

        let conn = setup();
        insert_allow_all(&conn);
        let sid = store::log::create_session(&conn, "a", None).unwrap();
        let mid = store::log::append_message(&conn, &sid, "assistant", "tool").unwrap();

        let mut hooks = ToolHooks::new();
        hooks.add_pre("override-args", "*", |_, _| PreOutcome::Continue {
            args: Some(r#"{"command":"echo overridden"}"#.into()),
        });

        let received_args: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
        let captured = Arc::clone(&received_args);

        let result = execute(
            &conn,
            "a",
            &sid,
            &mid,
            "shell_exec",
            r#"{"command":"original"}"#,
            &|_, args| {
                *captured.lock().unwrap() = args.to_string();
                Ok(ToolResult {
                    output: "ok".into(),
                    success: true,
                    duration_ms: 0,
                })
            },
            Some(&hooks),
        )
        .unwrap();

        assert!(result.success);
        let seen = received_args.lock().unwrap().clone();
        assert!(
            seen.contains("overridden"),
            "args should be overridden: {seen}"
        );
    }

    #[test]
    fn tool_glob_pattern_blocks_via_execute() {
        let conn = setup();
        insert_allow_all(&conn);
        register_builtins(&conn).unwrap();
        let sid = store::log::create_session(&conn, "a", None).unwrap();
        let mid = store::log::append_message(&conn, &sid, "assistant", "calling tool").unwrap();

        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, message, created_at)
             VALUES ('deny-shell', 'no shell', 100, 'deny', '*', 'call', 'tool:shell_*', 'shell blocked', 1)",
            [],
        ).unwrap();

        let result = execute(
            &conn,
            "a",
            &sid,
            &mid,
            "shell_exec",
            r#"{"command":"echo hi"}"#,
            &|_name, _args| panic!("executor should not be called"),
            None,
        )
        .unwrap();

        assert!(!result.success, "tool:shell_* pattern should block shell_exec");
        assert!(result.output.contains("shell blocked"));
    }
}
