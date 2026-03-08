use rusqlite::Connection;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorContext {
    pub agent_id: String,
    pub tenant_id: Option<String>,
    pub user_id: Option<String>,
    pub channel: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OperationContext {
    pub session_id: Option<String>,
    pub trace_id: Option<String>,
    pub timestamp: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationRequest {
    pub op: String,
    pub op_version: Option<String>,
    pub request_id: Option<String>,
    pub idempotency_key: Option<String>,
    pub actor: ActorContext,
    pub context: OperationContext,
    pub args: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyMeta {
    pub effect: String,
    pub policy_id: Option<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditMeta {
    pub recorded: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationResponse {
    pub ok: bool,
    pub code: String,
    pub message: String,
    pub data: serde_json::Value,
    pub policy: Option<PolicyMeta>,
    pub audit: AuditMeta,
}

pub fn execute(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    if let Some(mut aborted) = run_pre_hooks(req) {
        annotate_response_metadata(req, &mut aborted);
        return Ok(aborted);
    }

    let mut resp = dispatch_operation(conn, req)?;

    if is_policy_required(req.op.as_str()) && resp.ok && resp.policy.is_none() {
        let mut failed = fail_response(
            "policy_missing",
            format!("operation '{}' completed without policy metadata", req.op),
        );
        annotate_response_metadata(req, &mut failed);
        return Ok(failed);
    }

    run_post_hooks(req, &mut resp);
    annotate_response_metadata(req, &mut resp);
    Ok(resp)
}

fn dispatch_operation(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    match req.op.as_str() {
        "job.create" => op_job_create(conn, req),
        "job.list" => op_job_list(conn, req),
        "job.run" => op_job_run(conn, req),
        "job.pause" => op_job_pause(conn, req),
        "policy.add" => op_policy_add(conn, req),
        "knowledge.ingest" => op_knowledge_ingest(conn, req),
        "skill.add" => op_skill_add(conn, req),
        "skill.promote" => op_skill_promote(conn, req),
        "fact.delete" => op_fact_delete(conn, req),
        "agent.create" => op_agent_create(conn, req),
        "agent.delete" => op_agent_delete(conn, req),
        "agent.config" => op_agent_config(conn, req),
        "ingest.events" => op_ingest_events(conn, req),
        "ingest.status" => op_ingest_status(conn, req),
        "ingest.replay" => op_ingest_replay(conn, req),
        _ => Ok(fail_response("invalid_args", format!("unknown operation '{}'", req.op))),
    }
}

fn op_job_create(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let name = req.args["name"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'name'"))?;
    let schedule = req.args["schedule"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'schedule'"))?;
    let job_type = req.args["job_type"].as_str().unwrap_or("prompt");
    let description = req.args["description"].as_str().map(|s| s.to_string());
    let agent_id = req.args["agent_id"]
        .as_str()
        .map(|s| s.to_string())
        .unwrap_or_else(|| req.actor.agent_id.clone());
    let payload = match req.args.get("payload") {
        Some(v) if v.is_string() => v.as_str().unwrap_or("{}").to_string(),
        Some(v) => serde_json::to_string(v).unwrap_or_else(|_| "{}".to_string()),
        None => "{}".to_string(),
    };

    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "create",
            resource: "job",
            sql_content: None,
            channel: req.actor.channel.as_deref(),
        },
        req,
    )?;

    if matches!(decision.effect, crate::policy::Effect::Deny) {
        return Ok(denied_response(&decision));
    }

    let now = chrono::Utc::now().timestamp();
    let id = crate::scheduler::create_job(
        conn,
        &crate::scheduler::NewJob {
            agent_id: agent_id.clone(),
            name: name.to_string(),
            description,
            schedule: schedule.to_string(),
            next_run_at: now + 60,
            job_type: job_type.to_string(),
            payload,
            max_retries: req.args["max_retries"].as_i64(),
            retry_delay_ms: req.args["retry_delay_ms"].as_i64(),
            timeout_ms: req.args["timeout_ms"].as_i64(),
            overlap_policy: req.args["overlap_policy"].as_str().map(|s| s.to_string()),
        },
    )?;

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: "job created".into(),
        data: serde_json::json!({
            "id": id,
            "name": name,
            "schedule": schedule,
            "job_type": job_type,
            "agent_id": agent_id
        }),
        policy: Some(policy_meta(&decision)),
        audit: AuditMeta { recorded: true },
    })
}

fn op_job_list(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let requested_agent = req.args["agent_id"].as_str().map(|s| s.to_string());
    let agent_id = requested_agent.as_deref().unwrap_or(&req.actor.agent_id);

    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "list",
            resource: "job",
            sql_content: None,
            channel: req.actor.channel.as_deref(),
        },
        req,
    )?;

    if matches!(decision.effect, crate::policy::Effect::Deny) {
        return Ok(denied_response(&decision));
    }

    let jobs = crate::scheduler::list_jobs(conn, Some(agent_id))?;
    let data: Vec<serde_json::Value> = jobs
        .iter()
        .map(|j| {
            serde_json::json!({
                "id": j.id,
                "name": j.name,
                "schedule": j.schedule,
                "status": j.status,
                "enabled": j.enabled,
                "job_type": j.job_type,
                "agent_id": j.agent_id
            })
        })
        .collect();

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: "jobs listed".into(),
        data: serde_json::json!(data),
        policy: Some(policy_meta(&decision)),
        audit: AuditMeta { recorded: true },
    })
}

fn op_job_run(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let job_id = req.args["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'id'"))?;

    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "run",
            resource: "job",
            sql_content: None,
            channel: req.actor.channel.as_deref(),
        },
        req,
    )?;
    if matches!(decision.effect, crate::policy::Effect::Deny) {
        return Ok(denied_response(&decision));
    }

    let job = match crate::scheduler::get_job(conn, job_id)? {
        Some(j) => j,
        None => {
            return Ok(OperationResponse {
                ok: false,
                code: "not_found".into(),
                message: format!("job '{job_id}' not found"),
                data: serde_json::json!({}),
                policy: Some(policy_meta(&decision)),
                audit: AuditMeta { recorded: true },
            })
        }
    };

    let run = crate::scheduler::dispatch_job(conn, &job, &|j| {
        Ok(format!("Manual trigger of {}", j.name))
    })?;

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: "job run completed".into(),
        data: serde_json::json!({
            "job_id": job.id,
            "run_id": run.id,
            "status": run.status,
            "result": run.result
        }),
        policy: Some(policy_meta(&decision)),
        audit: AuditMeta { recorded: true },
    })
}

