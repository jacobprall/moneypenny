use rusqlite::Connection;
use super::{AuditMeta, OperationRequest, OperationResponse, denied_response, evaluate_policy_with_request_context, policy_meta};

pub(super) fn op_activity_query(
    conn: &Connection,
    req: &OperationRequest,
) -> anyhow::Result<OperationResponse> {
    let limit = req.args["limit"].as_u64().unwrap_or(50).clamp(1, 500) as i64;
    let event = req.args["event"].as_str().map(|s| s.to_string());
    let action = req.args["action"].as_str().map(|s| s.to_string());
    let resource = req.args["resource"].as_str().map(|s| s.to_string());
    let agent_id = req.args["agent_id"].as_str().map(|s| s.to_string());
    let conversation_id = req.args["conversation_id"].as_str().map(|s| s.to_string());
    let query = req.args["query"].as_str().map(|q| format!("%{q}%"));
    let source = req.args["source"].as_str().unwrap_or("all");

    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "query",
            resource: crate::policy::resource::ACTIVITY,
            sql_content: None,
            channel: req.actor.channel.as_deref(),
            arguments: None,
        },
        req,
    )?;
    if matches!(decision.effect, crate::policy::Effect::Deny) {
        return Ok(denied_response(&decision));
    }

    let rows: Vec<serde_json::Value> = match source {
        "decisions" => {
            let mut stmt = conn.prepare(
                "SELECT id, policy_id, actor, action, resource, effect, reason, session_id, created_at
                 FROM policy_audit
                 WHERE (?1 IS NULL OR action = ?1)
                   AND (?2 IS NULL OR resource = ?2)
                   AND (?3 IS NULL OR actor = ?3)
                   AND (?4 IS NULL OR session_id = ?4)
                   AND (
                     ?5 IS NULL OR
                     reason LIKE ?5 OR actor LIKE ?5 OR action LIKE ?5 OR resource LIKE ?5
                   )
                 ORDER BY created_at DESC
                 LIMIT ?6",
            )?;
            stmt.query_map(
                rusqlite::params![action, resource, agent_id, conversation_id, query, limit],
                |r| {
                    Ok(serde_json::json!({
                        "source": "decisions",
                        "id": r.get::<_, String>(0)?,
                        "policy_id": r.get::<_, Option<String>>(1)?,
                        "actor": r.get::<_, String>(2)?,
                        "action": r.get::<_, String>(3)?,
                        "resource": r.get::<_, String>(4)?,
                        "effect": r.get::<_, String>(5)?,
                        "reason": r.get::<_, Option<String>>(6)?,
                        "session_id": r.get::<_, Option<String>>(7)?,
                        "created_at": r.get::<_, i64>(8)?,
                    }))
                },
            )?
            .collect::<Result<Vec<_>, _>>()?
        }
        "events" => {
            let mut stmt = conn.prepare(
                "SELECT id, agent_id, event, action, resource, detail, conversation_id, generation_id, duration_ms, created_at
                 FROM activity_log
                 WHERE (?1 IS NULL OR event = ?1)
                   AND (?2 IS NULL OR action = ?2)
                   AND (?3 IS NULL OR resource = ?3)
                   AND (?4 IS NULL OR agent_id = ?4)
                   AND (?5 IS NULL OR conversation_id = ?5)
                   AND (
                     ?6 IS NULL OR
                     detail LIKE ?6 OR event LIKE ?6 OR action LIKE ?6 OR resource LIKE ?6
                   )
                 ORDER BY created_at DESC
                 LIMIT ?7",
            )?;
            stmt.query_map(
                rusqlite::params![event, action, resource, agent_id, conversation_id, query, limit],
                |r| {
                    Ok(serde_json::json!({
                        "source": "events",
                        "id": r.get::<_, String>(0)?,
                        "agent_id": r.get::<_, String>(1)?,
                        "event": r.get::<_, String>(2)?,
                        "action": r.get::<_, String>(3)?,
                        "resource": r.get::<_, String>(4)?,
                        "detail": r.get::<_, String>(5)?,
                        "conversation_id": r.get::<_, String>(6)?,
                        "generation_id": r.get::<_, String>(7)?,
                        "duration_ms": r.get::<_, Option<i64>>(8)?,
                        "created_at": r.get::<_, i64>(9)?,
                    }))
                },
            )?
            .collect::<Result<Vec<_>, _>>()?
        }
        _ => {
            // "all" — interleave both tables, sorted by created_at desc
            let half = (limit / 2).max(1);
            let mut stmt_events = conn.prepare(
                "SELECT id, agent_id, event, action, resource, detail, conversation_id, duration_ms, created_at
                 FROM activity_log
                 WHERE (?1 IS NULL OR event = ?1)
                   AND (?2 IS NULL OR action = ?2)
                   AND (?3 IS NULL OR resource = ?3)
                   AND (?4 IS NULL OR agent_id = ?4)
                   AND (?5 IS NULL OR conversation_id = ?5)
                   AND (?6 IS NULL OR detail LIKE ?6 OR event LIKE ?6 OR action LIKE ?6 OR resource LIKE ?6)
                 ORDER BY created_at DESC
                 LIMIT ?7",
            )?;
            let events: Vec<serde_json::Value> = stmt_events
                .query_map(
                    rusqlite::params![event, action, resource, agent_id, conversation_id, query, half],
                    |r| {
                        Ok(serde_json::json!({
                            "source": "events",
                            "id": r.get::<_, String>(0)?,
                            "agent_id": r.get::<_, String>(1)?,
                            "event": r.get::<_, String>(2)?,
                            "action": r.get::<_, String>(3)?,
                            "resource": r.get::<_, String>(4)?,
                            "detail": r.get::<_, String>(5)?,
                            "conversation_id": r.get::<_, String>(6)?,
                            "duration_ms": r.get::<_, Option<i64>>(7)?,
                            "created_at": r.get::<_, i64>(8)?,
                        }))
                    },
                )?
                .collect::<Result<Vec<_>, _>>()?;

            let mut stmt_decisions = conn.prepare(
                "SELECT id, actor, action, resource, effect, reason, session_id, created_at
                 FROM policy_audit
                 WHERE (?1 IS NULL OR action = ?1)
                   AND (?2 IS NULL OR resource = ?2)
                   AND (?3 IS NULL OR actor = ?3)
                   AND (?4 IS NULL OR session_id = ?4)
                   AND (?5 IS NULL OR reason LIKE ?5 OR actor LIKE ?5 OR action LIKE ?5 OR resource LIKE ?5)
                 ORDER BY created_at DESC
                 LIMIT ?6",
            )?;
            let decisions: Vec<serde_json::Value> = stmt_decisions
                .query_map(
                    rusqlite::params![action, resource, agent_id, conversation_id, query, half],
                    |r| {
                        Ok(serde_json::json!({
                            "source": "decisions",
                            "id": r.get::<_, String>(0)?,
                            "actor": r.get::<_, String>(1)?,
                            "action": r.get::<_, String>(2)?,
                            "resource": r.get::<_, String>(3)?,
                            "effect": r.get::<_, String>(4)?,
                            "reason": r.get::<_, Option<String>>(5)?,
                            "session_id": r.get::<_, Option<String>>(6)?,
                            "created_at": r.get::<_, i64>(7)?,
                        }))
                    },
                )?
                .collect::<Result<Vec<_>, _>>()?;

            let mut merged = events;
            merged.extend(decisions);
            merged.sort_by(|a, b| {
                let ta = a["created_at"].as_i64().unwrap_or(0);
                let tb = b["created_at"].as_i64().unwrap_or(0);
                tb.cmp(&ta)
            });
            merged.truncate(limit as usize);
            merged
        }
    };

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: "activity query completed".into(),
        data: serde_json::json!(rows),
        policy: Some(policy_meta(&decision)),
        audit: AuditMeta { recorded: true },
    })
}

