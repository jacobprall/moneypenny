use rusqlite::Connection;
use super::{AuditMeta, OperationRequest, OperationResponse, denied_response, evaluate_policy_with_request_context, fail_response, policy_meta};

pub(super) fn op_session_resolve(
    conn: &Connection,
    req: &OperationRequest,
) -> anyhow::Result<OperationResponse> {
    let requested = req.args["session_id"].as_str().map(|s| s.to_string());
    let channel = req.args["channel"]
        .as_str()
        .or(req.actor.channel.as_deref());
    let target_agent = req.args["agent_id"].as_str().unwrap_or(&req.actor.agent_id);
    let create_if_missing = req.args["create_if_missing"].as_bool().unwrap_or(true);

    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "resolve",
            resource: crate::policy::resource::SESSION,
            sql_content: None,
            channel: req.actor.channel.as_deref(),
            arguments: None,
        },
        req,
    )?;
    if matches!(decision.effect, crate::policy::Effect::Deny) {
        return Ok(denied_response(&decision));
    }

    let (session_id, created) = if let Some(sid) = requested {
        if let Some(existing) = crate::store::log::get_session(conn, &sid)? {
            (existing.id, false)
        } else if create_if_missing {
            let now = chrono::Utc::now().timestamp();
            conn.execute(
                "INSERT INTO sessions (id, agent_id, channel, started_at) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![sid, target_agent, channel, now],
            )?;
            (sid, true)
        } else {
            return Ok(OperationResponse {
                ok: false,
                code: "not_found".into(),
                message: "session not found".into(),
                data: serde_json::json!({}),
                policy: Some(policy_meta(&decision)),
                audit: AuditMeta { recorded: true },
            });
        }
    } else {
        (
            crate::store::log::create_session(conn, target_agent, channel)?,
            true,
        )
    };

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: if created {
            "session created".into()
        } else {
            "session resolved".into()
        },
        data: serde_json::json!({
            "session_id": session_id,
            "agent_id": target_agent,
            "channel": channel,
            "created": created
        }),
        policy: Some(policy_meta(&decision)),
        audit: AuditMeta { recorded: true },
    })
}

pub(super) fn op_session_list(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let target_agent = req.args["agent_id"].as_str().unwrap_or(&req.actor.agent_id);
    let limit = req.args["limit"].as_u64().unwrap_or(20).clamp(1, 200) as i64;

    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "list",
            resource: crate::policy::resource::SESSION,
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
        "SELECT s.id, s.channel, s.started_at, s.ended_at,
                COUNT(m.id) AS message_count,
                COALESCE(MAX(m.created_at), s.started_at) AS last_activity
         FROM sessions s
         LEFT JOIN messages m ON m.session_id = s.id
         WHERE s.agent_id = ?1
         GROUP BY s.id, s.channel, s.started_at, s.ended_at
         ORDER BY last_activity DESC
         LIMIT ?2",
    )?;
    let rows = stmt
        .query_map(rusqlite::params![target_agent, limit], |r| {
            Ok(serde_json::json!({
                "id": r.get::<_, String>(0)?,
                "channel": r.get::<_, Option<String>>(1)?,
                "started_at": r.get::<_, i64>(2)?,
                "ended_at": r.get::<_, Option<i64>>(3)?,
                "message_count": r.get::<_, i64>(4)?,
                "last_activity": r.get::<_, i64>(5)?,
            }))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: "sessions listed".into(),
        data: serde_json::json!(rows),
        policy: Some(policy_meta(&decision)),
        audit: AuditMeta { recorded: true },
    })
}

pub(super) fn op_js_tool_add(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let name = req.args["name"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'name'"))?;
    let description = req.args["description"]
        .as_str()
        .unwrap_or("User-defined JS tool");
    let script = req.args["script"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'script'"))?;

    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "add",
            resource: crate::policy::resource::JS_TOOL,
            sql_content: None,
            channel: req.actor.channel.as_deref(),
            arguments: None,
        },
        req,
    )?;
    if matches!(decision.effect, crate::policy::Effect::Deny) {
        return Ok(denied_response(&decision));
    }

    if name
        .chars()
        .any(|c| !c.is_alphanumeric() && c != '_' && c != '-')
    {
        return Ok(fail_response(
            "invalid_args",
            "tool name must contain only letters, digits, underscores, or hyphens".into(),
        ));
    }

    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp();
    let tool_id = format!("sqlite_js:{name}");
    conn.execute(
        "INSERT OR REPLACE INTO skills
         (id, name, description, content, tool_id, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![id, name, description, script, tool_id, now, now],
    )?;

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: "js tool added".into(),
        data: serde_json::json!({
            "id": id,
            "name": name,
            "status": "created",
        }),
        policy: Some(policy_meta(&decision)),
        audit: AuditMeta { recorded: true },
    })
}

pub(super) fn op_js_tool_list(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "list",
            resource: crate::policy::resource::JS_TOOL,
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
        "SELECT name, description, updated_at FROM skills
         WHERE tool_id LIKE 'sqlite_js:%'
         ORDER BY name",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok(serde_json::json!({
                "name": r.get::<_, String>(0)?,
                "description": r.get::<_, String>(1)?,
                "updated_at": r.get::<_, i64>(2)?,
            }))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: "js tools listed".into(),
        data: serde_json::json!(rows),
        policy: Some(policy_meta(&decision)),
        audit: AuditMeta { recorded: true },
    })
}

pub(super) fn op_js_tool_delete(
    conn: &Connection,
    req: &OperationRequest,
) -> anyhow::Result<OperationResponse> {
    let name = req.args["name"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'name'"))?;
    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "delete",
            resource: crate::policy::resource::JS_TOOL,
            sql_content: None,
            channel: req.actor.channel.as_deref(),
            arguments: None,
        },
        req,
    )?;
    if matches!(decision.effect, crate::policy::Effect::Deny) {
        return Ok(denied_response(&decision));
    }

    let rows = conn.execute(
        "DELETE FROM skills WHERE name = ?1 AND tool_id LIKE 'sqlite_js:%'",
        [name],
    )?;
    if rows == 0 {
        return Ok(OperationResponse {
            ok: false,
            code: "not_found".into(),
            message: format!("js tool '{name}' not found"),
            data: serde_json::json!({}),
            policy: Some(policy_meta(&decision)),
            audit: AuditMeta { recorded: true },
        });
    }

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: "js tool deleted".into(),
        data: serde_json::json!({
            "name": name,
            "status": "deleted",
        }),
        policy: Some(policy_meta(&decision)),
        audit: AuditMeta { recorded: true },
    })
}
