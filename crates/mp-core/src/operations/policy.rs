use rusqlite::Connection;
use super::{AuditMeta, OperationRequest, OperationResponse, denied_response, evaluate_policy_with_request_context, fail_response, parse_policy_mode, policy_meta};

pub(super) fn op_policy_add(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let name = req.args["name"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'name'"))?;
    let effect = req.args["effect"].as_str().unwrap_or("deny");
    let priority = req.args["priority"].as_i64().unwrap_or(0);
    let actor_pattern = req.args["actor_pattern"].as_str();
    let action_pattern = req.args["action_pattern"].as_str();
    let resource_pattern = req.args["resource_pattern"].as_str();
    let argument_pattern = req.args["argument_pattern"].as_str();
    let channel_pattern = req.args["channel_pattern"].as_str();
    let sql_pattern = req.args["sql_pattern"].as_str();
    let rule_type = req.args["rule_type"].as_str();
    let rule_config = req.args["rule_config"].as_str();
    let message = req.args["message"].as_str();

    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "add",
            resource: crate::policy::resource::POLICY,
            sql_content: None,
            channel: req.actor.channel.as_deref(),
            arguments: None,
        },
        req,
    )?;
    if matches!(decision.effect, crate::policy::Effect::Deny) {
        return Ok(denied_response(&decision));
    }

    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp();
    let brain_id = req.context.brain_id.as_deref().unwrap_or(&req.actor.agent_id);
    conn.execute(
        "INSERT INTO policies (id, brain_id, name, priority, effect, actor_pattern, action_pattern, resource_pattern,
         argument_pattern, channel_pattern, sql_pattern, rule_type, rule_config, message, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        rusqlite::params![id, brain_id, name, priority, effect, actor_pattern, action_pattern, resource_pattern,
            argument_pattern, channel_pattern, sql_pattern, rule_type, rule_config, message, now],
    )?;

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: "policy added".into(),
        data: serde_json::json!({
            "id": id,
            "name": name,
            "effect": effect,
            "priority": priority
        }),
        policy: Some(policy_meta(&decision)),
        audit: AuditMeta { recorded: true },
    })
}

pub(super) fn op_policy_list(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let limit = req.args["limit"].as_u64().unwrap_or(50).clamp(1, 500) as i64;
    let enabled = req.args["enabled"].as_bool();
    let effect = req.args["effect"].as_str().map(|s| s.to_string());

    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "list",
            resource: crate::policy::resource::POLICY,
            sql_content: None,
            channel: req.actor.channel.as_deref(),
            arguments: None,
        },
        req,
    )?;
    if matches!(decision.effect, crate::policy::Effect::Deny) {
        return Ok(denied_response(&decision));
    }

    let enabled_val: Option<i64> = enabled.map(|b| if b { 1 } else { 0 });

    let mut stmt = conn.prepare(
        "SELECT id, name, priority, effect, actor_pattern, action_pattern, resource_pattern,
                sql_pattern, argument_pattern, channel_pattern, message, rule_type, enabled, created_at
         FROM policies
         WHERE (?1 IS NULL OR enabled = ?1)
           AND (?2 IS NULL OR effect = ?2)
         ORDER BY priority DESC
         LIMIT ?3",
    )?;
    let rows = stmt
        .query_map(rusqlite::params![enabled_val, effect, limit], |r| {
            Ok(serde_json::json!({
                "id": r.get::<_, String>(0)?,
                "name": r.get::<_, String>(1)?,
                "priority": r.get::<_, i64>(2)?,
                "effect": r.get::<_, String>(3)?,
                "actor_pattern": r.get::<_, Option<String>>(4)?,
                "action_pattern": r.get::<_, Option<String>>(5)?,
                "resource_pattern": r.get::<_, Option<String>>(6)?,
                "sql_pattern": r.get::<_, Option<String>>(7)?,
                "argument_pattern": r.get::<_, Option<String>>(8)?,
                "channel_pattern": r.get::<_, Option<String>>(9)?,
                "message": r.get::<_, Option<String>>(10)?,
                "rule_type": r.get::<_, Option<String>>(11)?,
                "enabled": r.get::<_, i64>(12)? != 0,
                "created_at": r.get::<_, i64>(13)?,
            }))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: "policies listed".into(),
        data: serde_json::json!(rows),
        policy: Some(policy_meta(&decision)),
        audit: AuditMeta { recorded: true },
    })
}