pub(super) fn op_audit_query(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let limit = req.args["limit"].as_u64().unwrap_or(50).clamp(1, 500) as i64;
    let effect = req.args["effect"].as_str().map(|s| s.to_string());
    let actor = req.args["actor"].as_str().map(|s| s.to_string());
    let action = req.args["action"].as_str().map(|s| s.to_string());
    let resource = req.args["resource"].as_str().map(|s| s.to_string());
    let session_id = req.args["session_id"].as_str().map(|s| s.to_string());
    let query = req.args["query"].as_str().map(|q| format!("%{q}%"));
    let since = req.args["since"]
        .as_i64()
        .or_else(|| req.args["from"].as_i64());
    let until = req.args["until"]
        .as_i64()
        .or_else(|| req.args["to"].as_i64());

    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "query",
            resource: crate::policy::resource::AUDIT,
            sql_content: None,
            channel: req.actor.channel.as_deref(),
            arguments: None,
        },
        req,
    )?;
    if matches!(decision.effect, crate::policy::Effect::Deny) {
        return Ok(denied_response(&decision));
    }

    let mut stmt = conn.prepare(
        "SELECT id, policy_id, actor, action, resource, effect, reason, correlation_id, session_id, idempotency_key, idempotency_state, created_at
         FROM policy_audit
         WHERE (?1 IS NULL OR effect = ?1)
           AND (?2 IS NULL OR actor = ?2)
           AND (?3 IS NULL OR action = ?3)
           AND (?4 IS NULL OR resource = ?4)
           AND (?5 IS NULL OR session_id = ?5)
           AND (
             ?6 IS NULL OR
             reason LIKE ?6 OR actor LIKE ?6 OR action LIKE ?6 OR resource LIKE ?6 OR correlation_id LIKE ?6
           )
           AND (?7 IS NULL OR created_at >= ?7)
           AND (?8 IS NULL OR created_at <= ?8)
         ORDER BY created_at DESC
         LIMIT ?9",
    )?;
    let rows = stmt
        .query_map(
            rusqlite::params![
                effect, actor, action, resource, session_id, query, since, until, limit
            ],
            |r| {
                Ok(serde_json::json!({
                    "id": r.get::<_, String>(0)?,
                    "policy_id": r.get::<_, Option<String>>(1)?,
                    "actor": r.get::<_, String>(2)?,
                    "action": r.get::<_, String>(3)?,
                    "resource": r.get::<_, String>(4)?,
                    "effect": r.get::<_, String>(5)?,
                    "reason": r.get::<_, Option<String>>(6)?,
                    "correlation_id": r.get::<_, Option<String>>(7)?,
                    "session_id": r.get::<_, Option<String>>(8)?,
                    "idempotency_key": r.get::<_, Option<String>>(9)?,
                    "idempotency_state": r.get::<_, Option<String>>(10)?,
                    "created_at": r.get::<_, i64>(11)?,
                }))
            },
        )?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: "audit query completed".into(),
        data: serde_json::json!(rows),
        policy: Some(policy_meta(&decision)),
        audit: AuditMeta { recorded: true },
    })
}

