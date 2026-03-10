use rusqlite::Connection;
use super::{AuditMeta, OperationRequest, OperationResponse, denied_response, evaluate_policy_with_request_context, fail_response, policy_meta};

pub(super) fn op_job_create(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let name = req.args["name"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'name'"))?;
    let schedule = req.args["schedule"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'schedule'"))?;
    let job_type = req.args["job_type"].as_str().unwrap_or("prompt");
    let description = req.args["description"].as_str().map(|s| s.to_string());
    let agent_id = req.args["agent_id"]
        .as_str()
        .map(|s| s.to_string())
        .unwrap_or_else(|| req.actor.agent_id.clone());
    let payload = match req.args.get("payload") {
        Some(v) if v.is_string() => v.as_str().unwrap_or("{}").to_string(),
        Some(v) => serde_json::to_string(v).unwrap_or_else(|_| "{}".to_string()),
        None => "{}".to_string(),
    };

    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "create",
            resource: crate::policy::resource::JOB,
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
    let id = crate::scheduler::create_job(
        conn,
        &crate::scheduler::NewJob {
            agent_id: agent_id.clone(),
            name: name.to_string(),
            description,
            schedule: schedule.to_string(),
            next_run_at: now + 60,
            job_type: job_type.to_string(),
            payload,
            max_retries: req.args["max_retries"].as_i64(),
            retry_delay_ms: req.args["retry_delay_ms"].as_i64(),
            timeout_ms: req.args["timeout_ms"].as_i64(),
            overlap_policy: req.args["overlap_policy"].as_str().map(|s| s.to_string()),
        },
    )?;

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: "job created".into(),
        data: serde_json::json!({
            "id": id,
            "name": name,
            "schedule": schedule,
            "job_type": job_type,
            "agent_id": agent_id
        }),
        policy: Some(policy_meta(&decision)),
        audit: AuditMeta { recorded: true },
    })
}

pub(super) fn op_job_list(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let requested_agent = req.args["agent_id"].as_str().map(|s| s.to_string());
    let agent_id = requested_agent.as_deref().unwrap_or(&req.actor.agent_id);

    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "list",
            resource: crate::policy::resource::JOB,
            sql_content: None,
            channel: req.actor.channel.as_deref(),
            arguments: None,
        },
        req,
    )?;

    if matches!(decision.effect, crate::policy::Effect::Deny) {
        return Ok(denied_response(&decision));
    }

    let jobs = crate::scheduler::list_jobs(conn, Some(agent_id))?;
    let data: Vec<serde_json::Value> = jobs
        .iter()
        .map(|j| {
            serde_json::json!({
                "id": j.id,
                "name": j.name,
                "schedule": j.schedule,
                "status": j.status,
                "enabled": j.enabled,
                "job_type": j.job_type,
                "agent_id": j.agent_id
            })
        })
        .collect();

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: "jobs listed".into(),
        data: serde_json::json!(data),
        policy: Some(policy_meta(&decision)),
        audit: AuditMeta { recorded: true },
    })
}

pub(super) fn op_job_run(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let job_id = req.args["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'id'"))?;

    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "run",
            resource: crate::policy::resource::JOB,
            sql_content: None,
            channel: req.actor.channel.as_deref(),
            arguments: None,
        },
        req,
    )?;
    if matches!(decision.effect, crate::policy::Effect::Deny) {
        return Ok(denied_response(&decision));
    }

    let job = match crate::scheduler::get_job(conn, job_id)? {
        Some(j) => j,
        None => {
            return Ok(OperationResponse {
                ok: false,
                code: "not_found".into(),
                message: format!("job '{job_id}' not found"),
                data: serde_json::json!({}),
                policy: Some(policy_meta(&decision)),
                audit: AuditMeta { recorded: true },
            });
        }
    };

    let run = crate::scheduler::dispatch_job(conn, &job, &|j| {
        crate::scheduler::execute_job_payload(conn, j)
    })?;

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: "job run completed".into(),
        data: serde_json::json!({
            "job_id": job.id,
            "run_id": run.id,
            "status": run.status,
            "result": run.result
        }),
        policy: Some(policy_meta(&decision)),
        audit: AuditMeta { recorded: true },
    })
}