fn op_job_pause(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let job_id = req.args["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'id'"))?;

    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "pause",
            resource: "job",
            sql_content: None,
            channel: req.actor.channel.as_deref(),
        },
        req,
    )?;
    if matches!(decision.effect, crate::policy::Effect::Deny) {
        return Ok(denied_response(&decision));
    }

    crate::scheduler::pause_job(conn, job_id)?;

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: "job paused".into(),
        data: serde_json::json!({
            "id": job_id,
            "status": "paused"
        }),
        policy: Some(policy_meta(&decision)),
        audit: AuditMeta { recorded: true },
    })
}

fn op_policy_add(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let name = req.args["name"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'name'"))?;
    let effect = req.args["effect"].as_str().unwrap_or("deny");
    let actor_pattern = req.args["actor_pattern"].as_str();
    let action_pattern = req.args["action_pattern"].as_str();
    let resource_pattern = req.args["resource_pattern"].as_str();
    let message = req.args["message"].as_str();

    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "add",
            resource: "policy",
            sql_content: None,
            channel: req.actor.channel.as_deref(),
        },
        req,
    )?;
    if matches!(decision.effect, crate::policy::Effect::Deny) {
        return Ok(denied_response(&decision));
    }

    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp();
    conn.execute(
        "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, message, created_at)
         VALUES (?1, ?2, 0, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![id, name, effect, actor_pattern, action_pattern, resource_pattern, message, now],
    )?;

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: "policy added".into(),
        data: serde_json::json!({
            "id": id,
            "name": name,
            "effect": effect
        }),
        policy: Some(policy_meta(&decision)),
        audit: AuditMeta { recorded: true },
    })
}

fn op_knowledge_ingest(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let content = req.args["content"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'content'"))?;
    let path = req.args["path"].as_str();
    let title = req.args["title"].as_str();
    let metadata = req.args["metadata"].as_str();

    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "ingest",
            resource: "knowledge",
            sql_content: None,
            channel: req.actor.channel.as_deref(),
        },
        req,
    )?;
    if matches!(decision.effect, crate::policy::Effect::Deny) {
        return Ok(denied_response(&decision));
    }

    let (doc_id, chunk_count) =
        crate::store::knowledge::ingest(conn, path, title, content, metadata)?;

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: "knowledge ingested".into(),
        data: serde_json::json!({
            "document_id": doc_id,
            "chunks_created": chunk_count
        }),
        policy: Some(policy_meta(&decision)),
        audit: AuditMeta { recorded: true },
    })
}

fn op_skill_add(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let name = req.args["name"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'name'"))?;
    let content = req.args["content"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'content'"))?;
    let description = req.args["description"]
        .as_str()
        .unwrap_or("Skill added via canonical operation");
    let tool_id = req.args["tool_id"].as_str();

    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "add",
            resource: "skill",
            sql_content: None,
            channel: req.actor.channel.as_deref(),
        },
        req,
    )?;
    if matches!(decision.effect, crate::policy::Effect::Deny) {
        return Ok(denied_response(&decision));
    }

    let id = crate::store::knowledge::add_skill(conn, name, description, content, tool_id)?;
    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: "skill added".into(),
        data: serde_json::json!({
            "id": id,
            "name": name,
        }),
        policy: Some(policy_meta(&decision)),
        audit: AuditMeta { recorded: true },
    })
}

fn op_skill_promote(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let skill_id = req.args["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'id'"))?;

    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "promote",
            resource: "skill",
            sql_content: None,
            channel: req.actor.channel.as_deref(),
        },
        req,
    )?;
    if matches!(decision.effect, crate::policy::Effect::Deny) {
        return Ok(denied_response(&decision));
    }

    if crate::store::knowledge::get_skill(conn, skill_id)?.is_none() {
        return Ok(OperationResponse {
            ok: false,
            code: "not_found".into(),
            message: format!("skill '{skill_id}' not found"),
            data: serde_json::json!({}),
            policy: Some(policy_meta(&decision)),
            audit: AuditMeta { recorded: true },
        });
    }

    crate::store::knowledge::promote_skill(conn, skill_id)?;
    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: "skill promoted".into(),
        data: serde_json::json!({
            "id": skill_id,
            "promoted": true,
        }),
        policy: Some(policy_meta(&decision)),
        audit: AuditMeta { recorded: true },
    })
}

fn op_fact_delete(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let fact_id = req.args["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'id'"))?;
    let reason = req.args["reason"].as_str();

    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "delete",
            resource: "fact",
            sql_content: None,
            channel: req.actor.channel.as_deref(),
        },
        req,
    )?;
    if matches!(decision.effect, crate::policy::Effect::Deny) {
        return Ok(denied_response(&decision));
    }

    if crate::store::facts::get(conn, fact_id)?.is_none() {
        return Ok(OperationResponse {
            ok: false,
            code: "not_found".into(),
            message: format!("fact '{fact_id}' not found"),
            data: serde_json::json!({}),
            policy: Some(policy_meta(&decision)),
            audit: AuditMeta { recorded: true },
        });
    }

    crate::store::facts::delete(conn, fact_id, reason)?;
    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: "fact deleted".into(),
        data: serde_json::json!({
            "id": fact_id,
            "deleted": true
        }),
        policy: Some(policy_meta(&decision)),
        audit: AuditMeta { recorded: true },
    })
}

