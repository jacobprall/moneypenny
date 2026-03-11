//! Brain.memories.events operations.

use rusqlite::Connection;
use serde_json::json;

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

pub(super) fn op_events_append(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let brain_id = brain_id_from_req(req)?;
    let event_type = req.args["event_type"]
        .as_str()
        .or_else(|| req.args["event"].as_str())
        .unwrap_or("custom")
        .to_string();
    let action = req.args["action"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'action'"))?
        .to_string();
    let resource = req.args["resource"].as_str().map(String::from);
    let actor = req.args["actor"].as_str().map(String::from);
    let session_id = req.args["session_id"].as_str().map(String::from);
    let correlation_id = req.args["correlation_id"].as_str().map(String::from);
    let detail = req.args["detail"]
        .as_str()
        .map(String::from)
        .or_else(|| req.args["detail"].as_object().map(|_| req.args["detail"].to_string()));

    let input = crate::store::events::AppendInput {
        brain_id: brain_id.clone(),
        event_type,
        action,
        resource,
        actor,
        session_id,
        correlation_id,
        detail,
    };

    let event_id = crate::store::events::append(conn, &input)?;

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: String::new(),
        data: json!({ "event_id": event_id }),
        policy: None,
        audit: AuditMeta { recorded: false },
    })
}

pub(super) fn op_events_query(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let brain_id = brain_id_from_req(req)?;
    let event_type = req.args["event_type"].as_str().or_else(|| req.args["event"].as_str());
    let action = req.args["action"].as_str();
    let resource = req.args["resource"].as_str();
    let session_id = req.args["session_id"].as_str();
    let query = req.args["query"].as_str();
    let limit = req.args["limit"].as_u64().and_then(|n| n.try_into().ok());

    let rows = crate::store::events::query(
        conn,
        &brain_id,
        event_type,
        action,
        resource,
        session_id,
        query,
        limit,
    )?;

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: format!("{} event(s)", rows.len()),
        data: json!({ "events": rows }),
        policy: None,
        audit: AuditMeta { recorded: false },
    })
}

pub(super) fn op_events_compact(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let brain_id = brain_id_from_req(req)?;
    let older_than_days = req.args["older_than_days"]
        .as_i64()
        .ok_or_else(|| anyhow::anyhow!("missing 'older_than_days'"))?;
    let confirm = req.args["confirm"].as_bool().unwrap_or(false);

    if !confirm {
        return Ok(fail_response(
            "confirmation_required",
            "brain.memories.events.compact requires confirm: true".to_string(),
        ));
    }

    let deleted = crate::store::events::compact(conn, &brain_id, older_than_days)?;

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: format!("compacted {deleted} event(s)"),
        data: json!({ "deleted_count": deleted }),
        policy: None,
        audit: AuditMeta { recorded: false },
    })
}