pub(super) fn op_job_history(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let job_id = req.args.get("id").and_then(|v| v.as_str());
    let limit = req.args["limit"].as_u64().unwrap_or(20) as usize;

    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "list",
            resource: crate::policy::resource::JOB_RUN,
            sql_content: None,
            channel: req.actor.channel.as_deref(),
            arguments: None,
        },
        req,
    )?;
    if matches!(decision.effect, crate::policy::Effect::Deny) {
        return Ok(denied_response(&decision));
    }

    let runs = crate::scheduler::list_runs(conn, job_id, limit)?;
    let data: Vec<serde_json::Value> = runs
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.id,
                "job_id": r.job_id,
                "agent_id": r.agent_id,
                "status": r.status,
                "result": r.result,
                "started_at": r.started_at,
                "ended_at": r.ended_at,
            })
        })
        .collect();

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: "job history".into(),
        data: serde_json::json!(data),
        policy: Some(policy_meta(&decision)),
        audit: AuditMeta { recorded: true },
    })
}

pub(super) fn op_job_pause(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let job_id = req.args["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'id'"))?;

    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "pause",
            resource: crate::policy::resource::JOB,
            sql_content: None,
            channel: req.actor.channel.as_deref(),
            arguments: None,
        },
        req,
    )?;
    if matches!(decision.effect, crate::policy::Effect::Deny) {
        return Ok(denied_response(&decision));
    }

    crate::scheduler::pause_job(conn, job_id)?;

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: "job paused".into(),
        data: serde_json::json!({
            "id": job_id,
            "status": "paused"
        }),
        policy: Some(policy_meta(&decision)),
        audit: AuditMeta { recorded: true },
    })
}

pub(super) fn op_job_resume(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let job_id = req.args["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'id'"))?;

    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "resume",
            resource: crate::policy::resource::JOB,
            sql_content: None,
            channel: req.actor.channel.as_deref(),
            arguments: None,
        },
        req,
    )?;
    if matches!(decision.effect, crate::policy::Effect::Deny) {
        return Ok(denied_response(&decision));
    }

    crate::scheduler::resume_job(conn, job_id)?;

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: "job resumed".into(),
        data: serde_json::json!({ "id": job_id }),
        policy: Some(policy_meta(&decision)),
        audit: AuditMeta { recorded: true },
    })
}

#[derive(Debug, Clone)]
struct JobSpecRecord {
    id: String,
    agent_id: String,
    intent: String,
    plan_json: String,
    job_name: String,
    schedule: String,
    job_type: String,
    payload_json: String,
    status: String,
    applied_job_id: Option<String>,
}

fn load_job_spec(conn: &Connection, spec_id: &str) -> anyhow::Result<Option<JobSpecRecord>> {
    let mut stmt = conn.prepare(
        "SELECT id, agent_id, intent, plan_json, job_name, schedule, job_type, payload_json, status, applied_job_id
         FROM job_specs
         WHERE id = ?1",
    )?;
    let row = stmt
        .query_row([spec_id], |r| {
            Ok(JobSpecRecord {
                id: r.get(0)?,
                agent_id: r.get(1)?,
                intent: r.get(2)?,
                plan_json: r.get(3)?,
                job_name: r.get(4)?,
                schedule: r.get(5)?,
                job_type: r.get(6)?,
                payload_json: r.get(7)?,
                status: r.get(8)?,
                applied_job_id: r.get(9)?,
            })
        })
        .ok();
    Ok(row)
}

