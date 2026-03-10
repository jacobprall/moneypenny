use rusqlite::Connection;
use super::{AuditMeta, OperationRequest, OperationResponse, denied_response, evaluate_policy_with_request_context, policy_meta};

pub(super) fn op_memory_search(
    conn: &Connection,
    req: &OperationRequest,
) -> anyhow::Result<OperationResponse> {
    let query = req.args["query"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'query'"))?;
    let agent_id = req.args["agent_id"].as_str().unwrap_or(&req.actor.agent_id);
    let limit = req.args["limit"].as_u64().unwrap_or(20) as usize;

    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "search",
            resource: crate::policy::resource::MEMORY,
            sql_content: None,
            channel: req.actor.channel.as_deref(),
            arguments: None,
        },
        req,
    )?;
    if matches!(decision.effect, crate::policy::Effect::Deny) {
        return Ok(denied_response(&decision));
    }

    let query_embedding = parse_query_embedding_blob(&req.args);
    let rows = crate::search::search(
        conn,
        query,
        agent_id,
        limit,
        None,
        query_embedding.as_deref(),
    )?;
    let retrieval_mode = if query_embedding.is_some() {
        "hybrid"
    } else {
        "text"
    };
    let data = rows
        .into_iter()
        .map(|r| {
            let store_label = match r.store {
                crate::search::Store::Facts => "facts",
                crate::search::Store::Knowledge => "knowledge",
                crate::search::Store::Log => "log",
            };
            let relevance = if r.score >= 0.8 {
                "high"
            } else if r.score >= 0.4 {
                "medium"
            } else {
                "low"
            };
            serde_json::json!({
                "id": r.id,
                "store": store_label,
                "content": r.content,
                "score": r.score,
                "relevance": relevance,
            })
        })
        .collect::<Vec<_>>();

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: format!(
            "{} result(s) via {retrieval_mode} search",
            data.len()
        ),
        data: serde_json::json!(data),
        policy: Some(policy_meta(&decision)),
        audit: AuditMeta { recorded: true },
    })
}

fn parse_query_embedding_blob(args: &serde_json::Value) -> Option<Vec<u8>> {
    // Optional private arg injected by runtime callers for hybrid retrieval.
    let arr = args
        .get("__query_embedding")
        .and_then(|v| v.as_array())
        .or_else(|| args.get("query_embedding").and_then(|v| v.as_array()))?;
    if arr.is_empty() || arr.len() > 8192 {
        return None;
    }

    let mut blob = Vec::with_capacity(arr.len() * std::mem::size_of::<f32>());
    for n in arr {
        let value = n.as_f64()?;
        if !value.is_finite() {
            return None;
        }
        blob.extend_from_slice(&(value as f32).to_le_bytes());
    }
    Some(blob)
}

pub(super) fn op_memory_fact_add(
    conn: &Connection,
    req: &OperationRequest,
) -> anyhow::Result<OperationResponse> {
    let content = req.args["content"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'content'"))?;
    let summary = req.args["summary"].as_str().unwrap_or(content);
    let pointer = req.args["pointer"].as_str().unwrap_or(content);
    let confidence = req.args["confidence"].as_f64().unwrap_or(1.0);
    let agent_id = req.args["agent_id"].as_str().unwrap_or(&req.actor.agent_id);
    let scope = req.args["scope"].as_str().unwrap_or("shared");
    let reason = req.args["reason"]
        .as_str()
        .or(Some("added via canonical operation"));
    let keywords = req.args["keywords"].as_str().map(|s| s.to_string());

    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "add",
            resource: crate::policy::resource::FACT,
            sql_content: None,
            channel: req.actor.channel.as_deref(),
            arguments: None,
        },
        req,
    )?;
    if matches!(decision.effect, crate::policy::Effect::Deny) {
        return Ok(denied_response(&decision));
    }

    let id = crate::store::facts::add(
        conn,
        &crate::store::facts::NewFact {
            agent_id: agent_id.to_string(),
            scope: scope.to_string(),
            content: content.to_string(),
            summary: summary.to_string(),
            pointer: pointer.to_string(),
            keywords,
            source_message_id: req.args["source_message_id"]
                .as_str()
                .map(|s| s.to_string()),
            confidence,
        },
        reason,
    )?;

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: "fact added".into(),
        data: serde_json::json!({
            "id": id,
            "agent_id": agent_id,
            "scope": scope,
            "summary": summary,
            "pointer": pointer,
            "confidence": confidence
        }),
        policy: Some(policy_meta(&decision)),
        audit: AuditMeta { recorded: true },
    })
}

pub(super) fn op_memory_fact_update(
    conn: &Connection,
    req: &OperationRequest,
) -> anyhow::Result<OperationResponse> {
    let fact_id = req.args["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'id'"))?;
    let content = req.args["content"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'content'"))?;
    let summary = req.args["summary"].as_str().unwrap_or(content);
    let pointer = req.args["pointer"].as_str().unwrap_or(content);
    let reason = req.args["reason"]
        .as_str()
        .or(Some("updated via canonical operation"));
    let source_message_id = req.args["source_message_id"].as_str();

    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "update",
            resource: crate::policy::resource::FACT,
            sql_content: None,
            channel: req.actor.channel.as_deref(),
            arguments: None,
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

    crate::store::facts::update(
        conn,
        fact_id,
        content,
        summary,
        pointer,
        reason,
        source_message_id,
    )?;

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: "fact updated".into(),
        data: serde_json::json!({
            "id": fact_id,
            "summary": summary,
            "pointer": pointer
        }),
        policy: Some(policy_meta(&decision)),
        audit: AuditMeta { recorded: true },
    })
}

