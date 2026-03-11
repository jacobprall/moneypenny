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

pub(super) fn op_usage_summary(
    conn: &Connection,
    req: &OperationRequest,
) -> anyhow::Result<OperationResponse> {
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

    let period = req.args["period"].as_str().unwrap_or("all");
    let group_by = req.args["group_by"].as_str().unwrap_or("model");

    let now = chrono::Utc::now().timestamp();
    let since = match period {
        "today" => now - 86400,
        "week" => now - 86400 * 7,
        "month" => now - 86400 * 30,
        _ => 0,
    };

    let totals: (i64, i64, i64, f64, i64) = conn.query_row(
        "SELECT COALESCE(SUM(normalized_input_tokens), 0),
                COALESCE(SUM(normalized_output_tokens), 0),
                COALESCE(SUM(normalized_total_tokens), 0),
                COALESCE(SUM(normalized_cost_usd), 0.0),
                COUNT(*)
         FROM external_events
         WHERE event_type = 'model.usage' AND ingested_at >= ?1",
        rusqlite::params![since],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
    )?;

    let breakdown = match group_by {
        "session" => {
            let mut stmt = conn.prepare(
                "SELECT COALESCE(session_id, 'unknown'),
                        COALESCE(SUM(normalized_input_tokens), 0),
                        COALESCE(SUM(normalized_output_tokens), 0),
                        COALESCE(SUM(normalized_total_tokens), 0),
                        COALESCE(SUM(normalized_cost_usd), 0.0),
                        COUNT(*)
                 FROM external_events
                 WHERE event_type = 'model.usage' AND ingested_at >= ?1
                 GROUP BY session_id
                 ORDER BY SUM(normalized_cost_usd) DESC
                 LIMIT 20",
            )?;
            stmt.query_map(rusqlite::params![since], |r| {
                Ok(serde_json::json!({
                    "key": r.get::<_, String>(0)?,
                    "input_tokens": r.get::<_, i64>(1)?,
                    "output_tokens": r.get::<_, i64>(2)?,
                    "total_tokens": r.get::<_, i64>(3)?,
                    "cost_usd": r.get::<_, f64>(4)?,
                    "count": r.get::<_, i64>(5)?,
                }))
            })?
            .collect::<Result<Vec<_>, _>>()?
        }
        "day" => {
            let mut stmt = conn.prepare(
                "SELECT date(event_ts, 'unixepoch') AS day,
                        COALESCE(SUM(normalized_input_tokens), 0),
                        COALESCE(SUM(normalized_output_tokens), 0),
                        COALESCE(SUM(normalized_total_tokens), 0),
                        COALESCE(SUM(normalized_cost_usd), 0.0),
                        COUNT(*)
                 FROM external_events
                 WHERE event_type = 'model.usage' AND ingested_at >= ?1
                 GROUP BY day
                 ORDER BY day DESC
                 LIMIT 30",
            )?;
            stmt.query_map(rusqlite::params![since], |r| {
                Ok(serde_json::json!({
                    "key": r.get::<_, String>(0)?,
                    "input_tokens": r.get::<_, i64>(1)?,
                    "output_tokens": r.get::<_, i64>(2)?,
                    "total_tokens": r.get::<_, i64>(3)?,
                    "cost_usd": r.get::<_, f64>(4)?,
                    "count": r.get::<_, i64>(5)?,
                }))
            })?
            .collect::<Result<Vec<_>, _>>()?
        }
        _ => {
            // group by model (default)
            let mut stmt = conn.prepare(
                "SELECT COALESCE(normalized_provider, 'unknown') || '/' || COALESCE(normalized_model, 'unknown'),
                        COALESCE(SUM(normalized_input_tokens), 0),
                        COALESCE(SUM(normalized_output_tokens), 0),
                        COALESCE(SUM(normalized_total_tokens), 0),
                        COALESCE(SUM(normalized_cost_usd), 0.0),
                        COUNT(*)
                 FROM external_events
                 WHERE event_type = 'model.usage' AND ingested_at >= ?1
                 GROUP BY normalized_provider, normalized_model
                 ORDER BY SUM(normalized_cost_usd) DESC
                 LIMIT 20",
            )?;
            stmt.query_map(rusqlite::params![since], |r| {
                Ok(serde_json::json!({
                    "key": r.get::<_, String>(0)?,
                    "input_tokens": r.get::<_, i64>(1)?,
                    "output_tokens": r.get::<_, i64>(2)?,
                    "total_tokens": r.get::<_, i64>(3)?,
                    "cost_usd": r.get::<_, f64>(4)?,
                    "count": r.get::<_, i64>(5)?,
                }))
            })?
            .collect::<Result<Vec<_>, _>>()?
        }
    };

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: "usage summary".into(),
        data: serde_json::json!({
            "period": period,
            "group_by": group_by,
            "totals": {
                "input_tokens": totals.0,
                "output_tokens": totals.1,
                "total_tokens": totals.2,
                "cost_usd": totals.3,
                "event_count": totals.4,
            },
            "breakdown": breakdown,
        }),
        policy: Some(policy_meta(&decision)),
        audit: AuditMeta { recorded: true },
    })
}