pub(super) fn op_audit_append(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let action = req.args["action"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'action'"))?;
    let resource = req.args["resource"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'resource'"))?;
    let effect = req.args["effect"].as_str().unwrap_or("audited");
    let actor = req.args["actor"].as_str().unwrap_or(&req.actor.agent_id);
    let reason = req.args["reason"].as_str();
    let now = req.args["created_at"]
        .as_i64()
        .unwrap_or_else(|| chrono::Utc::now().timestamp());

    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "append",
            resource: crate::policy::resource::AUDIT,
            sql_content: None,
            channel: req.actor.channel.as_deref(),
            arguments: None,
        },
        req,
    )?;
    if matches!(decision.effect, crate::policy::Effect::Deny) {
        return Ok(denied_response(&decision));
    }

    let id = req.args["id"]
        .as_str()
        .map(|s| s.to_string())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    conn.execute(
        "INSERT INTO policy_audit (id, policy_id, actor, action, resource, effect, reason, correlation_id, session_id, idempotency_key, idempotency_state, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        rusqlite::params![
            id,
            req.args["policy_id"].as_str(),
            actor,
            action,
            resource,
            effect,
            reason,
            req.args["correlation_id"].as_str().or(req.request_id.as_deref()).or(req.context.trace_id.as_deref()),
            req.args["session_id"].as_str().or(req.context.session_id.as_deref()),
            req.idempotency_key.as_deref(),
            Some(if req.idempotency_key.is_some() { "provided_enforced" } else { "not_provided" }),
            now,
        ],
    )?;

    let brain_id = req.context.brain_id.as_deref().unwrap_or(&req.actor.agent_id);
    if !brain_id.is_empty() {
        let _ = crate::store::events::append(
            conn,
            &crate::store::events::AppendInput {
                brain_id: brain_id.to_string(),
                event_type: "policy.decision".to_string(),
                action: action.to_string(),
                resource: Some(resource.to_string()),
                actor: Some(actor.to_string()),
                session_id: req.args["session_id"].as_str().or(req.context.session_id.as_deref()).map(String::from),
                correlation_id: req.args["correlation_id"].as_str().or(req.request_id.as_deref()).or(req.context.trace_id.as_deref()).map(String::from),
                detail: reason.map(|s| format!("effect={effect} reason={s}")),
            },
        );
    }

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: "audit entry appended".into(),
        data: serde_json::json!({
            "id": id,
            "actor": actor,
            "action": action,
            "resource": resource,
            "effect": effect,
        }),
        policy: Some(policy_meta(&decision)),
        audit: AuditMeta { recorded: true },
    })
}