fn op_agent_create(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let name = req.args["name"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'name'"))?;
    let metadata_db_path = req.args["metadata_db_path"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'metadata_db_path'"))?;
    let agent_db_path = req.args["agent_db_path"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'agent_db_path'"))?;
    let trust_level = req.args["trust_level"].as_str().unwrap_or("standard");
    let llm_provider = req.args["llm_provider"].as_str().unwrap_or("local");
    let llm_model = req.args["llm_model"].as_str();
    let persona = req.args["persona"].as_str();

    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "create",
            resource: "agent",
            sql_content: None,
            channel: req.actor.channel.as_deref(),
        },
        req,
    )?;
    if matches!(decision.effect, crate::policy::Effect::Deny) {
        return Ok(denied_response(&decision));
    }

    let meta_conn = crate::db::open(std::path::Path::new(metadata_db_path))?;
    crate::schema::init_metadata_db(&meta_conn)?;
    if crate::gateway::get_agent(&meta_conn, name)?.is_some() {
        return Ok(OperationResponse {
            ok: false,
            code: "already_exists".into(),
            message: format!("agent '{name}' already exists"),
            data: serde_json::json!({}),
            policy: Some(policy_meta(&decision)),
            audit: AuditMeta { recorded: true },
        });
    }

    let db_path = std::path::PathBuf::from(agent_db_path);
    if let Some(parent) = db_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let agent_conn = crate::db::open(&db_path)?;
    crate::schema::init_agent_db(&agent_conn)?;
    crate::tools::registry::register_builtins(&agent_conn)?;
    crate::tools::registry::register_runtime_skills(&agent_conn)?;

    let now = chrono::Utc::now().timestamp();
    let id = uuid::Uuid::new_v4().to_string();
    meta_conn.execute(
        "INSERT INTO agents (id, name, persona, trust_level, llm_provider, llm_model, db_path, sync_enabled, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, ?8)",
        rusqlite::params![id, name, persona, trust_level, llm_provider, llm_model, db_path.to_string_lossy(), now],
    )?;

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: "agent created".into(),
        data: serde_json::json!({
            "id": id,
            "name": name,
            "db_path": db_path.to_string_lossy(),
        }),
        policy: Some(policy_meta(&decision)),
        audit: AuditMeta { recorded: true },
    })
}

fn op_agent_delete(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let name = req.args["name"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'name'"))?;
    let metadata_db_path = req.args["metadata_db_path"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'metadata_db_path'"))?;

    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "delete",
            resource: "agent",
            sql_content: None,
            channel: req.actor.channel.as_deref(),
        },
        req,
    )?;
    if matches!(decision.effect, crate::policy::Effect::Deny) {
        return Ok(denied_response(&decision));
    }

    let meta_conn = crate::db::open(std::path::Path::new(metadata_db_path))?;
    crate::schema::init_metadata_db(&meta_conn)?;
    let existing = match crate::gateway::get_agent(&meta_conn, name)? {
        Some(v) => v,
        None => {
            return Ok(OperationResponse {
                ok: false,
                code: "not_found".into(),
                message: format!("agent '{name}' not found"),
                data: serde_json::json!({}),
                policy: Some(policy_meta(&decision)),
                audit: AuditMeta { recorded: true },
            });
        }
    };

    meta_conn.execute("DELETE FROM agents WHERE name = ?1", [name])?;
    let _ = std::fs::remove_file(&existing.db_path);

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: "agent deleted".into(),
        data: serde_json::json!({
            "name": name,
            "db_path": existing.db_path,
        }),
        policy: Some(policy_meta(&decision)),
        audit: AuditMeta { recorded: true },
    })
}

fn op_agent_config(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let name = req.args["name"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'name'"))?;
    let key = req.args["key"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'key'"))?;
    let value = req.args["value"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'value'"))?;
    let metadata_db_path = req.args["metadata_db_path"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'metadata_db_path'"))?;

    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "config",
            resource: "agent",
            sql_content: None,
            channel: req.actor.channel.as_deref(),
        },
        req,
    )?;
    if matches!(decision.effect, crate::policy::Effect::Deny) {
        return Ok(denied_response(&decision));
    }

    let meta_conn = crate::db::open(std::path::Path::new(metadata_db_path))?;
    crate::schema::init_metadata_db(&meta_conn)?;
    if crate::gateway::get_agent(&meta_conn, name)?.is_none() {
        return Ok(OperationResponse {
            ok: false,
            code: "not_found".into(),
            message: format!("agent '{name}' not found"),
            data: serde_json::json!({}),
            policy: Some(policy_meta(&decision)),
            audit: AuditMeta { recorded: true },
        });
    }

    match key {
        "persona" => {
            meta_conn.execute("UPDATE agents SET persona = ?1 WHERE name = ?2", rusqlite::params![value, name])?;
        }
        "trust_level" => {
            meta_conn.execute("UPDATE agents SET trust_level = ?1 WHERE name = ?2", rusqlite::params![value, name])?;
        }
        "llm_provider" => {
            meta_conn.execute("UPDATE agents SET llm_provider = ?1 WHERE name = ?2", rusqlite::params![value, name])?;
        }
        "llm_model" => {
            meta_conn.execute("UPDATE agents SET llm_model = ?1 WHERE name = ?2", rusqlite::params![value, name])?;
        }
        "sync_enabled" => {
            let as_int = if value.eq_ignore_ascii_case("true") || value == "1" { 1 } else { 0 };
            meta_conn.execute("UPDATE agents SET sync_enabled = ?1 WHERE name = ?2", rusqlite::params![as_int, name])?;
        }
        _ => {
            return Ok(fail_response(
                "invalid_args",
                format!("unsupported config key '{}'", key),
            ));
        }
    }

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: "agent config updated".into(),
        data: serde_json::json!({
            "name": name,
            "key": key,
            "value": value,
        }),
        policy: Some(policy_meta(&decision)),
        audit: AuditMeta { recorded: true },
    })
}

fn op_ingest_events(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let source = req.args["source"].as_str().unwrap_or("openclaw");
    let file_path = req.args["file_path"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'file_path'"))?;
    let replay = req.args["replay"].as_bool().unwrap_or(false);

    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "ingest",
            resource: "events",
            sql_content: None,
            channel: req.actor.channel.as_deref(),
        },
        req,
    )?;
    if matches!(decision.effect, crate::policy::Effect::Deny) {
        return Ok(denied_response(&decision));
    }

    let summary = crate::ingest::ingest_jsonl_file(
        conn,
        source,
        std::path::Path::new(file_path),
        replay,
        &req.actor.agent_id,
    )?;
    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: "events ingested".into(),
        data: serde_json::json!({
            "run_id": summary.run_id,
            "source": summary.source,
            "file_path": summary.file_path,
            "from_line": summary.from_line,
            "to_line": summary.to_line,
            "processed_count": summary.processed_count,
            "inserted_count": summary.inserted_count,
            "deduped_count": summary.deduped_count,
            "projected_count": summary.projected_count,
            "error_count": summary.error_count,
        }),
        policy: Some(policy_meta(&decision)),
        audit: AuditMeta { recorded: true },
    })
}