pub(super) fn op_briefing_compose(
    conn: &Connection,
    req: &OperationRequest,
) -> anyhow::Result<OperationResponse> {
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

    let now = chrono::Utc::now().timestamp();
    let since_48h = now - 86400 * 2;

    // 1. Recent sessions with summaries
    let mut stmt_sessions = conn.prepare(
        "SELECT id, channel, started_at, ended_at, summary
         FROM sessions
         WHERE started_at >= ?1
         ORDER BY started_at DESC
         LIMIT 10",
    )?;
    let recent_sessions: Vec<serde_json::Value> = stmt_sessions
        .query_map(rusqlite::params![since_48h], |r| {
            Ok(serde_json::json!({
                "id": r.get::<_, String>(0)?,
                "channel": r.get::<_, Option<String>>(1)?,
                "started_at": r.get::<_, i64>(2)?,
                "ended_at": r.get::<_, Option<i64>>(3)?,
                "summary": r.get::<_, Option<String>>(4)?,
            }))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    // 2. Activity stats by action type
    let mut stmt_stats = conn.prepare(
        "SELECT action, COUNT(*) as cnt
         FROM activity_log
         WHERE created_at >= ?1
         GROUP BY action
         ORDER BY cnt DESC",
    )?;
    let activity_stats: Vec<serde_json::Value> = stmt_stats
        .query_map(rusqlite::params![since_48h], |r| {
            Ok(serde_json::json!({
                "action": r.get::<_, String>(0)?,
                "count": r.get::<_, i64>(1)?,
            }))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    // 3. Recent facts added/updated
    let mut stmt_facts = conn.prepare(
        "SELECT id, COALESCE(summary, SUBSTR(content, 1, 120)) as summary, created_at
         FROM facts
         WHERE created_at >= ?1 AND superseded_by IS NULL
         ORDER BY created_at DESC
         LIMIT 10",
    )?;
    let recent_facts: Vec<serde_json::Value> = stmt_facts
        .query_map(rusqlite::params![since_48h], |r| {
            Ok(serde_json::json!({
                "id": r.get::<_, String>(0)?,
                "summary": r.get::<_, String>(1)?,
                "created_at": r.get::<_, i64>(2)?,
            }))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    // 4. Recent policy denials
    let mut stmt_denials = conn.prepare(
        "SELECT actor, action, resource, reason, created_at
         FROM policy_audit
         WHERE effect = 'deny' AND created_at >= ?1
         ORDER BY created_at DESC
         LIMIT 5",
    )?;
    let recent_denials: Vec<serde_json::Value> = stmt_denials
        .query_map(rusqlite::params![since_48h], |r| {
            Ok(serde_json::json!({
                "actor": r.get::<_, String>(0)?,
                "action": r.get::<_, String>(1)?,
                "resource": r.get::<_, String>(2)?,
                "reason": r.get::<_, Option<String>>(3)?,
                "created_at": r.get::<_, i64>(4)?,
            }))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    // 5. Token spend summary (last 48h)
    let spend: (i64, f64, i64) = conn
        .query_row(
            "SELECT COALESCE(SUM(normalized_total_tokens), 0),
                    COALESCE(SUM(normalized_cost_usd), 0.0),
                    COUNT(*)
             FROM external_events
             WHERE event_type = 'model.usage' AND ingested_at >= ?1",
            rusqlite::params![since_48h],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap_or((0, 0.0, 0));

    // Build human-readable text
    let last_summary = recent_sessions
        .first()
        .and_then(|s| s["summary"].as_str())
        .unwrap_or("No recent session summary available.");

    let mut text = format!("## Session Briefing\n\n**Last session:** {last_summary}\n\n");

    if !activity_stats.is_empty() {
        text.push_str("**Recent activity (48h):** ");
        let parts: Vec<String> = activity_stats
            .iter()
            .filter_map(|s| {
                let action = s["action"].as_str()?;
                let count = s["count"].as_i64()?;
                Some(format!("{count} {action}"))
            })
            .collect();
        text.push_str(&parts.join(", "));
        text.push_str("\n\n");
    }

    if !recent_facts.is_empty() {
        text.push_str(&format!(
            "**New facts (48h):** {} added\n\n",
            recent_facts.len()
        ));
    }

    if !recent_denials.is_empty() {
        text.push_str(&format!(
            "**Policy denials (48h):** {}\n\n",
            recent_denials.len()
        ));
    }

    if spend.2 > 0 {
        text.push_str(&format!(
            "**Token spend (48h):** {} tokens, ${:.4}\n",
            spend.0, spend.1
        ));
    }

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: "briefing composed".into(),
        data: serde_json::json!({
            "text": text,
            "recent_sessions": recent_sessions,
            "activity_stats": activity_stats,
            "recent_facts": recent_facts,
            "recent_denials": recent_denials,
            "spend": {
                "total_tokens": spend.0,
                "cost_usd": spend.1,
                "event_count": spend.2,
            },
        }),
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
