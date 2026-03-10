use anyhow::Result;
use mp_core::config::Config;
use serde::Deserialize;
use std::path::Path;

pub fn resolve_agent<'a>(
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

pub fn open_agent_db(config: &Config, agent_name: &str) -> Result<rusqlite::Connection> {
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
        let _ = mp_core::schema::init_vector_indexes(&conn, agent.embedding.dimensions);
        if let Err(e) = mp_core::schema::init_sync_tables(&conn) {
            tracing::warn!(agent = agent_name, "sync table init warning: {e}");
        }
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

pub fn build_provider(
    agent: &mp_core::config::AgentConfig,
) -> Result<Box<dyn mp_llm::provider::LlmProvider>> {
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

pub fn build_embedding_provider(
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

pub fn build_embedding_provider_with_override(
    config: &Config,
    agent: &mp_core::config::AgentConfig,
    model_override: Option<&str>,
) -> Result<Box<dyn mp_llm::provider::EmbeddingProvider>> {
    let mut embed_cfg = agent.embedding.clone();
    if let Some(model) = model_override {
        embed_cfg.model = model.to_string();
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

pub fn embedding_model_id(agent: &mp_core::config::AgentConfig) -> String {
    mp_core::store::embedding::model_identity(
        &agent.embedding.provider,
        &agent.embedding.model,
        agent.embedding.dimensions,
    )
}

pub fn op_request(
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

pub fn resolve_or_create_session(
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

pub async fn embed_pending(
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

// ── Sidecar helpers ─────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct SidecarOperationInput {
    pub op: String,
    #[serde(default)]
    pub op_version: Option<String>,
    #[serde(default)]
    pub request_id: Option<String>,
    #[serde(default)]
    pub idempotency_key: Option<String>,
    #[serde(default)]
    pub actor: Option<mp_core::operations::ActorContext>,
    #[serde(default)]
    pub context: Option<mp_core::operations::OperationContext>,
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub tenant_id: Option<String>,
    #[serde(default)]
    pub user_id: Option<String>,
    #[serde(default)]
    pub channel: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub trace_id: Option<String>,
    #[serde(default = "default_sidecar_args")]
    pub args: serde_json::Value,
}

fn default_sidecar_args() -> serde_json::Value {
    serde_json::json!({})
}

pub fn sidecar_error_response(code: &str, message: impl Into<String>) -> serde_json::Value {
    serde_json::json!({
        "ok": false,
        "code": code,
        "message": message.into(),
        "data": {},
        "policy": null,
        "audit": { "recorded": false }
    })
}

pub fn build_sidecar_request(
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

// ── Small utilities ─────────────────────────────────────────────────

pub fn parse_duration_hours(s: &str) -> i64 {
    if let Some(d) = s.strip_suffix('d') {
        d.parse::<i64>().unwrap_or(7) * 24
    } else if let Some(h) = s.strip_suffix('h') {
        h.parse::<i64>().unwrap_or(24)
    } else {
        168
    }
}

pub fn normalize_embedding_target(value: &str) -> Option<&'static str> {
    match value.trim().to_ascii_lowercase().as_str() {
        "facts" | "fact" => Some("facts"),
        "messages" | "message" | "msg" => Some("messages"),
        "tool_calls" | "tool-calls" | "toolcalls" | "tool_call" => Some("tool_calls"),
        "policy_audit" | "policy-audit" | "policyaudit" | "policy" => Some("policy_audit"),
        "chunks" | "chunk" | "knowledge" => Some("chunks"),
        _ => None,
    }
}

pub fn toml_to_json(v: &toml::Value) -> serde_json::Value {
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

pub fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

pub fn sql_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

pub fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max { s } else { &s[..max] }
}

pub fn default_model_url(model_name: &str) -> Option<&'static str> {
    match model_name {
        "nomic-embed-text-v1.5" => Some(
            "https://huggingface.co/nomic-ai/nomic-embed-text-v1.5-GGUF/resolve/main/nomic-embed-text-v1.5.Q4_K_M.gguf",
        ),
        _ => None,
    }
}

pub async fn download_model(url: &str, dest: &Path) -> Result<()> {
    use crate::ui;
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

pub async fn ensure_embedding_models(config: &Config) {
    use crate::ui;
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

// ── Extraction & summarization ───────────────────────────────────────

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

pub async fn extract_facts(
    conn: &rusqlite::Connection,
    provider: &dyn mp_llm::provider::LlmProvider,
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

const SUMMARIZE_EVERY: usize = 20;
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

pub async fn maybe_summarize_session(
    conn: &rusqlite::Connection,
    provider: &dyn mp_llm::provider::LlmProvider,
    session_id: &str,
) {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM messages WHERE session_id = ?1",
            rusqlite::params![session_id],
            |r| r.get(0),
        )
        .unwrap_or(0);

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

// ── Bootstrap seed facts ────────────────────────────────────────────

pub fn seed_bootstrap_facts(conn: &rusqlite::Connection, agent_id: &str) {
    use mp_core::store::facts::{NewFact, add};

    let seeds: &[(&str, &str, &str, &str)] = &[
        (
            "Moneypenny: persistent memory, knowledge, policies, tools, extraction",
            "Moneypenny is an autonomous AI agent runtime where the database is the runtime. \
             It provides persistent long-term memory (facts), knowledge retrieval from ingested \
             documents, governance policies, scheduled jobs, and conversation history across sessions.",
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