pub(super) fn op_policy_disable(
    conn: &Connection,
    req: &OperationRequest,
) -> anyhow::Result<OperationResponse> {
    let id = req.args["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'id'"))?;

    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "disable",
            resource: crate::policy::resource::POLICY,
            sql_content: None,
            channel: req.actor.channel.as_deref(),
            arguments: None,
        },
        req,
    )?;
    if matches!(decision.effect, crate::policy::Effect::Deny) {
        return Ok(denied_response(&decision));
    }

    let affected = conn.execute(
        "UPDATE policies SET enabled = 0 WHERE id = ?1",
        rusqlite::params![id],
    )?;

    if affected == 0 {
        return Ok(OperationResponse {
            ok: false,
            code: "not_found".into(),
            message: format!("policy '{id}' not found"),
            data: serde_json::json!({}),
            policy: Some(policy_meta(&decision)),
            audit: AuditMeta { recorded: true },
        });
    }

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: "policy disabled".into(),
        data: serde_json::json!({
            "id": id,
            "enabled": false
        }),
        policy: Some(policy_meta(&decision)),
        audit: AuditMeta { recorded: true },
    })
}

// ---------------------------------------------------------------------------
// Policy spec: plan → confirm → apply (agent-proposed policy creation)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct PolicySpecRecord {
    id: String,
    _agent_id: String,
    intent: String,
    policy_name: String,
    effect: String,
    priority: i64,
    actor_pattern: Option<String>,
    action_pattern: Option<String>,
    resource_pattern: Option<String>,
    argument_pattern: Option<String>,
    channel_pattern: Option<String>,
    sql_pattern: Option<String>,
    rule_type: Option<String>,
    rule_config: Option<String>,
    message: Option<String>,
    status: String,
    applied_policy_id: Option<String>,
}

fn load_policy_spec(conn: &Connection, spec_id: &str) -> anyhow::Result<Option<PolicySpecRecord>> {
    let mut stmt = conn.prepare(
        "SELECT id, agent_id, intent, policy_name, effect, priority,
                actor_pattern, action_pattern, resource_pattern, argument_pattern,
                channel_pattern, sql_pattern, rule_type, rule_config, message,
                status, applied_policy_id
         FROM policy_specs WHERE id = ?1",
    )?;
    let row = stmt
        .query_row([spec_id], |r| {
            Ok(PolicySpecRecord {
                id: r.get(0)?,
                _agent_id: r.get(1)?,
                intent: r.get(2)?,
                policy_name: r.get(3)?,
                effect: r.get(4)?,
                priority: r.get(5)?,
                actor_pattern: r.get(6)?,
                action_pattern: r.get(7)?,
                resource_pattern: r.get(8)?,
                argument_pattern: r.get(9)?,
                channel_pattern: r.get(10)?,
                sql_pattern: r.get(11)?,
                rule_type: r.get(12)?,
                rule_config: r.get(13)?,
                message: r.get(14)?,
                status: r.get(15)?,
                applied_policy_id: r.get(16)?,
            })
        })
        .ok();
    Ok(row)
}

pub(super) fn op_policy_spec_plan(
    conn: &Connection,
    req: &OperationRequest,
) -> anyhow::Result<OperationResponse> {
    let intent = req.args["intent"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'intent'"))?;
    let policy_name = req.args["policy_name"]
        .as_str()
        .or(req.args["name"].as_str())
        .ok_or_else(|| anyhow::anyhow!("missing 'policy_name'"))?;
    let effect = req.args["effect"].as_str().unwrap_or("deny");
    let priority = req.args["priority"].as_i64().unwrap_or(0);
    let agent_id = req.args["agent_id"]
        .as_str()
        .map(|s| s.to_string())
        .unwrap_or_else(|| req.actor.agent_id.clone());
    let actor_pattern = req.args["actor_pattern"].as_str();
    let action_pattern = req.args["action_pattern"].as_str();
    let resource_pattern = req.args["resource_pattern"].as_str();
    let argument_pattern = req.args["argument_pattern"].as_str();
    let channel_pattern = req.args["channel_pattern"].as_str();
    let sql_pattern = req.args["sql_pattern"].as_str();
    let rule_type = req.args["rule_type"].as_str();
    let rule_config = req.args["rule_config"].as_str();
    let message = req.args["message"].as_str();
    let plan_json = match req.args.get("plan") {
        Some(v) if v.is_string() => v.as_str().unwrap_or("{}").to_string(),
        Some(v) => serde_json::to_string(v).unwrap_or_else(|_| "{}".to_string()),
        None => "{}".to_string(),
    };
    let proposed_by = req.args["proposed_by"].as_str().unwrap_or("agent");
    let source_session_id = req
        .args
        .get("source_session_id")
        .and_then(|v| v.as_str())
        .or(req.context.session_id.as_deref());
    let source_message_id = req.args["source_message_id"].as_str();

    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "plan",
            resource: crate::policy::resource::POLICY_SPEC,
            sql_content: None,
            channel: req.actor.channel.as_deref(),
            arguments: None,
        },
        req,
    )?;
    if matches!(decision.effect, crate::policy::Effect::Deny) {
        return Ok(denied_response(&decision));
    }

    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp();
    conn.execute(
        "INSERT INTO policy_specs
         (id, agent_id, intent, plan_json, policy_name, effect, priority,
          actor_pattern, action_pattern, resource_pattern, argument_pattern,
          channel_pattern, sql_pattern, rule_type, rule_config, message,
          status, proposed_by, source_session_id, source_message_id, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16,
                 'planned', ?17, ?18, ?19, ?20, ?21)",
        rusqlite::params![
            id,
            agent_id,
            intent,
            plan_json,
            policy_name,
            effect,
            priority,
            actor_pattern,
            action_pattern,
            resource_pattern,
            argument_pattern,
            channel_pattern,
            sql_pattern,
            rule_type,
            rule_config,
            message,
            proposed_by,
            source_session_id,
            source_message_id,
            now,
            now
        ],
    )?;

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: "policy spec planned — awaiting confirmation".into(),
        data: serde_json::json!({
            "spec_id": id,
            "status": "planned",
            "policy_name": policy_name,
            "effect": effect,
            "priority": priority,
            "intent": intent,
            "resource_pattern": resource_pattern,
            "argument_pattern": argument_pattern
        }),
        policy: Some(policy_meta(&decision)),
        audit: AuditMeta { recorded: true },
    })
}

