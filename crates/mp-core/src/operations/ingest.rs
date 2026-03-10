use rusqlite::Connection;
use super::{AuditMeta, OperationRequest, OperationResponse, denied_response, evaluate_policy_with_request_context, fail_response, policy_meta};

pub(super) fn op_ingest_events(
    conn: &Connection,
    req: &OperationRequest,
) -> anyhow::Result<OperationResponse> {
    let source = req.args["source"].as_str().unwrap_or("openclaw");
    let file_path = req.args["file_path"].as_str();
    let replay = req.args["replay"].as_bool().unwrap_or(false);
    let project_slug = req.args["project_slug"].as_str();

    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "ingest",
            resource: crate::policy::resource::EVENTS,
            sql_content: None,
            channel: req.actor.channel.as_deref(),
            arguments: None,
        },
        req,
    )?;
    if matches!(decision.effect, crate::policy::Effect::Deny) {
        return Ok(denied_response(&decision));
    }

    if let Some(path) = file_path {
        let summary = crate::ingest::ingest_jsonl_file(
            conn,
            source,
            std::path::Path::new(path),
            replay,
            &req.actor.agent_id,
        )?;
        return Ok(OperationResponse {
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
        });
    }

    // Auto-discover and ingest sessions by source type
    let (sessions, converter, source_label): (
        Vec<std::path::PathBuf>,
        fn(&std::path::Path) -> anyhow::Result<Vec<String>>,
        &str,
    ) = match source {
        "claude-code" => (
            crate::ingest::discover_claude_code_sessions(project_slug),
            crate::ingest::convert_claude_code_session,
            "claude-code",
        ),
        "cursor" => (
            crate::ingest::discover_cursor_sessions(project_slug),
            crate::ingest::convert_cursor_session,
            "cursor",
        ),
        _ => {
            return Ok(fail_response(
                "invalid_args",
                format!("source '{source}' requires 'file_path' (auto-discovery only supports 'claude-code' and 'cursor')"),
            ));
        }
    };

    if sessions.is_empty() {
        return Ok(OperationResponse {
            ok: true,
            code: "ok".into(),
            message: format!("no {source_label} sessions found"),
            data: serde_json::json!({
                "source": source_label,
                "sessions_found": 0,
                "total_inserted": 0,
                "total_deduped": 0,
                "total_errors": 0,
            }),
            policy: Some(policy_meta(&decision)),
            audit: AuditMeta { recorded: true },
        });
    }

    let mut total_inserted: i64 = 0;
    let mut total_deduped: i64 = 0;
    let mut total_errors: i64 = 0;
    let mut sessions_processed: usize = 0;

    for session_path in &sessions {
        let lines = match converter(session_path) {
            Ok(l) => l,
            Err(_) => {
                total_errors += 1;
                continue;
            }
        };
        if lines.is_empty() {
            continue;
        }
        let tmp = match crate::ingest::write_temp_jsonl(&lines, source_label) {
            Ok(t) => t,
            Err(_) => {
                total_errors += 1;
                continue;
            }
        };
        match crate::ingest::ingest_jsonl_file(conn, source_label, &tmp, replay, &req.actor.agent_id) {
            Ok(summary) => {
                total_inserted += summary.inserted_count;
                total_deduped += summary.deduped_count;
                total_errors += summary.error_count;
                sessions_processed += 1;
            }
            Err(_) => {
                total_errors += 1;
            }
        }
    }

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: format!("{sessions_processed} {source_label} session(s) ingested"),
        data: serde_json::json!({
            "source": source_label,
            "sessions_found": sessions.len(),
            "sessions_processed": sessions_processed,
            "total_inserted": total_inserted,
            "total_deduped": total_deduped,
            "total_errors": total_errors,
        }),
        policy: Some(policy_meta(&decision)),
        audit: AuditMeta { recorded: true },
    })
}

