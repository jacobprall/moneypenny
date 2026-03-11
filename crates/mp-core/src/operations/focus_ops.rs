//! Brain.focus operations — working set (scratch) + composition engine.

use rusqlite::Connection;
use serde_json::json;
use uuid::Uuid;

use super::{fail_response, AuditMeta, OperationRequest, OperationResponse};

fn brain_id_from_req(req: &OperationRequest) -> anyhow::Result<String> {
    let from_args = req.args["brain_id"].as_str().map(String::from);
    let from_ctx = req.context.brain_id.clone();
    let from_actor = (!req.actor.agent_id.is_empty()).then(|| req.actor.agent_id.clone());

    from_args
        .or(from_ctx)
        .or(from_actor)
        .ok_or_else(|| anyhow::anyhow!("missing brain_id and no default from context/actor"))
}

fn session_id_from_req(req: &OperationRequest) -> anyhow::Result<String> {
    req.args["session_id"]
        .as_str()
        .map(String::from)
        .or_else(|| req.context.session_id.clone())
        .ok_or_else(|| anyhow::anyhow!("missing session_id"))
}

pub(super) fn op_focus_set(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let session_id = session_id_from_req(req)?;
    let key = req.args["key"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'key'"))?;
    let content = req.args["content"]
        .as_str()
        .unwrap_or("");

    let id = crate::store::scratch::set(conn, &session_id, key, content)?;

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: String::new(),
        data: json!({ "id": id, "key": key }),
        policy: None,
        audit: AuditMeta { recorded: false },
    })
}

pub(super) fn op_focus_get(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let session_id = session_id_from_req(req)?;
    let key = req.args["key"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'key'"))?;

    match crate::store::scratch::get(conn, &session_id, key)? {
        Some(entry) => Ok(OperationResponse {
            ok: true,
            code: "ok".into(),
            message: String::new(),
            data: json!({
                "id": entry.id,
                "key": entry.key,
                "content": entry.content,
                "created_at": entry.created_at,
                "updated_at": entry.updated_at,
            }),
            policy: None,
            audit: AuditMeta { recorded: false },
        }),
        None => Ok(fail_response(
            "not_found",
            format!("no focus entry for key '{key}'"),
        )),
    }
}

pub(super) fn op_focus_list(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let session_id = session_id_from_req(req)?;

    let entries = crate::store::scratch::list(conn, &session_id)?;
    let items: Vec<serde_json::Value> = entries
        .iter()
        .map(|e| {
            json!({
                "id": e.id,
                "key": e.key,
                "content": e.content,
                "created_at": e.created_at,
                "updated_at": e.updated_at,
            })
        })
        .collect();

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: format!("{} focus entries", items.len()),
        data: json!({ "entries": items }),
        policy: None,
        audit: AuditMeta { recorded: false },
    })
}

pub(super) fn op_focus_clear(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let session_id = session_id_from_req(req)?;

    if let Some(key) = req.args["key"].as_str() {
        if let Some(entry) = crate::store::scratch::get(conn, &session_id, key)? {
            crate::store::scratch::remove(conn, &entry.id)?;
            return Ok(OperationResponse {
                ok: true,
                code: "ok".into(),
                message: format!("cleared key '{key}'"),
                data: json!({ "cleared": key }),
                policy: None,
                audit: AuditMeta { recorded: false },
            });
        }
        return Ok(fail_response("not_found", format!("no focus entry for key '{key}'")));
    }

    let deleted = crate::store::scratch::clear_session(conn, &session_id)?;
    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: format!("cleared {deleted} focus entries"),
        data: json!({ "deleted": deleted }),
        policy: None,
        audit: AuditMeta { recorded: false },
    })
}