pub(super) fn op_policy_spec_confirm(
    conn: &Connection,
    req: &OperationRequest,
) -> anyhow::Result<OperationResponse> {
    let spec_id = req.args["spec_id"]
        .as_str()
        .or(req.args["id"].as_str())
        .ok_or_else(|| anyhow::anyhow!("missing 'spec_id'"))?;
    let confirmed = req.args["confirm"].as_bool().unwrap_or(true);
    if !confirmed {
        return Ok(fail_response(
            "invalid_args",
            "confirm=false is not supported; provide confirm=true to approve".to_string(),
        ));
    }

    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "confirm",
            resource: crate::policy::resource::POLICY_SPEC,
            sql_content: None,
            channel: req.actor.channel.as_deref(),
            arguments: None,
        },
        req,
    )?;
    if matches!(decision.effect, crate::policy::Effect::Deny) {
        return Ok(denied_response(&decision));
    }

    let spec = match load_policy_spec(conn, spec_id)? {
        Some(s) => s,
        None => {
            return Ok(OperationResponse {
                ok: false,
                code: "not_found".into(),
                message: format!("policy spec '{spec_id}' not found"),
                data: serde_json::json!({}),
                policy: Some(policy_meta(&decision)),
                audit: AuditMeta { recorded: true },
            });
        }
    };
    if spec.status == "applied" {
        return Ok(OperationResponse {
            ok: false,
            code: "invalid_state".into(),
            message: format!("policy spec '{spec_id}' is already applied"),
            data: serde_json::json!({
                "spec_id": spec.id,
                "status": spec.status,
                "applied_policy_id": spec.applied_policy_id
            }),
            policy: Some(policy_meta(&decision)),
            audit: AuditMeta { recorded: true },
        });
    }
    if spec.status != "planned" && spec.status != "confirmed" {
        return Ok(OperationResponse {
            ok: false,
            code: "invalid_state".into(),
            message: format!(
                "policy spec '{spec_id}' cannot be confirmed from status '{}'",
                spec.status
            ),
            data: serde_json::json!({ "spec_id": spec.id, "status": spec.status }),
            policy: Some(policy_meta(&decision)),
            audit: AuditMeta { recorded: true },
        });
    }

    let now = chrono::Utc::now().timestamp();
    conn.execute(
        "UPDATE policy_specs SET status = 'confirmed', updated_at = ?2 WHERE id = ?1",
        rusqlite::params![spec_id, now],
    )?;

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: "policy spec confirmed — ready to apply".into(),
        data: serde_json::json!({
            "spec_id": spec.id,
            "status": "confirmed",
            "policy_name": spec.policy_name,
            "effect": spec.effect,
            "priority": spec.priority,
            "intent": spec.intent
        }),
        policy: Some(policy_meta(&decision)),
        audit: AuditMeta { recorded: true },
    })
}

