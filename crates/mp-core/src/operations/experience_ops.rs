//! Brain.memories.experience operations.

use rusqlite::Connection;
use serde_json::json;

use super::{AuditMeta, OperationRequest, OperationResponse};

fn brain_id_from_req(req: &OperationRequest) -> anyhow::Result<String> {
    let from_args = req.args["brain_id"].as_str().map(String::from);
    let from_ctx = req.context.brain_id.clone();
    let from_actor = (!req.actor.agent_id.is_empty()).then(|| req.actor.agent_id.clone());

    from_args
        .or(from_ctx)
        .or(from_actor)
        .ok_or_else(|| anyhow::anyhow!("missing brain_id and no default from context/actor"))
}

pub(super) fn op_experience_record(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let brain_id = brain_id_from_req(req)?;
    let case_type = req.args["type"]
        .as_str()
        .unwrap_or("failure")
        .to_string();
    let tool = req.args["tool"].as_str().map(String::from);
    let command = req.args["command"].as_str().map(String::from);
    let error_signature = req.args["error"].as_str().or_else(|| req.args["error_signature"].as_str()).map(String::from);
    let context = req.args["context"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let outcome = req.args["outcome"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let confidence = req.args["confidence"].as_f64();

    let input = crate::store::experience::RecordInput {
        brain_id: brain_id.clone(),
        case_type,
        tool,
        command,
        error_signature,
        context,
        outcome,
        confidence,
    };

    let case_id = crate::store::experience::record(conn, &input)?;

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: String::new(),
        data: json!({ "case_id": case_id }),
        policy: None,
        audit: AuditMeta { recorded: false },
    })
}

pub(super) fn op_experience_match(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let brain_id = brain_id_from_req(req)?;
    let case_type = req.args["type"].as_str();
    let tool = req.args["tool"].as_str();
    let command = req.args["command"].as_str();
    let error = req.args["error"].as_str();
    let limit = req.args["limit"].as_u64().and_then(|n| n.try_into().ok());

    let cases = crate::store::experience::r#match(
        conn,
        &brain_id,
        case_type,
        tool,
        command,
        error,
        limit,
    )?;

    let items: Vec<serde_json::Value> = cases
        .into_iter()
        .map(|c| {
            json!({
                "case_id": c.id,
                "type": c.case_type,
                "tool": c.tool,
                "command": c.command,
                "error_signature": c.error_signature,
                "context": c.context,
                "outcome": c.outcome,
                "confidence": c.confidence,
                "hit_count": c.hit_count,
            })
        })
        .collect();

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: String::new(),
        data: json!({ "cases": items }),
        policy: None,
        audit: AuditMeta { recorded: false },
    })
}

pub(super) fn op_experience_resolve(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let case_id = req.args["case_id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'case_id'"))?;
    let fix_text = req.args["fix_text"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'fix_text'"))?
        .to_string();
    let fix_type = req.args["fix_type"]
        .as_str()
        .unwrap_or("workaround")
        .to_string();

    crate::store::experience::resolve(conn, case_id, &fix_text, &fix_type)?;

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: String::new(),
        data: json!({ "case_id": case_id }),
        policy: None,
        audit: AuditMeta { recorded: false },
    })
}

pub(super) fn op_experience_ignore(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let case_id = req.args["case_id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'case_id'"))?;
    let reason = req.args["reason"]
        .as_str()
        .unwrap_or("suppressed")
        .to_string();

    crate::store::experience::ignore(conn, case_id, &reason)?;

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: String::new(),
        data: json!({ "case_id": case_id }),
        policy: None,
        audit: AuditMeta { recorded: false },
    })
}

pub(super) fn op_experience_search(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let brain_id = brain_id_from_req(req)?;
    let query = req.args["query"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'query'"))?;
    let limit = req.args["limit"].as_u64().and_then(|n| n.try_into().ok());

    let cases = crate::store::experience::search(conn, &brain_id, query, limit)?;

    let items: Vec<serde_json::Value> = cases
        .into_iter()
        .map(|c| {
            json!({
                "case_id": c.id,
                "type": c.case_type,
                "context": c.context,
                "outcome": c.outcome,
                "confidence": c.confidence,
                "status": c.status,
            })
        })
        .collect();

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: String::new(),
        data: json!({ "cases": items }),
        policy: None,
        audit: AuditMeta { recorded: false },
    })
}

pub(super) fn op_experience_stats(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let brain_id = brain_id_from_req(req)?;
    let window_days = req.args["window_days"].as_i64();
    let case_type = req.args["type"].as_str();

    let stats = crate::store::experience::stats(conn, &brain_id, window_days, case_type)?;

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: String::new(),
        data: stats,
        policy: None,
        audit: AuditMeta { recorded: false },
    })
}

pub(super) fn op_experience_compact(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let brain_id = brain_id_from_req(req)?;
    let min_confidence = req.args["min_confidence"].as_f64();
    let older_than_days = req.args["older_than_days"].as_i64();

    let deleted = crate::store::experience::compact(conn, &brain_id, min_confidence, older_than_days)?;

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: format!("compacted {} case(s)", deleted.len()),
        data: json!({ "deleted_count": deleted.len(), "deleted_ids": deleted }),
        policy: None,
        audit: AuditMeta { recorded: false },
    })
}