pub(super) fn op_ingest_status(
    conn: &Connection,
    req: &OperationRequest,
) -> anyhow::Result<OperationResponse> {
    let source = req.args["source"].as_str();
    let status = req.args["status"].as_str();
    let file_path_like = req.args["file_path_like"].as_str();
    let limit = req.args["limit"].as_u64().unwrap_or(20) as usize;
    let rows = crate::ingest::recent_runs(conn, source, status, file_path_like, limit)?;
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

pub(super) fn op_ingest_replay(
    conn: &Connection,
    req: &OperationRequest,
) -> anyhow::Result<OperationResponse> {
    let run_id = req.args["run_id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'run_id'"))?;
    let dry_run = req.args["dry_run"].as_bool().unwrap_or(false);
    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "ingest",
            resource: crate::policy::resource::EVENTS,
            sql_content: None,
            channel: req.actor.channel.as_deref(),
            arguments: None,
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

pub(super) fn op_embedding_status(
    conn: &Connection,
    req: &OperationRequest,
) -> anyhow::Result<OperationResponse> {
    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "status",
            resource: crate::policy::resource::EMBEDDING_QUEUE,
            sql_content: None,
            channel: req.actor.channel.as_deref(),
            arguments: None,
        },
        req,
    )?;
    if matches!(decision.effect, crate::policy::Effect::Deny) {
        return Ok(denied_response(&decision));
    }

    let stats = crate::store::embedding::queue_stats(conn)?;
    let by_target = crate::store::embedding::queue_target_stats(conn)?;
    let rows: Vec<serde_json::Value> = by_target
        .iter()
        .map(|r| {
            serde_json::json!({
                "target": r.target,
                "total": r.total,
                "pending": r.pending,
                "retry": r.retry,
                "processing": r.processing,
                "dead": r.dead
            })
        })
        .collect();

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: "embedding queue status".into(),
        data: serde_json::json!({
            "total": stats.total,
            "pending": stats.pending,
            "retry": stats.retry,
            "processing": stats.processing,
            "dead": stats.dead,
            "by_target": rows
        }),
        policy: Some(policy_meta(&decision)),
        audit: AuditMeta { recorded: true },
    })
}

pub(super) fn op_embedding_retry_dead(
    conn: &Connection,
    req: &OperationRequest,
) -> anyhow::Result<OperationResponse> {
    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "retry_dead",
            resource: crate::policy::resource::EMBEDDING_QUEUE,
            sql_content: None,
            channel: req.actor.channel.as_deref(),
            arguments: None,
        },
        req,
    )?;
    if matches!(decision.effect, crate::policy::Effect::Deny) {
        return Ok(denied_response(&decision));
    }

    let target = req.args["target"].as_str();
    let limit = req.args["limit"].as_u64().unwrap_or(500) as usize;
    let revived = crate::store::embedding::retry_dead_jobs(conn, target, limit)?;

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: "embedding dead jobs revived".into(),
        data: serde_json::json!({
            "revived": revived,
            "target": target,
            "limit": limit
        }),
        policy: Some(policy_meta(&decision)),
        audit: AuditMeta { recorded: true },
    })
}

pub(super) fn op_embedding_backfill_enqueue(
    conn: &Connection,
    req: &OperationRequest,
) -> anyhow::Result<OperationResponse> {
    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "backfill_enqueue",
            resource: crate::policy::resource::EMBEDDING_QUEUE,
            sql_content: None,
            channel: req.actor.channel.as_deref(),
            arguments: None,
        },
        req,
    )?;
    if matches!(decision.effect, crate::policy::Effect::Deny) {
        return Ok(denied_response(&decision));
    }

    let agent_id = req.args["agent_id"]
        .as_str()
        .unwrap_or(&req.actor.agent_id)
        .to_string();
    let provider = req.args["provider"].as_str().unwrap_or("local");
    let model = req.args["model"]
        .as_str()
        .unwrap_or("nomic-embed-text-v1.5");
    let dimensions = req.args["dimensions"].as_u64().unwrap_or(768) as usize;
    let model_id = req.args["model_id"]
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| crate::store::embedding::model_identity(provider, model, dimensions));
    let limit = req.args["limit"].as_u64().unwrap_or(10_000) as usize;
    let queued = crate::store::embedding::enqueue_drift_jobs(conn, &agent_id, &model_id, limit)?;

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: "embedding backfill enqueue complete".into(),
        data: serde_json::json!({
            "agent_id": agent_id,
            "provider": provider,
            "model": model,
            "dimensions": dimensions,
            "model_id": model_id,
            "limit": limit,
            "queued": queued
        }),
        policy: Some(policy_meta(&decision)),
        audit: AuditMeta { recorded: true },
    })
}