pub(super) fn op_job_spec_plan(
    conn: &Connection,
    req: &OperationRequest,
) -> anyhow::Result<OperationResponse> {
    let intent = req.args["intent"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'intent'"))?;
    let job_name = req.args["job_name"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'job_name'"))?;
    let schedule = req.args["schedule"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'schedule'"))?;
    let job_type = req.args["job_type"].as_str().unwrap_or("prompt");
    let agent_id = req.args["agent_id"]
        .as_str()
        .map(|s| s.to_string())
        .unwrap_or_else(|| req.actor.agent_id.clone());
    let payload_json = match req.args.get("payload") {
        Some(v) if v.is_string() => v.as_str().unwrap_or("{}").to_string(),
        Some(v) => serde_json::to_string(v).unwrap_or_else(|_| "{}".to_string()),
        None => "{}".to_string(),
    };
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
            resource: crate::policy::resource::JOB_SPEC,
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
        "INSERT INTO job_specs
         (id, agent_id, intent, plan_json, job_name, schedule, job_type, payload_json, status, proposed_by, source_session_id, source_message_id, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'planned', ?9, ?10, ?11, ?12, ?13)",
        rusqlite::params![
            id,
            agent_id,
            intent,
            plan_json,
            job_name,
            schedule,
            job_type,
            payload_json,
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
        message: "job spec planned".into(),
        data: serde_json::json!({
            "spec_id": id,
            "status": "planned",
            "agent_id": agent_id,
            "job_name": job_name,
            "schedule": schedule,
            "job_type": job_type,
            "intent": intent
        }),
        policy: Some(policy_meta(&decision)),
        audit: AuditMeta { recorded: true },
    })
}

pub(super) fn op_job_spec_confirm(
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
            resource: crate::policy::resource::JOB_SPEC,
            sql_content: None,
            channel: req.actor.channel.as_deref(),
            arguments: None,
        },
        req,
    )?;
    if matches!(decision.effect, crate::policy::Effect::Deny) {
        return Ok(denied_response(&decision));
    }

    let spec = match load_job_spec(conn, spec_id)? {
        Some(s) => s,
        None => {
            return Ok(OperationResponse {
                ok: false,
                code: "not_found".into(),
                message: format!("job spec '{spec_id}' not found"),
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
            message: format!("job spec '{spec_id}' is already applied"),
            data: serde_json::json!({
                "spec_id": spec.id,
                "status": spec.status,
                "applied_job_id": spec.applied_job_id
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
                "job spec '{spec_id}' cannot be confirmed from status '{}'",
                spec.status
            ),
            data: serde_json::json!({
                "spec_id": spec.id,
                "status": spec.status
            }),
            policy: Some(policy_meta(&decision)),
            audit: AuditMeta { recorded: true },
        });
    }

    let now = chrono::Utc::now().timestamp();
    conn.execute(
        "UPDATE job_specs SET status = 'confirmed', updated_at = ?2 WHERE id = ?1",
        rusqlite::params![spec_id, now],
    )?;

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: "job spec confirmed".into(),
        data: serde_json::json!({
            "spec_id": spec.id,
            "status": "confirmed",
            "agent_id": spec.agent_id,
            "job_name": spec.job_name,
            "schedule": spec.schedule,
            "job_type": spec.job_type,
            "intent": spec.intent,
            "plan": serde_json::from_str::<serde_json::Value>(&spec.plan_json).unwrap_or_else(|_| serde_json::json!({}))
        }),
        policy: Some(policy_meta(&decision)),
        audit: AuditMeta { recorded: true },
    })
}

pub(super) fn op_job_spec_apply(
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
            resource: crate::policy::resource::JOB_SPEC,
            sql_content: None,
            channel: req.actor.channel.as_deref(),
            arguments: None,
        },
        req,
    )?;
    if matches!(decision.effect, crate::policy::Effect::Deny) {
        return Ok(denied_response(&decision));
    }

    let spec = match load_job_spec(conn, spec_id)? {
        Some(s) => s,
        None => {
            return Ok(OperationResponse {
                ok: false,
                code: "not_found".into(),
                message: format!("job spec '{spec_id}' not found"),
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
            message: "job spec already applied".into(),
            data: serde_json::json!({
                "spec_id": spec.id,
                "status": "applied",
                "job_id": spec.applied_job_id
            }),
            policy: Some(policy_meta(&decision)),
            audit: AuditMeta { recorded: true },
        });
    }
    if spec.status != "confirmed" {
        return Ok(OperationResponse {
            ok: false,
            code: "invalid_state".into(),
            message: format!("job spec '{spec_id}' must be confirmed before apply"),
            data: serde_json::json!({
                "spec_id": spec.id,
                "status": spec.status
            }),
            policy: Some(policy_meta(&decision)),
            audit: AuditMeta { recorded: true },
        });
    }

    let now = chrono::Utc::now().timestamp();
    let job_id = crate::scheduler::create_job(
        conn,
        &crate::scheduler::NewJob {
            agent_id: spec.agent_id.clone(),
            name: spec.job_name.clone(),
            description: Some(format!("applied from job spec {}", spec.id)),
            schedule: spec.schedule.clone(),
            next_run_at: now + 60,
            job_type: spec.job_type.clone(),
            payload: spec.payload_json.clone(),
            max_retries: req.args["max_retries"].as_i64(),
            retry_delay_ms: req.args["retry_delay_ms"].as_i64(),
            timeout_ms: req.args["timeout_ms"].as_i64(),
            overlap_policy: req.args["overlap_policy"].as_str().map(|s| s.to_string()),
        },
    )?;
    conn.execute(
        "UPDATE job_specs
         SET status = 'applied',
             applied_job_id = ?2,
             updated_at = ?3
         WHERE id = ?1",
        rusqlite::params![spec.id, job_id, now],
    )?;

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: "job spec applied".into(),
        data: serde_json::json!({
            "spec_id": spec.id,
            "status": "applied",
            "job_id": job_id,
            "agent_id": spec.agent_id,
            "job_name": spec.job_name,
            "schedule": spec.schedule,
            "job_type": spec.job_type
        }),
        policy: Some(policy_meta(&decision)),
        audit: AuditMeta { recorded: true },
    })
}