fn op_ingest_status(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let source = req.args["source"].as_str();
    let limit = req.args["limit"].as_u64().unwrap_or(20) as usize;
    let rows = crate::ingest::recent_runs(conn, source, limit)?;
    let data = rows
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "id": r.id,
                "source": r.source,
                "file_path": r.file_path,
                "from_line": r.from_line,
                "to_line": r.to_line,
                "processed_count": r.processed_count,
                "inserted_count": r.inserted_count,
                "deduped_count": r.deduped_count,
                "projected_count": r.projected_count,
                "error_count": r.error_count,
                "status": r.status,
                "started_at": r.started_at,
                "finished_at": r.finished_at,
            })
        })
        .collect::<Vec<_>>();
    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: "ingest runs listed".into(),
        data: serde_json::json!(data),
        policy: None,
        audit: AuditMeta { recorded: true },
    })
}

fn op_ingest_replay(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let run_id = req.args["run_id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'run_id'"))?;
    let dry_run = req.args["dry_run"].as_bool().unwrap_or(false);
    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "ingest",
            resource: "events",
            sql_content: None,
            channel: req.actor.channel.as_deref(),
        },
        req,
    )?;
    if matches!(decision.effect, crate::policy::Effect::Deny) {
        return Ok(denied_response(&decision));
    }
    if dry_run {
        let preview = crate::ingest::replay_run_preflight(conn, run_id)?;
        return Ok(OperationResponse {
            ok: true,
            code: "ok".into(),
            message: "ingest replay preflight".into(),
            data: serde_json::json!({
                "run_id": run_id,
                "source": preview.source,
                "file_path": preview.file_path,
                "from_line": preview.from_line,
                "to_line": preview.to_line,
                "processed_count": preview.processed_count,
                "would_insert_count": preview.would_insert_count,
                "would_dedupe_count": preview.would_dedupe_count,
                "parse_error_count": preview.parse_error_count,
            }),
            policy: Some(policy_meta(&decision)),
            audit: AuditMeta { recorded: true },
        });
    }
    let summary = crate::ingest::replay_run(conn, run_id, &req.actor.agent_id)?;
    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: "ingest run replayed".into(),
        data: serde_json::json!({
            "run_id": summary.run_id,
            "source": summary.source,
            "file_path": summary.file_path,
            "from_line": summary.from_line,
            "to_line": summary.to_line,
            "processed_count": summary.processed_count,
            "inserted_count": summary.inserted_count,
            "deduped_count": summary.deduped_count,
            "projected_count": summary.projected_count,
            "error_count": summary.error_count,
        }),
        policy: Some(policy_meta(&decision)),
        audit: AuditMeta { recorded: true },
    })
}

fn policy_meta(decision: &crate::policy::PolicyDecision) -> PolicyMeta {
    let effect = match decision.effect {
        crate::policy::Effect::Allow => "allow",
        crate::policy::Effect::Deny => "deny",
        crate::policy::Effect::Audit => "audit",
    }
    .to_string();
    PolicyMeta {
        effect,
        policy_id: decision.policy_id.clone(),
        reason: decision.reason.clone(),
    }
}

fn denied_response(decision: &crate::policy::PolicyDecision) -> OperationResponse {
    OperationResponse {
        ok: false,
        code: "policy_denied".into(),
        message: decision
            .reason
            .clone()
            .unwrap_or_else(|| "operation denied by policy".into()),
        data: serde_json::json!({}),
        policy: Some(policy_meta(decision)),
        audit: AuditMeta { recorded: true },
    }
}

fn fail_response(code: &str, message: String) -> OperationResponse {
    OperationResponse {
        ok: false,
        code: code.into(),
        message,
        data: serde_json::json!({}),
        policy: None,
        audit: AuditMeta { recorded: false },
    }
}

fn is_policy_required(op: &str) -> bool {
    matches!(
        op,
        "job.create"
            | "job.list"
            | "job.run"
            | "job.pause"
            | "policy.add"
            | "knowledge.ingest"
            | "skill.add"
            | "skill.promote"
            | "fact.delete"
            | "agent.create"
            | "agent.delete"
            | "agent.config"
            | "ingest.events"
            | "ingest.replay"
    )
}

fn run_pre_hooks(req: &OperationRequest) -> Option<OperationResponse> {
    // Hard guardrail: avoid oversized operation envelopes overwhelming runtime.
    let args_size = req.args.to_string().len();
    if args_size > 2_000_000 {
        return Some(fail_response(
            "invalid_args",
            format!("operation args too large: {} bytes", args_size),
        ));
    }
    None
}

fn run_post_hooks(_req: &OperationRequest, resp: &mut OperationResponse) {
    // Standardized post-hook: redact accidental secret-like output text.
    resp.message = crate::store::redact::redact(&resp.message);
}

fn annotate_response_metadata(req: &OperationRequest, resp: &mut OperationResponse) {
    let correlation_id = req
        .request_id
        .as_deref()
        .or(req.context.trace_id.as_deref())
        .unwrap_or("")
        .to_string();
    let idempotency_key = req.idempotency_key.clone();
    let idempotency_state = if idempotency_key.is_some() {
        "provided_unenforced"
    } else {
        "not_provided"
    };
    let meta = serde_json::json!({
        "op": req.op,
        "op_version": req.op_version,
        "correlation_id": correlation_id,
        "idempotency_key": idempotency_key,
        "idempotency_state": idempotency_state,
    });

    match &mut resp.data {
        serde_json::Value::Object(map) => {
            map.insert("_meta".into(), meta);
        }
        other => {
            let prior = other.take();
            resp.data = serde_json::json!({
                "result": prior,
                "_meta": meta
            });
        }
    }
}