pub(super) fn op_focus_compose(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let brain_id = brain_id_from_req(req)?;
    let session_id = req.args["session_id"].as_str().or(req.context.session_id.as_deref());
    let task_hint = req.args["task_hint"].as_str().unwrap_or("");
    let max_tokens = req.args["max_tokens"]
        .as_u64()
        .unwrap_or(128_000)
        .min(1_000_000) as usize;

    let agent_id = &req.actor.agent_id;
    if agent_id.is_empty() {
        return Ok(fail_response(
            "invalid_args",
            "brain.focus.compose requires agent_id (actor)".to_string(),
        ));
    }

    let sid = session_id.ok_or_else(|| anyhow::anyhow!("brain.focus.compose requires session_id"))?;
    let persona: Option<&str> = req.args["persona"].as_str();

    let budget = crate::context::TokenBudget::new(max_tokens);
    let split = req.args["overrides"].as_object().and_then(|o| {
        if o.is_empty() {
            return None;
        }
        Some(crate::context::BudgetSplit {
            facts_expanded_pct: o.get("facts_expanded_pct").and_then(|v| v.as_f64()).unwrap_or(0.20),
            scratch_pct: o.get("scratch_pct").and_then(|v| v.as_f64()).unwrap_or(0.10),
            log_pct: o.get("log_pct").and_then(|v| v.as_f64()).unwrap_or(0.30),
            knowledge_pct: o.get("knowledge_pct").and_then(|v| v.as_f64()).unwrap_or(0.40),
        })
    });

    let segments = crate::context::assemble(
        conn,
        agent_id,
        sid,
        persona,
        task_hint,
        &budget,
        split.as_ref(),
    )?;

    let total_tokens: usize = segments.iter().map(|s| s.token_estimate).sum();
    let segments_summary: Vec<serde_json::Value> = segments
        .iter()
        .map(|s| {
            json!({
                "label": s.label,
                "token_estimate": s.token_estimate,
                "content_preview": s.content.chars().take(100).collect::<String>()
            })
        })
        .collect();

    let composition_id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp();
    conn.execute(
        "INSERT INTO composition_logs (id, brain_id, session_id, task_hint, max_tokens, segments_json, total_tokens, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![
            composition_id,
            brain_id,
            sid,
            if task_hint.is_empty() { None::<&str> } else { Some(task_hint) },
            max_tokens as i64,
            serde_json::to_string(&segments_summary).unwrap_or_default(),
            total_tokens as i64,
            now,
        ],
    )?;

    let segments_data: Vec<serde_json::Value> = segments
        .iter()
        .map(|s| json!({ "label": s.label, "content": s.content, "token_estimate": s.token_estimate }))
        .collect();

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: format!("composed {} segments, {} tokens", segments.len(), total_tokens),
        data: json!({
            "composition_id": composition_id,
            "segments": segments_data,
            "total_tokens": total_tokens,
            "max_tokens": max_tokens,
        }),
        policy: None,
        audit: AuditMeta { recorded: false },
    })
}

pub(super) fn op_focus_composition_log(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let composition_id = req.args["composition_id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'composition_id'"))?;

    let row: Option<(String, String, Option<String>, Option<String>, i64, Option<String>, i64, i64)> = conn
        .query_row(
            "SELECT id, brain_id, session_id, task_hint, max_tokens, segments_json, total_tokens, created_at
             FROM composition_logs WHERE id = ?1",
            [composition_id],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                    r.get(7)?,
                ))
            },
        )
        .ok();

    match row {
        Some((id, brain_id, session_id, task_hint, max_tokens, segments_json, total_tokens, created_at)) => {
            let segments: serde_json::Value = segments_json
                .as_ref()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or(serde_json::Value::Array(vec![]));

            Ok(OperationResponse {
                ok: true,
                code: "ok".into(),
                message: String::new(),
                data: json!({
                    "composition_id": id,
                    "brain_id": brain_id,
                    "session_id": session_id,
                    "task_hint": task_hint,
                    "max_tokens": max_tokens,
                    "total_tokens": total_tokens,
                    "segments": segments,
                    "created_at": created_at,
                }),
                policy: None,
                audit: AuditMeta { recorded: false },
            })
        }
        None => Ok(fail_response(
            "not_found",
            format!("composition '{composition_id}' not found"),
        )),
    }
}

pub(super) fn op_focus_composition_last(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let brain_id = brain_id_from_req(req)?;
    let session_id = req.args["session_id"].as_str().or(req.context.session_id.as_deref());

    let row: Option<(String, String, Option<String>, Option<String>, i64, Option<String>, i64, i64)> = if let Some(sid) = session_id {
        conn.query_row(
            "SELECT id, brain_id, session_id, task_hint, max_tokens, segments_json, total_tokens, created_at
             FROM composition_logs WHERE brain_id = ?1 AND session_id = ?2 ORDER BY created_at DESC LIMIT 1",
            rusqlite::params![brain_id, sid],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                    r.get(7)?,
                ))
            },
        )
        .ok()
    } else {
        conn.query_row(
            "SELECT id, brain_id, session_id, task_hint, max_tokens, segments_json, total_tokens, created_at
             FROM composition_logs WHERE brain_id = ?1 ORDER BY created_at DESC LIMIT 1",
            [&brain_id],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                    r.get(7)?,
                ))
            },
        )
        .ok()
    };

    match row {
        Some((id, brain_id, session_id, task_hint, max_tokens, segments_json, total_tokens, created_at)) => {
            let segments: serde_json::Value = segments_json
                .as_ref()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or(serde_json::Value::Array(vec![]));

            Ok(OperationResponse {
                ok: true,
                code: "ok".into(),
                message: String::new(),
                data: json!({
                    "composition_id": id,
                    "brain_id": brain_id,
                    "session_id": session_id,
                    "task_hint": task_hint,
                    "max_tokens": max_tokens,
                    "total_tokens": total_tokens,
                    "segments": segments,
                    "created_at": created_at,
                }),
                policy: None,
                audit: AuditMeta { recorded: false },
            })
        }
        None => Ok(fail_response(
            "not_found",
            "no composition found for brain".to_string(),
        )),
    }
}
