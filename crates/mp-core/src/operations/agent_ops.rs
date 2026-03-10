use rusqlite::Connection;
use super::{AuditMeta, OperationRequest, OperationResponse, denied_response, evaluate_policy_with_request_context, fail_response, policy_meta};

pub(super) fn op_agent_create(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
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
    let tags = req.args["tags"].as_str();

    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "create",
            resource: crate::policy::resource::AGENT,
            sql_content: None,
            channel: req.actor.channel.as_deref(),
            arguments: None,
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
        "INSERT INTO agents (id, name, persona, tags, trust_level, llm_provider, llm_model, db_path, sync_enabled, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 1, ?9)",
        rusqlite::params![id, name, persona, tags, trust_level, llm_provider, llm_model, db_path.to_string_lossy(), now],
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

pub(super) fn op_agent_delete(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
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
            resource: crate::policy::resource::AGENT,
            sql_content: None,
            channel: req.actor.channel.as_deref(),
            arguments: None,
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

pub(super) fn op_agent_config(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
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
            resource: crate::policy::resource::AGENT,
            sql_content: None,
            channel: req.actor.channel.as_deref(),
            arguments: None,
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
            meta_conn.execute(
                "UPDATE agents SET persona = ?1 WHERE name = ?2",
                rusqlite::params![value, name],
            )?;
        }
        "trust_level" => {
            meta_conn.execute(
                "UPDATE agents SET trust_level = ?1 WHERE name = ?2",
                rusqlite::params![value, name],
            )?;
        }
        "llm_provider" => {
            meta_conn.execute(
                "UPDATE agents SET llm_provider = ?1 WHERE name = ?2",
                rusqlite::params![value, name],
            )?;
        }
        "llm_model" => {
            meta_conn.execute(
                "UPDATE agents SET llm_model = ?1 WHERE name = ?2",
                rusqlite::params![value, name],
            )?;
        }
        "sync_enabled" => {
            let as_int = if value.eq_ignore_ascii_case("true") || value == "1" {
                1
            } else {
                0
            };
            meta_conn.execute(
                "UPDATE agents SET sync_enabled = ?1 WHERE name = ?2",
                rusqlite::params![as_int, name],
            )?;
        }
        "tags" => {
            meta_conn.execute(
                "UPDATE agents SET tags = ?1 WHERE name = ?2",
                rusqlite::params![value, name],
            )?;
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