fn evaluate_policy_with_request_context(
    conn: &Connection,
    policy_req: &crate::policy::PolicyRequest,
    req: &OperationRequest,
) -> anyhow::Result<crate::policy::PolicyDecision> {
    let audit = crate::policy::PolicyAuditContext {
        session_id: req.context.session_id.as_deref(),
        correlation_id: req.request_id.as_deref().or(req.context.trace_id.as_deref()),
    };
    crate::policy::evaluate_with_audit(conn, policy_req, &audit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn setup() -> Connection {
        let conn = crate::db::open_memory().unwrap();
        crate::schema::init_agent_db(&conn).unwrap();
        conn
    }

    fn temp_db_path(prefix: &str) -> PathBuf {
        std::env::temp_dir().join(format!("mp-{prefix}-{}.db", uuid::Uuid::new_v4()))
    }

    fn temp_jsonl_file(prefix: &str, lines: &[&str]) -> PathBuf {
        let path = std::env::temp_dir().join(format!("mp-{prefix}-{}.jsonl", uuid::Uuid::new_v4()));
        let content = lines.join("\n");
        std::fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn job_create_succeeds_with_allow_policy() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, created_at)
             VALUES ('allow-all', 'allow all', 10, 'allow', '*', '*', '*', 1)",
            [],
        )
        .unwrap();

        let req = OperationRequest {
            op: "job.create".into(),
            op_version: Some("v1".into()),
            request_id: None,
            idempotency_key: Some("key-1".into()),
            actor: ActorContext {
                agent_id: "main".into(),
                tenant_id: None,
                user_id: None,
                channel: Some("cli".into()),
            },
            context: OperationContext::default(),
            args: serde_json::json!({
                "name": "daily",
                "schedule": "0 9 * * *",
                "job_type": "prompt",
                "payload": "{\"message\":\"hello\"}"
            }),
        };

        let resp = execute(&conn, &req).unwrap();
        assert!(resp.ok);
        assert_eq!(resp.code, "ok");
        assert_eq!(resp.data["name"], "daily");
    }

    #[test]
    fn job_list_returns_jobs() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, created_at)
             VALUES ('allow-all', 'allow all', 10, 'allow', '*', '*', '*', 1)",
            [],
        )
        .unwrap();

        let id = crate::scheduler::create_job(
            &conn,
            &crate::scheduler::NewJob {
                agent_id: "main".into(),
                name: "nightly".into(),
                description: None,
                schedule: "0 1 * * *".into(),
                next_run_at: chrono::Utc::now().timestamp() + 60,
                job_type: "prompt".into(),
                payload: "{}".into(),
                max_retries: None,
                retry_delay_ms: None,
                timeout_ms: None,
                overlap_policy: None,
            },
        )
        .unwrap();

        let req = OperationRequest {
            op: "job.list".into(),
            op_version: Some("v1".into()),
            request_id: None,
            idempotency_key: None,
            actor: ActorContext {
                agent_id: "main".into(),
                tenant_id: None,
                user_id: None,
                channel: Some("cli".into()),
            },
            context: OperationContext::default(),
            args: serde_json::json!({}),
        };
        let resp = execute(&conn, &req).unwrap();
        assert!(resp.ok);
        let rows = resp.data.as_array().unwrap();
        assert!(rows.iter().any(|r| r["id"] == id));
    }

    #[test]
    fn job_create_denied_by_policy() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, message, created_at)
             VALUES ('deny-job-create', 'deny create', 100, 'deny', '*', 'create', 'job', 'blocked', 1)",
            [],
        )
        .unwrap();

        let req = OperationRequest {
            op: "job.create".into(),
            op_version: Some("v1".into()),
            request_id: None,
            idempotency_key: None,
            actor: ActorContext {
                agent_id: "main".into(),
                tenant_id: None,
                user_id: None,
                channel: Some("cli".into()),
            },
            context: OperationContext::default(),
            args: serde_json::json!({
                "name": "blocked-job",
                "schedule": "* * * * *"
            }),
        };

        let resp = execute(&conn, &req).unwrap();
        assert!(!resp.ok);
        assert_eq!(resp.code, "policy_denied");
    }

    #[test]
    fn job_pause_updates_status() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, created_at)
             VALUES ('allow-all', 'allow all', 10, 'allow', '*', '*', '*', 1)",
            [],
        )
        .unwrap();

        let job_id = crate::scheduler::create_job(
            &conn,
            &crate::scheduler::NewJob {
                agent_id: "main".into(),
                name: "pause-me".into(),
                description: None,
                schedule: "* * * * *".into(),
                next_run_at: chrono::Utc::now().timestamp() + 60,
                job_type: "prompt".into(),
                payload: "{}".into(),
                max_retries: None,
                retry_delay_ms: None,
                timeout_ms: None,
                overlap_policy: None,
            },
        )
        .unwrap();

        let req = OperationRequest {
            op: "job.pause".into(),
            op_version: Some("v1".into()),
            request_id: None,
            idempotency_key: None,
            actor: ActorContext {
                agent_id: "main".into(),
                tenant_id: None,
                user_id: None,
                channel: Some("cli".into()),
            },
            context: OperationContext::default(),
            args: serde_json::json!({ "id": job_id }),
        };

        let resp = execute(&conn, &req).unwrap();
        assert!(resp.ok);
        let paused = crate::scheduler::get_job(&conn, resp.data["id"].as_str().unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(paused.status, "paused");
    }

    #[test]
    fn job_run_not_found() {
        let conn = setup();
        let req = OperationRequest {
            op: "job.run".into(),
            op_version: Some("v1".into()),
            request_id: None,
            idempotency_key: None,
            actor: ActorContext {
                agent_id: "main".into(),
                tenant_id: None,
                user_id: None,
                channel: Some("cli".into()),
            },
            context: OperationContext::default(),
            args: serde_json::json!({ "id": "missing" }),
        };

        let resp = execute(&conn, &req).unwrap();
        assert!(!resp.ok);
        assert_eq!(resp.code, "not_found");
        assert!(resp.policy.is_some());
    }

    #[test]
    fn oversized_operation_args_are_rejected_by_pre_hook() {
        let conn = setup();
        let req = OperationRequest {
            op: "job.list".into(),
            op_version: Some("v1".into()),
            request_id: None,
            idempotency_key: None,
            actor: ActorContext {
                agent_id: "main".into(),
                tenant_id: None,
                user_id: None,
                channel: Some("cli".into()),
            },
            context: OperationContext::default(),
            args: serde_json::json!({
                "blob": "x".repeat(2_000_100)
            }),
        };
        let resp = execute(&conn, &req).unwrap();
        assert!(!resp.ok);
        assert_eq!(resp.code, "invalid_args");
        assert!(resp.message.contains("too large"));
    }

    #[test]
    fn response_contains_idempotency_metadata() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, created_at)
             VALUES ('allow-all', 'allow all', 10, 'allow', '*', '*', '*', 1)",
            [],
        )
        .unwrap();
        let req = OperationRequest {
            op: "job.list".into(),
            op_version: Some("v1".into()),
            request_id: Some("corr-123".into()),
            idempotency_key: Some("idem-123".into()),
            actor: ActorContext {
                agent_id: "main".into(),
                tenant_id: None,
                user_id: None,
                channel: Some("cli".into()),
            },
            context: OperationContext::default(),
            args: serde_json::json!({}),
        };
        let resp = execute(&conn, &req).unwrap();
        assert!(resp.ok);
        assert_eq!(resp.data["_meta"]["correlation_id"], "corr-123");
        assert_eq!(resp.data["_meta"]["idempotency_key"], "idem-123");
        assert_eq!(resp.data["_meta"]["idempotency_state"], "provided_unenforced");
    }

    #[test]
    fn policy_add_creates_rule() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, created_at)
             VALUES ('allow-all', 'allow all', 10, 'allow', '*', '*', '*', 1)",
            [],
        )
        .unwrap();

        let req = OperationRequest {
            op: "policy.add".into(),
            op_version: Some("v1".into()),
            request_id: None,
            idempotency_key: None,
            actor: ActorContext {
                agent_id: "main".into(),
                tenant_id: None,
                user_id: None,
                channel: Some("cli".into()),
            },
            context: OperationContext::default(),
            args: serde_json::json!({
                "name": "deny-shell",
                "effect": "deny",
                "actor_pattern": "*",
                "action_pattern": "call",
                "resource_pattern": "tool:shell_*",
                "message": "blocked"
            }),
        };

        let resp = execute(&conn, &req).unwrap();
        assert!(resp.ok);
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM policies WHERE name = 'deny-shell'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn knowledge_ingest_creates_document() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, created_at)
             VALUES ('allow-all', 'allow all', 10, 'allow', '*', '*', '*', 1)",
            [],
        )
        .unwrap();

        let req = OperationRequest {
            op: "knowledge.ingest".into(),
            op_version: Some("v1".into()),
            request_id: None,
            idempotency_key: None,
            actor: ActorContext {
                agent_id: "main".into(),
                tenant_id: None,
                user_id: None,
                channel: Some("cli".into()),
            },
            context: OperationContext::default(),
            args: serde_json::json!({
                "title": "Test",
                "content": "# Header\nBody"
            }),
        };

        let resp = execute(&conn, &req).unwrap();
        assert!(resp.ok);
        assert!(resp.data["document_id"].as_str().is_some());
    }

    #[test]
    fn skill_add_and_promote_work() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, created_at)
             VALUES ('allow-all', 'allow all', 10, 'allow', '*', '*', '*', 1)",
            [],
        )
        .unwrap();

        let add_req = OperationRequest {
            op: "skill.add".into(),
            op_version: Some("v1".into()),
            request_id: Some("corr-skill-add".into()),
            idempotency_key: None,
            actor: ActorContext {
                agent_id: "main".into(),
                tenant_id: None,
                user_id: None,
                channel: Some("cli".into()),
            },
            context: OperationContext::default(),
            args: serde_json::json!({
                "name": "SkillTest",
                "description": "test",
                "content": "Do the thing"
            }),
        };
        let add_resp = execute(&conn, &add_req).unwrap();
        assert!(add_resp.ok);
        let skill_id = add_resp.data["id"].as_str().unwrap().to_string();

        let promote_req = OperationRequest {
            op: "skill.promote".into(),
            op_version: Some("v1".into()),
            request_id: Some("corr-skill-promote".into()),
            idempotency_key: None,
            actor: ActorContext {
                agent_id: "main".into(),
                tenant_id: None,
                user_id: None,
                channel: Some("cli".into()),
            },
            context: OperationContext::default(),
            args: serde_json::json!({ "id": skill_id }),
        };
        let promote_resp = execute(&conn, &promote_req).unwrap();
        assert!(promote_resp.ok);
        assert_eq!(promote_resp.data["promoted"], true);
    }

    #[test]
    fn fact_delete_returns_not_found_for_missing_fact() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, created_at)
             VALUES ('allow-all', 'allow all', 10, 'allow', '*', '*', '*', 1)",
            [],
        )
        .unwrap();

        let req = OperationRequest {
            op: "fact.delete".into(),
            op_version: Some("v1".into()),
            request_id: Some("corr-fact-delete-missing".into()),
            idempotency_key: None,
            actor: ActorContext {
                agent_id: "main".into(),
                tenant_id: None,
                user_id: None,
                channel: Some("cli".into()),
            },
            context: OperationContext::default(),
            args: serde_json::json!({ "id": "missing-fact" }),
        };
        let resp = execute(&conn, &req).unwrap();
        assert!(!resp.ok);
        assert_eq!(resp.code, "not_found");
    }

    #[test]
    fn agent_create_config_delete_work() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, created_at)
             VALUES ('allow-all', 'allow all', 10, 'allow', '*', '*', '*', 1)",
            [],
        )
        .unwrap();

        let meta_path = temp_db_path("meta");
        let agent_path = temp_db_path("agent");
        let meta_path_s = meta_path.to_string_lossy().to_string();
        let agent_path_s = agent_path.to_string_lossy().to_string();

        let create_resp = execute(
            &conn,
            &OperationRequest {
                op: "agent.create".into(),
                op_version: Some("v1".into()),
                request_id: Some("corr-agent-create".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "name": "agent-x",
                    "metadata_db_path": meta_path_s,
                    "agent_db_path": agent_path_s,
                }),
            },
        )
        .unwrap();
        assert!(create_resp.ok);

        let config_resp = execute(
            &conn,
            &OperationRequest {
                op: "agent.config".into(),
                op_version: Some("v1".into()),
                request_id: Some("corr-agent-config".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "name": "agent-x",
                    "key": "trust_level",
                    "value": "elevated",
                    "metadata_db_path": meta_path.to_string_lossy().to_string(),
                }),
            },
        )
        .unwrap();
        assert!(config_resp.ok);

        let delete_resp = execute(
            &conn,
            &OperationRequest {
                op: "agent.delete".into(),
                op_version: Some("v1".into()),
                request_id: Some("corr-agent-delete".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "name": "agent-x",
                    "metadata_db_path": meta_path.to_string_lossy().to_string(),
                }),
            },
        )
        .unwrap();
        assert!(delete_resp.ok);

        let _ = std::fs::remove_file(meta_path);
        let _ = std::fs::remove_file(agent_path);
    }

    #[test]
    fn ingest_events_is_replay_safe() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, created_at)
             VALUES ('allow-all', 'allow all', 10, 'allow', '*', '*', '*', 1)",
            [],
        )
        .unwrap();
        let file = temp_jsonl_file(
            "ingest-events",
            &[
                r#"{"event_id":"e1","type":"session.state","session_id":"s1","timestamp":100}"#,
                r#"{"event_id":"e2","type":"message.processed","session_id":"s1","role":"assistant","content":"hello","timestamp":101}"#,
            ],
        );

        let first = execute(
            &conn,
            &OperationRequest {
                op: "ingest.events".into(),
                op_version: Some("v1".into()),
                request_id: Some("corr-ingest-first".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "source": "openclaw",
                    "file_path": file.to_string_lossy().to_string(),
                    "replay": true
                }),
            },
        )
        .unwrap();
        assert!(first.ok);
        assert_eq!(first.data["inserted_count"], 2);

        let second = execute(
            &conn,
            &OperationRequest {
                op: "ingest.events".into(),
                op_version: Some("v1".into()),
                request_id: Some("corr-ingest-second".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "source": "openclaw",
                    "file_path": file.to_string_lossy().to_string(),
                    "replay": true
                }),
            },
        )
        .unwrap();
        assert!(second.ok);
        assert_eq!(second.data["inserted_count"], 0);
        assert_eq!(second.data["deduped_count"], 2);

        let _ = std::fs::remove_file(file);
    }

    #[test]
    fn ingest_status_and_replay_work() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, created_at)
             VALUES ('allow-all', 'allow all', 10, 'allow', '*', '*', '*', 1)",
            [],
        )
        .unwrap();
        let file = temp_jsonl_file(
            "ingest-status-replay",
            &[
                r#"{"event_id":"sre1","type":"session.state","session_id":"sre-s1","timestamp":100}"#,
                r#"{"event_id":"sre2","type":"message.processed","session_id":"sre-s1","role":"assistant","content":"hello","timestamp":101}"#,
            ],
        );
        let ingest = execute(
            &conn,
            &OperationRequest {
                op: "ingest.events".into(),
                op_version: Some("v1".into()),
                request_id: Some("corr-ingest-status-source".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "source": "openclaw",
                    "file_path": file.to_string_lossy().to_string(),
                    "replay": true
                }),
            },
        )
        .unwrap();
        assert!(ingest.ok);

        let status = execute(
            &conn,
            &OperationRequest {
                op: "ingest.status".into(),
                op_version: Some("v1".into()),
                request_id: Some("corr-ingest-status".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "source": "openclaw",
                    "limit": 5
                }),
            },
        )
        .unwrap();
        assert!(status.ok);
        let rows = status.data.as_array().unwrap();
        assert!(!rows.is_empty());
        let first_run_id = rows[0]["id"].as_str().unwrap().to_string();

        let replay = execute(
            &conn,
            &OperationRequest {
                op: "ingest.replay".into(),
                op_version: Some("v1".into()),
                request_id: Some("corr-ingest-replay".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "run_id": first_run_id
                }),
            },
        )
        .unwrap();
        assert!(replay.ok);
        assert_eq!(replay.data["inserted_count"], 0);
        assert_eq!(replay.data["deduped_count"], 2);

        let preflight = execute(
            &conn,
            &OperationRequest {
                op: "ingest.replay".into(),
                op_version: Some("v1".into()),
                request_id: Some("corr-ingest-replay-dry".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "run_id": first_run_id,
                    "dry_run": true
                }),
            },
        )
        .unwrap();
        assert!(preflight.ok);
        assert_eq!(preflight.data["would_insert_count"], 0);
        assert_eq!(preflight.data["would_dedupe_count"], 2);

        let _ = std::fs::remove_file(file);
    }

    #[test]
    fn ingest_projects_priority_families() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, created_at)
             VALUES ('allow-all', 'allow all', 10, 'allow', '*', '*', '*', 1)",
            [],
        )
        .unwrap();
        let file = temp_jsonl_file(
            "ingest-priority-families",
            &[
                r#"{"event_id":"u1","type":"model.usage","session_id":"pf-s1","provider":"anthropic","model":"claude","channel":"cli","input_tokens":10,"output_tokens":5,"cost_usd":0.01,"duration_ms":120}"#,
                r#"{"event_id":"r1","type":"run.attempt","session_id":"pf-s1","status":"ok","output":"done","duration_ms":50}"#,
                r#"{"event_id":"w1","type":"webhook.error","session_id":"pf-s1","provider":"stripe","endpoint":"/hook","error":"bad sig"}"#,
            ],
        );

        let resp = execute(
            &conn,
            &OperationRequest {
                op: "ingest.events".into(),
                op_version: Some("v1".into()),
                request_id: Some("corr-ingest-priority".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "source": "openclaw",
                    "file_path": file.to_string_lossy().to_string(),
                    "replay": true
                }),
            },
        )
        .unwrap();
        assert!(resp.ok);
        assert_eq!(resp.data["projected_count"], 3);

        let usage_calls: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM tool_calls WHERE tool_name LIKE 'model.usage:%'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(usage_calls, 1);

        let run_calls: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM tool_calls WHERE tool_name = 'run.attempt'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(run_calls, 1);

        let webhook_audit: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM policy_audit WHERE action = 'webhook.error'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(webhook_audit, 1);

        let _ = std::fs::remove_file(file);
    }

    #[test]
    fn canonical_mutating_ops_return_policy_denied_consistently() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, message, created_at)
             VALUES ('deny-all', 'deny all', 100, 'deny', '*', '*', '*', 'blocked', 1)",
            [],
        )
        .unwrap();

        let ops = vec![
            OperationRequest {
                op: "job.create".into(),
                op_version: Some("v1".into()),
                request_id: Some("deny-job-create".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({ "name": "x", "schedule": "* * * * *" }),
            },
            OperationRequest {
                op: "policy.add".into(),
                op_version: Some("v1".into()),
                request_id: Some("deny-policy-add".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({ "name": "x", "effect": "deny" }),
            },
            OperationRequest {
                op: "knowledge.ingest".into(),
                op_version: Some("v1".into()),
                request_id: Some("deny-knowledge-ingest".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({ "content": "x" }),
            },
            OperationRequest {
                op: "skill.add".into(),
                op_version: Some("v1".into()),
                request_id: Some("deny-skill-add".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({ "name": "x", "content": "x" }),
            },
            OperationRequest {
                op: "fact.delete".into(),
                op_version: Some("v1".into()),
                request_id: Some("deny-fact-delete".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({ "id": "any" }),
            },
            OperationRequest {
                op: "agent.create".into(),
                op_version: Some("v1".into()),
                request_id: Some("deny-agent-create".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "name": "x",
                    "metadata_db_path": "/tmp/ignored.db",
                    "agent_db_path": "/tmp/ignored-agent.db"
                }),
            },
            OperationRequest {
                op: "agent.config".into(),
                op_version: Some("v1".into()),
                request_id: Some("deny-agent-config".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "name": "x",
                    "key": "persona",
                    "value": "x",
                    "metadata_db_path": "/tmp/ignored.db"
                }),
            },
            OperationRequest {
                op: "agent.delete".into(),
                op_version: Some("v1".into()),
                request_id: Some("deny-agent-delete".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "name": "x",
                    "metadata_db_path": "/tmp/ignored.db"
                }),
            },
            OperationRequest {
                op: "ingest.events".into(),
                op_version: Some("v1".into()),
                request_id: Some("deny-ingest-events".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "source": "openclaw",
                    "file_path": "/tmp/ignored.jsonl",
                    "replay": true
                }),
            },
            OperationRequest {
                op: "ingest.replay".into(),
                op_version: Some("v1".into()),
                request_id: Some("deny-ingest-replay".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "run_id": "any"
                }),
            },
        ];

        for req in ops {
            let resp = execute(&conn, &req).unwrap();
            assert!(!resp.ok);
            assert_eq!(resp.code, "policy_denied");
        }
    }

    #[test]
    fn canonical_ops_record_correlation_id_in_policy_audit() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, created_at)
             VALUES ('allow-all', 'allow all', 10, 'allow', '*', '*', '*', 1)",
            [],
        )
        .unwrap();

        let fact_id = crate::store::facts::add(
            &conn,
            &crate::store::facts::NewFact {
                agent_id: "main".into(),
                content: "fact".into(),
                summary: "fact".into(),
                pointer: "fact".into(),
                keywords: None,
                source_message_id: None,
                confidence: 1.0,
            },
            Some("seed"),
        )
        .unwrap();

        let reqs = vec![
            OperationRequest {
                op: "job.create".into(),
                op_version: Some("v1".into()),
                request_id: Some("corr-job-create".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "name": "corr-job",
                    "schedule": "* * * * *",
                    "job_type": "prompt",
                    "payload": "{}"
                }),
            },
            OperationRequest {
                op: "policy.add".into(),
                op_version: Some("v1".into()),
                request_id: Some("corr-policy-add".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "name": "corr-policy",
                    "effect": "audit",
                }),
            },
            OperationRequest {
                op: "knowledge.ingest".into(),
                op_version: Some("v1".into()),
                request_id: Some("corr-knowledge-ingest".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "title": "Correlation Test",
                    "content": "hello"
                }),
            },
            OperationRequest {
                op: "skill.add".into(),
                op_version: Some("v1".into()),
                request_id: Some("corr-skill-add-2".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "name": "corr-skill",
                    "content": "hello"
                }),
            },
            OperationRequest {
                op: "fact.delete".into(),
                op_version: Some("v1".into()),
                request_id: Some("corr-fact-delete".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "id": fact_id,
                    "reason": "test"
                }),
            },
        ];

        let meta_path = temp_db_path("meta-correlation");
        let agent_path = temp_db_path("agent-correlation");
        let meta_path_s = meta_path.to_string_lossy().to_string();
        let agent_path_s = agent_path.to_string_lossy().to_string();
        let create_agent_req = OperationRequest {
            op: "agent.create".into(),
            op_version: Some("v1".into()),
            request_id: Some("corr-agent-create-2".into()),
            idempotency_key: None,
            actor: ActorContext {
                agent_id: "main".into(),
                tenant_id: None,
                user_id: None,
                channel: Some("cli".into()),
            },
            context: OperationContext::default(),
            args: serde_json::json!({
                "name": "corr-agent",
                "metadata_db_path": meta_path_s,
                "agent_db_path": agent_path_s
            }),
        };
        let config_agent_req = OperationRequest {
            op: "agent.config".into(),
            op_version: Some("v1".into()),
            request_id: Some("corr-agent-config-2".into()),
            idempotency_key: None,
            actor: ActorContext {
                agent_id: "main".into(),
                tenant_id: None,
                user_id: None,
                channel: Some("cli".into()),
            },
            context: OperationContext::default(),
            args: serde_json::json!({
                "name": "corr-agent",
                "key": "persona",
                "value": "Hello",
                "metadata_db_path": meta_path.to_string_lossy().to_string()
            }),
        };
        let delete_agent_req = OperationRequest {
            op: "agent.delete".into(),
            op_version: Some("v1".into()),
            request_id: Some("corr-agent-delete-2".into()),
            idempotency_key: None,
            actor: ActorContext {
                agent_id: "main".into(),
                tenant_id: None,
                user_id: None,
                channel: Some("cli".into()),
            },
            context: OperationContext::default(),
            args: serde_json::json!({
                "name": "corr-agent",
                "metadata_db_path": meta_path.to_string_lossy().to_string()
            }),
        };
        let ingest_file = temp_jsonl_file(
            "ingest-correlation",
            &[r#"{"event_id":"corr-e1","type":"session.state","session_id":"corr-s1","timestamp":100}"#],
        );
        let ingest_req = OperationRequest {
            op: "ingest.events".into(),
            op_version: Some("v1".into()),
            request_id: Some("corr-ingest-events-2".into()),
            idempotency_key: None,
            actor: ActorContext {
                agent_id: "main".into(),
                tenant_id: None,
                user_id: None,
                channel: Some("cli".into()),
            },
            context: OperationContext::default(),
            args: serde_json::json!({
                "source": "openclaw",
                "file_path": ingest_file.to_string_lossy().to_string(),
                "replay": true
            }),
        };

        for req in reqs {
            let resp = execute(&conn, &req).unwrap();
            assert!(resp.ok, "operation {} should succeed", req.op);
            let corr = req.request_id.unwrap();
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM policy_audit WHERE correlation_id = ?1",
                    [corr],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(count, 1, "expected one policy audit row for correlation id");
        }

        for req in [create_agent_req, config_agent_req, ingest_req, delete_agent_req] {
            let resp = execute(&conn, &req).unwrap();
            assert!(resp.ok, "operation {} should succeed", req.op);
            let corr = req.request_id.unwrap();
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM policy_audit WHERE correlation_id = ?1",
                    [corr],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(count, 1, "expected one policy audit row for correlation id");
        }

        let _ = std::fs::remove_file(meta_path);
        let _ = std::fs::remove_file(agent_path);
        let _ = std::fs::remove_file(ingest_file);
    }
}