pub(super) fn op_policy_spec_apply(
    conn: &Connection,
    req: &OperationRequest,
) -> anyhow::Result<OperationResponse> {
    let spec_id = req.args["spec_id"]
        .as_str()
        .or(req.args["id"].as_str())
        .ok_or_else(|| anyhow::anyhow!("missing 'spec_id'"))?;

    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "apply",
            resource: crate::policy::resource::POLICY_SPEC,
            sql_content: None,
            channel: req.actor.channel.as_deref(),
            arguments: None,
        },
        req,
    )?;
    if matches!(decision.effect, crate::policy::Effect::Deny) {
        return Ok(denied_response(&decision));
    }

    let spec = match load_policy_spec(conn, spec_id)? {
        Some(s) => s,
        None => {
            return Ok(OperationResponse {
                ok: false,
                code: "not_found".into(),
                message: format!("policy spec '{spec_id}' not found"),
                data: serde_json::json!({}),
                policy: Some(policy_meta(&decision)),
                audit: AuditMeta { recorded: true },
            });
        }
    };
    if spec.status == "applied" {
        return Ok(OperationResponse {
            ok: true,
            code: "ok".into(),
            message: "policy spec already applied".into(),
            data: serde_json::json!({
                "spec_id": spec.id,
                "status": "applied",
                "policy_id": spec.applied_policy_id
            }),
            policy: Some(policy_meta(&decision)),
            audit: AuditMeta { recorded: true },
        });
    }
    if spec.status != "confirmed" {
        return Ok(OperationResponse {
            ok: false,
            code: "invalid_state".into(),
            message: format!("policy spec '{spec_id}' must be confirmed before apply"),
            data: serde_json::json!({ "spec_id": spec.id, "status": spec.status }),
            policy: Some(policy_meta(&decision)),
            audit: AuditMeta { recorded: true },
        });
    }

    let policy_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp();
    let brain_id = req.context.brain_id.as_deref().unwrap_or(&req.actor.agent_id);
    conn.execute(
        "INSERT INTO policies (id, brain_id, name, priority, effect, actor_pattern, action_pattern, resource_pattern,
         argument_pattern, channel_pattern, sql_pattern, rule_type, rule_config, message, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        rusqlite::params![
            policy_id, brain_id, spec.policy_name, spec.priority, spec.effect,
            spec.actor_pattern, spec.action_pattern, spec.resource_pattern,
            spec.argument_pattern, spec.channel_pattern, spec.sql_pattern,
            spec.rule_type, spec.rule_config, spec.message, now
        ],
    )?;
    conn.execute(
        "UPDATE policy_specs SET status = 'applied', applied_policy_id = ?2, updated_at = ?3 WHERE id = ?1",
        rusqlite::params![spec.id, policy_id, now],
    )?;

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: "policy spec applied — policy is now active".into(),
        data: serde_json::json!({
            "spec_id": spec.id,
            "status": "applied",
            "policy_id": policy_id,
            "policy_name": spec.policy_name,
            "effect": spec.effect,
            "priority": spec.priority
        }),
        policy: Some(policy_meta(&decision)),
        audit: AuditMeta { recorded: true },
    })
}

pub(super) fn op_policy_evaluate(
    conn: &Connection,
    req: &OperationRequest,
) -> anyhow::Result<OperationResponse> {
    let action = req.args["action"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'action'"))?;
    let resource = req.args["resource"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'resource'"))?;
    let actor = req.args["actor"].as_str().unwrap_or(&req.actor.agent_id);
    let sql_content = req.args["sql_content"].as_str();
    let channel = req.args["channel"]
        .as_str()
        .or(req.actor.channel.as_deref());
    let mode = parse_policy_mode(req.args["mode"].as_str());

    let decision = if let Some(mode) = mode {
        crate::policy::evaluate_with_mode(
            conn,
            &crate::policy::PolicyRequest {
                actor,
                action,
                resource,
                sql_content,
                channel,
                arguments: None,
            },
            mode,
        )?
    } else {
        crate::policy::evaluate(
            conn,
            &crate::policy::PolicyRequest {
                actor,
                action,
                resource,
                sql_content,
                channel,
                arguments: None,
            },
        )?
    };

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: "policy evaluated".into(),
        data: serde_json::json!({
            "actor": actor,
            "action": action,
            "resource": resource,
            "effect": match decision.effect {
                crate::policy::Effect::Allow => "allow",
                crate::policy::Effect::Deny => "deny",
                crate::policy::Effect::Audit => "audit",
            },
            "policy_id": decision.policy_id,
            "reason": decision.reason,
            "mode": req.args["mode"].as_str().unwrap_or("default"),
        }),
        policy: Some(policy_meta(&decision)),
        audit: AuditMeta { recorded: true },
    })
}

pub(super) fn op_policy_explain(
    conn: &Connection,
    req: &OperationRequest,
) -> anyhow::Result<OperationResponse> {
    let mut resp = op_policy_evaluate(conn, req)?;
    resp.message = "policy explanation generated".into();
    Ok(resp)
}