pub(super) fn op_memory_fact_get(
    conn: &Connection,
    req: &OperationRequest,
) -> anyhow::Result<OperationResponse> {
    let fact_id = req.args["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'id'"))?;

    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "read",
            resource: crate::policy::resource::FACT,
            sql_content: None,
            channel: req.actor.channel.as_deref(),
            arguments: None,
        },
        req,
    )?;
    if matches!(decision.effect, crate::policy::Effect::Deny) {
        return Ok(denied_response(&decision));
    }

    let fact = match crate::store::facts::get(conn, fact_id)? {
        Some(v) => v,
        None => {
            return Ok(OperationResponse {
                ok: false,
                code: "not_found".into(),
                message: format!("fact '{fact_id}' not found"),
                data: serde_json::json!({}),
                policy: Some(policy_meta(&decision)),
                audit: AuditMeta { recorded: true },
            });
        }
    };

    let trust_level = resolve_fact_read_trust_level(conn, req);
    let fact_scope = fact
        .scope
        .parse::<crate::gateway::FactScope>()
        .unwrap_or(crate::gateway::FactScope::Shared);
    if !crate::gateway::can_access_fact(
        &trust_level,
        &fact_scope,
        &fact.agent_id,
        &req.actor.agent_id,
    ) {
        return Ok(OperationResponse {
            ok: false,
            code: "not_found".into(),
            message: format!("fact '{fact_id}' not found"),
            data: serde_json::json!({}),
            policy: Some(policy_meta(&decision)),
            audit: AuditMeta { recorded: true },
        });
    }

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: "fact loaded".into(),
        data: serde_json::json!({
            "id": fact.id,
            "agent_id": fact.agent_id,
            "content": fact.content,
            "summary": fact.summary,
            "pointer": fact.pointer,
            "scope": fact.scope,
            "confidence": fact.confidence,
            "version": fact.version,
            "context_compact": fact.context_compact,
            "compaction_level": fact.compaction_level,
            "last_compacted_at": fact.last_compacted_at,
            "superseded_at": fact.superseded_at,
        }),
        policy: Some(policy_meta(&decision)),
        audit: AuditMeta { recorded: true },
    })
}

fn resolve_fact_read_trust_level(conn: &Connection, req: &OperationRequest) -> String {
    // Private/internal override for platform-controlled calls.
    if let Some(level) = req
        .args
        .get("__trust_level")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return level.to_string();
    }

    // Backward-compatible public key (soft-deprecated).
    if let Some(level) = req
        .args
        .get("trust_level")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return level.to_string();
    }

    // Best-effort lookup from local metadata if available.
    conn.query_row(
        "SELECT trust_level FROM agents WHERE name = ?1 OR id = ?1 LIMIT 1",
        [req.actor.agent_id.as_str()],
        |r| r.get::<_, String>(0),
    )
    .unwrap_or_else(|_| "standard".to_string())
}

pub(super) fn op_memory_fact_compaction_reset(
    conn: &Connection,
    req: &OperationRequest,
) -> anyhow::Result<OperationResponse> {
    let fact_id = req.args["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'id'"))?;
    let reason = req.args["reason"]
        .as_str()
        .or(Some("compaction reset via canonical operation"));

    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "update",
            resource: crate::policy::resource::FACT,
            sql_content: None,
            channel: req.actor.channel.as_deref(),
            arguments: None,
        },
        req,
    )?;
    if matches!(decision.effect, crate::policy::Effect::Deny) {
        return Ok(denied_response(&decision));
    }

    let fact = match crate::store::facts::get(conn, fact_id)? {
        Some(v) => v,
        None => {
            return Ok(OperationResponse {
                ok: false,
                code: "not_found".into(),
                message: format!("fact '{fact_id}' not found"),
                data: serde_json::json!({}),
                policy: Some(policy_meta(&decision)),
                audit: AuditMeta { recorded: true },
            });
        }
    };

    crate::store::facts::reset_compaction(conn, fact_id)?;
    crate::store::facts::update(
        conn,
        fact_id,
        &fact.content,
        &fact.summary,
        &fact.pointer,
        reason,
        req.args["source_message_id"].as_str(),
    )?;

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: "fact compaction reset".into(),
        data: serde_json::json!({
            "id": fact_id,
            "compaction_level": 0,
            "context_compact": serde_json::Value::Null,
            "reset": true,
        }),
        policy: Some(policy_meta(&decision)),
        audit: AuditMeta { recorded: true },
    })
}

pub(super) fn op_skill_add(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
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
            resource: crate::policy::resource::SKILL,
            sql_content: None,
            channel: req.actor.channel.as_deref(),
            arguments: None,
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

pub(super) fn op_skill_promote(
    conn: &Connection,
    req: &OperationRequest,
) -> anyhow::Result<OperationResponse> {
    let skill_id = req.args["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'id'"))?;

    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "promote",
            resource: crate::policy::resource::SKILL,
            sql_content: None,
            channel: req.actor.channel.as_deref(),
            arguments: None,
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

pub(super) fn op_fact_delete(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let fact_id = req.args["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'id'"))?;
    let reason = req.args["reason"].as_str();

    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "delete",
            resource: crate::policy::resource::FACT,
            sql_content: None,
            channel: req.actor.channel.as_deref(),
            arguments: None,
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
