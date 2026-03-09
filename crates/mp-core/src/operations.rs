use rusqlite::Connection;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorContext {
    pub agent_id: String,
    pub tenant_id: Option<String>,
    pub user_id: Option<String>,
    pub channel: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OperationContext {
    pub session_id: Option<String>,
    pub trace_id: Option<String>,
    pub timestamp: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationRequest {
    pub op: String,
    pub op_version: Option<String>,
    pub request_id: Option<String>,
    pub idempotency_key: Option<String>,
    pub actor: ActorContext,
    pub context: OperationContext,
    pub args: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyMeta {
    pub effect: String,
    pub policy_id: Option<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditMeta {
    pub recorded: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationResponse {
    pub ok: bool,
    pub code: String,
    pub message: String,
    pub data: serde_json::Value,
    pub policy: Option<PolicyMeta>,
    pub audit: AuditMeta,
}

pub fn execute(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let mut idempotency_state = if req.idempotency_key.is_some() {
        "provided_unenforced"
    } else {
        "not_provided"
    };

    if is_idempotent_mutation(req.op.as_str()) {
        if let Some(key) = req.idempotency_key.as_deref() {
            let fingerprint = request_fingerprint(req)?;
            if let Some(stored) = load_idempotency_record(conn, req, key)? {
                if stored.request_fingerprint != fingerprint {
                    let mut conflict = fail_response(
                        "idempotency_conflict",
                        "idempotency key was already used with different arguments".to_string(),
                    );
                    idempotency_state = "conflict";
                    record_idempotency_audit_event(
                        conn,
                        req,
                        idempotency_state,
                        Some(conflict.message.as_str()),
                    )?;
                    annotate_response_metadata(req, &mut conflict, idempotency_state);
                    return Ok(conflict);
                }

                let mut replayed: OperationResponse = serde_json::from_str(&stored.response_json)
                    .map_err(|e| {
                    anyhow::anyhow!("failed to decode stored idempotent response: {e}")
                })?;
                bump_idempotency_replay(conn, stored.id.as_str())?;
                idempotency_state = "replayed";
                record_idempotency_audit_event(
                    conn,
                    req,
                    idempotency_state,
                    Some("replayed stored response"),
                )?;
                annotate_response_metadata(req, &mut replayed, idempotency_state);
                return Ok(replayed);
            }
            idempotency_state = "provided_enforced";
        }
    }

    if let Some(mut aborted) = run_pre_hooks(conn, req)? {
        annotate_response_metadata(req, &mut aborted, idempotency_state);
        return Ok(aborted);
    }

    let mut resp = dispatch_operation(conn, req)?;

    if is_policy_required(req.op.as_str()) && resp.ok && resp.policy.is_none() {
        let mut failed = fail_response(
            "policy_missing",
            format!("operation '{}' completed without policy metadata", req.op),
        );
        annotate_response_metadata(req, &mut failed, idempotency_state);
        return Ok(failed);
    }

    run_post_hooks(conn, req, &mut resp)?;

    if is_idempotent_mutation(req.op.as_str()) {
        if let Some(key) = req.idempotency_key.as_deref() {
            let fingerprint = request_fingerprint(req)?;
            store_idempotency_record(conn, req, key, &fingerprint, &resp)?;
        }
    }

    annotate_response_metadata(req, &mut resp, idempotency_state);
    Ok(resp)
}

fn dispatch_operation(
    conn: &Connection,
    req: &OperationRequest,
) -> anyhow::Result<OperationResponse> {
    match req.op.as_str() {
        "job.create" => op_job_create(conn, req),
        "job.list" => op_job_list(conn, req),
        "job.run" => op_job_run(conn, req),
        "job.pause" => op_job_pause(conn, req),
        "job.resume" => op_job_resume(conn, req),
        "job.history" => op_job_history(conn, req),
        "job.spec.plan" => op_job_spec_plan(conn, req),
        "job.spec.confirm" => op_job_spec_confirm(conn, req),
        "job.spec.apply" => op_job_spec_apply(conn, req),
        "policy.add" => op_policy_add(conn, req),
        "policy.spec.plan" => op_policy_spec_plan(conn, req),
        "policy.spec.confirm" => op_policy_spec_confirm(conn, req),
        "policy.spec.apply" => op_policy_spec_apply(conn, req),
        "knowledge.ingest" => op_knowledge_ingest(conn, req),
        "memory.search" => op_memory_search(conn, req),
        "memory.fact.add" => op_memory_fact_add(conn, req),
        "memory.fact.update" => op_memory_fact_update(conn, req),
        "memory.fact.get" => op_memory_fact_get(conn, req),
        "memory.fact.compaction.reset" => op_memory_fact_compaction_reset(conn, req),
        "skill.add" => op_skill_add(conn, req),
        "skill.promote" => op_skill_promote(conn, req),
        "fact.delete" => op_fact_delete(conn, req),
        "policy.evaluate" => op_policy_evaluate(conn, req),
        "policy.explain" => op_policy_explain(conn, req),
        "audit.query" => op_audit_query(conn, req),
        "audit.append" => op_audit_append(conn, req),
        "session.resolve" => op_session_resolve(conn, req),
        "session.list" => op_session_list(conn, req),
        "js.tool.add" => op_js_tool_add(conn, req),
        "js.tool.list" => op_js_tool_list(conn, req),
        "js.tool.delete" => op_js_tool_delete(conn, req),
        "agent.create" => op_agent_create(conn, req),
        "agent.delete" => op_agent_delete(conn, req),
        "agent.config" => op_agent_config(conn, req),
        "ingest.events" => op_ingest_events(conn, req),
        "ingest.status" => op_ingest_status(conn, req),
        "ingest.replay" => op_ingest_replay(conn, req),
        "embedding.status" => op_embedding_status(conn, req),
        "embedding.retry_dead" => op_embedding_retry_dead(conn, req),
        "embedding.backfill.enqueue" => op_embedding_backfill_enqueue(conn, req),
        _ => Ok(fail_response(
            "invalid_args",
            format!("unknown operation '{}'", req.op),
        )),
    }
}

fn op_job_create(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
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
            resource: "job",
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

fn op_job_list(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let requested_agent = req.args["agent_id"].as_str().map(|s| s.to_string());
    let agent_id = requested_agent.as_deref().unwrap_or(&req.actor.agent_id);

    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "list",
            resource: "job",
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

fn op_job_run(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let job_id = req.args["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'id'"))?;

    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "run",
            resource: "job",
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

fn op_job_history(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let job_id = req.args.get("id").and_then(|v| v.as_str());
    let limit = req.args["limit"].as_u64().unwrap_or(20) as usize;

    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "list",
            resource: "job_run",
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

fn op_job_pause(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let job_id = req.args["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'id'"))?;

    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "pause",
            resource: "job",
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

fn op_job_resume(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let job_id = req.args["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'id'"))?;

    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "resume",
            resource: "job",
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

fn op_job_spec_plan(
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
            resource: "job_spec",
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

fn op_job_spec_confirm(
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
            resource: "job_spec",
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

fn op_job_spec_apply(
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
            resource: "job_spec",
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

fn op_policy_add(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
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
            resource: "policy",
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
        "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern,
         argument_pattern, channel_pattern, sql_pattern, rule_type, rule_config, message, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        rusqlite::params![id, name, priority, effect, actor_pattern, action_pattern, resource_pattern,
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

fn op_policy_spec_plan(
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
            resource: "policy_spec",
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

fn op_policy_spec_confirm(
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
            resource: "policy_spec",
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

fn op_policy_spec_apply(
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
            resource: "policy_spec",
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
    conn.execute(
        "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern,
         argument_pattern, channel_pattern, sql_pattern, rule_type, rule_config, message, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        rusqlite::params![
            policy_id, spec.policy_name, spec.priority, spec.effect,
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

fn op_policy_evaluate(
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

fn op_policy_explain(
    conn: &Connection,
    req: &OperationRequest,
) -> anyhow::Result<OperationResponse> {
    let mut resp = op_policy_evaluate(conn, req)?;
    resp.message = "policy explanation generated".into();
    Ok(resp)
}

fn op_audit_query(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let limit = req.args["limit"].as_u64().unwrap_or(50).clamp(1, 500) as i64;
    let effect = req.args["effect"].as_str().map(|s| s.to_string());
    let actor = req.args["actor"].as_str().map(|s| s.to_string());
    let action = req.args["action"].as_str().map(|s| s.to_string());
    let resource = req.args["resource"].as_str().map(|s| s.to_string());
    let session_id = req.args["session_id"].as_str().map(|s| s.to_string());
    let query = req.args["query"].as_str().map(|q| format!("%{q}%"));

    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "query",
            resource: "audit",
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
         ORDER BY created_at DESC
         LIMIT ?7",
    )?;
    let rows = stmt
        .query_map(
            rusqlite::params![effect, actor, action, resource, session_id, query, limit],
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

fn op_audit_append(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
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
            resource: "audit",
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

fn op_session_resolve(
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
            resource: "session",
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

fn op_session_list(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let target_agent = req.args["agent_id"].as_str().unwrap_or(&req.actor.agent_id);
    let limit = req.args["limit"].as_u64().unwrap_or(20).clamp(1, 200) as i64;

    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "list",
            resource: "session",
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

fn op_js_tool_add(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
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
            resource: "js_tool",
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

fn op_js_tool_list(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "list",
            resource: "js_tool",
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

fn op_js_tool_delete(
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
            resource: "js_tool",
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

fn op_knowledge_ingest(
    conn: &Connection,
    req: &OperationRequest,
) -> anyhow::Result<OperationResponse> {
    let content = req.args["content"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'content'"))?;
    let path = req.args["path"].as_str();
    let title = req.args["title"].as_str();
    let metadata = req.args["metadata"].as_str();

    let is_url = path.map_or(false, |p| {
        p.starts_with("http://") || p.starts_with("https://")
    });
    let resource = if is_url { "knowledge:url" } else { "knowledge" };

    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "ingest",
            resource,
            sql_content: None,
            channel: req.actor.channel.as_deref(),
            arguments: if is_url { path } else { None },
        },
        req,
    )?;
    if matches!(decision.effect, crate::policy::Effect::Deny) {
        return Ok(denied_response(&decision));
    }

    let (doc_id, chunk_count) =
        crate::store::knowledge::ingest(conn, path, title, content, metadata)?;

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: "knowledge ingested".into(),
        data: serde_json::json!({
            "document_id": doc_id,
            "chunks_created": chunk_count
        }),
        policy: Some(policy_meta(&decision)),
        audit: AuditMeta { recorded: true },
    })
}

fn op_memory_search(
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
            resource: "memory",
            sql_content: None,
            channel: req.actor.channel.as_deref(),
            arguments: None,
        },
        req,
    )?;
    if matches!(decision.effect, crate::policy::Effect::Deny) {
        return Ok(denied_response(&decision));
    }

    let rows = crate::search::search(conn, query, agent_id, limit, None, None)?;
    let data = rows
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "id": r.id,
                "store": format!("{:?}", r.store),
                "content": r.content,
                "score": r.score,
            })
        })
        .collect::<Vec<_>>();

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: "memory search completed".into(),
        data: serde_json::json!(data),
        policy: Some(policy_meta(&decision)),
        audit: AuditMeta { recorded: true },
    })
}

fn op_memory_fact_add(
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
    let reason = req.args["reason"]
        .as_str()
        .or(Some("added via canonical operation"));
    let keywords = req.args["keywords"].as_str().map(|s| s.to_string());

    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "add",
            resource: "fact",
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
            "summary": summary,
            "pointer": pointer,
            "confidence": confidence
        }),
        policy: Some(policy_meta(&decision)),
        audit: AuditMeta { recorded: true },
    })
}

fn op_memory_fact_update(
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
            resource: "fact",
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

fn op_memory_fact_get(
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
            resource: "fact",
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

fn op_memory_fact_compaction_reset(
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
            resource: "fact",
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

fn op_skill_add(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
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
            resource: "skill",
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

fn op_skill_promote(
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
            resource: "skill",
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

fn op_fact_delete(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let fact_id = req.args["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'id'"))?;
    let reason = req.args["reason"].as_str();

    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "delete",
            resource: "fact",
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

fn op_agent_create(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let name = req.args["name"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'name'"))?;
    let metadata_db_path = req.args["metadata_db_path"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'metadata_db_path'"))?;
    let agent_db_path = req.args["agent_db_path"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'agent_db_path'"))?;
    let trust_level = req.args["trust_level"].as_str().unwrap_or("standard");
    let llm_provider = req.args["llm_provider"].as_str().unwrap_or("local");
    let llm_model = req.args["llm_model"].as_str();
    let persona = req.args["persona"].as_str();

    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "create",
            resource: "agent",
            sql_content: None,
            channel: req.actor.channel.as_deref(),
            arguments: None,
        },
        req,
    )?;
    if matches!(decision.effect, crate::policy::Effect::Deny) {
        return Ok(denied_response(&decision));
    }

    let meta_conn = crate::db::open(std::path::Path::new(metadata_db_path))?;
    crate::schema::init_metadata_db(&meta_conn)?;
    if crate::gateway::get_agent(&meta_conn, name)?.is_some() {
        return Ok(OperationResponse {
            ok: false,
            code: "already_exists".into(),
            message: format!("agent '{name}' already exists"),
            data: serde_json::json!({}),
            policy: Some(policy_meta(&decision)),
            audit: AuditMeta { recorded: true },
        });
    }

    let db_path = std::path::PathBuf::from(agent_db_path);
    if let Some(parent) = db_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let agent_conn = crate::db::open(&db_path)?;
    crate::schema::init_agent_db(&agent_conn)?;
    crate::tools::registry::register_builtins(&agent_conn)?;
    crate::tools::registry::register_runtime_skills(&agent_conn)?;

    let now = chrono::Utc::now().timestamp();
    let id = uuid::Uuid::new_v4().to_string();
    meta_conn.execute(
        "INSERT INTO agents (id, name, persona, trust_level, llm_provider, llm_model, db_path, sync_enabled, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, ?8)",
        rusqlite::params![id, name, persona, trust_level, llm_provider, llm_model, db_path.to_string_lossy(), now],
    )?;

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: "agent created".into(),
        data: serde_json::json!({
            "id": id,
            "name": name,
            "db_path": db_path.to_string_lossy(),
        }),
        policy: Some(policy_meta(&decision)),
        audit: AuditMeta { recorded: true },
    })
}

fn op_agent_delete(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let name = req.args["name"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'name'"))?;
    let metadata_db_path = req.args["metadata_db_path"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'metadata_db_path'"))?;

    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "delete",
            resource: "agent",
            sql_content: None,
            channel: req.actor.channel.as_deref(),
            arguments: None,
        },
        req,
    )?;
    if matches!(decision.effect, crate::policy::Effect::Deny) {
        return Ok(denied_response(&decision));
    }

    let meta_conn = crate::db::open(std::path::Path::new(metadata_db_path))?;
    crate::schema::init_metadata_db(&meta_conn)?;
    let existing = match crate::gateway::get_agent(&meta_conn, name)? {
        Some(v) => v,
        None => {
            return Ok(OperationResponse {
                ok: false,
                code: "not_found".into(),
                message: format!("agent '{name}' not found"),
                data: serde_json::json!({}),
                policy: Some(policy_meta(&decision)),
                audit: AuditMeta { recorded: true },
            });
        }
    };

    meta_conn.execute("DELETE FROM agents WHERE name = ?1", [name])?;
    let _ = std::fs::remove_file(&existing.db_path);

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: "agent deleted".into(),
        data: serde_json::json!({
            "name": name,
            "db_path": existing.db_path,
        }),
        policy: Some(policy_meta(&decision)),
        audit: AuditMeta { recorded: true },
    })
}

fn op_agent_config(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let name = req.args["name"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'name'"))?;
    let key = req.args["key"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'key'"))?;
    let value = req.args["value"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'value'"))?;
    let metadata_db_path = req.args["metadata_db_path"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'metadata_db_path'"))?;

    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "config",
            resource: "agent",
            sql_content: None,
            channel: req.actor.channel.as_deref(),
            arguments: None,
        },
        req,
    )?;
    if matches!(decision.effect, crate::policy::Effect::Deny) {
        return Ok(denied_response(&decision));
    }

    let meta_conn = crate::db::open(std::path::Path::new(metadata_db_path))?;
    crate::schema::init_metadata_db(&meta_conn)?;
    if crate::gateway::get_agent(&meta_conn, name)?.is_none() {
        return Ok(OperationResponse {
            ok: false,
            code: "not_found".into(),
            message: format!("agent '{name}' not found"),
            data: serde_json::json!({}),
            policy: Some(policy_meta(&decision)),
            audit: AuditMeta { recorded: true },
        });
    }

    match key {
        "persona" => {
            meta_conn.execute(
                "UPDATE agents SET persona = ?1 WHERE name = ?2",
                rusqlite::params![value, name],
            )?;
        }
        "trust_level" => {
            meta_conn.execute(
                "UPDATE agents SET trust_level = ?1 WHERE name = ?2",
                rusqlite::params![value, name],
            )?;
        }
        "llm_provider" => {
            meta_conn.execute(
                "UPDATE agents SET llm_provider = ?1 WHERE name = ?2",
                rusqlite::params![value, name],
            )?;
        }
        "llm_model" => {
            meta_conn.execute(
                "UPDATE agents SET llm_model = ?1 WHERE name = ?2",
                rusqlite::params![value, name],
            )?;
        }
        "sync_enabled" => {
            let as_int = if value.eq_ignore_ascii_case("true") || value == "1" {
                1
            } else {
                0
            };
            meta_conn.execute(
                "UPDATE agents SET sync_enabled = ?1 WHERE name = ?2",
                rusqlite::params![as_int, name],
            )?;
        }
        _ => {
            return Ok(fail_response(
                "invalid_args",
                format!("unsupported config key '{}'", key),
            ));
        }
    }

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: "agent config updated".into(),
        data: serde_json::json!({
            "name": name,
            "key": key,
            "value": value,
        }),
        policy: Some(policy_meta(&decision)),
        audit: AuditMeta { recorded: true },
    })
}

fn op_ingest_events(
    conn: &Connection,
    req: &OperationRequest,
) -> anyhow::Result<OperationResponse> {
    let source = req.args["source"].as_str().unwrap_or("openclaw");
    let file_path = req.args["file_path"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'file_path'"))?;
    let replay = req.args["replay"].as_bool().unwrap_or(false);

    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "ingest",
            resource: "events",
            sql_content: None,
            channel: req.actor.channel.as_deref(),
            arguments: None,
        },
        req,
    )?;
    if matches!(decision.effect, crate::policy::Effect::Deny) {
        return Ok(denied_response(&decision));
    }

    let summary = crate::ingest::ingest_jsonl_file(
        conn,
        source,
        std::path::Path::new(file_path),
        replay,
        &req.actor.agent_id,
    )?;
    Ok(OperationResponse {
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
    })
}

fn op_ingest_status(
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

fn op_ingest_replay(
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
            resource: "events",
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

fn op_embedding_status(
    conn: &Connection,
    req: &OperationRequest,
) -> anyhow::Result<OperationResponse> {
    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "status",
            resource: "embedding_queue",
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

fn op_embedding_retry_dead(
    conn: &Connection,
    req: &OperationRequest,
) -> anyhow::Result<OperationResponse> {
    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "retry_dead",
            resource: "embedding_queue",
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

fn op_embedding_backfill_enqueue(
    conn: &Connection,
    req: &OperationRequest,
) -> anyhow::Result<OperationResponse> {
    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "backfill_enqueue",
            resource: "embedding_queue",
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

fn policy_meta(decision: &crate::policy::PolicyDecision) -> PolicyMeta {
    let effect = match decision.effect {
        crate::policy::Effect::Allow => "allow",
        crate::policy::Effect::Deny => "deny",
        crate::policy::Effect::Audit => "audit",
    }
    .to_string();
    PolicyMeta {
        effect,
        policy_id: decision.policy_id.clone(),
        reason: decision.reason.clone(),
    }
}

fn parse_policy_mode(mode: Option<&str>) -> Option<crate::policy::PolicyMode> {
    match mode {
        Some("deny_by_default") | Some("deny") => Some(crate::policy::PolicyMode::DenyByDefault),
        Some("allow_by_default") | Some("allow") => Some(crate::policy::PolicyMode::AllowByDefault),
        _ => None,
    }
}

fn denied_response(decision: &crate::policy::PolicyDecision) -> OperationResponse {
    OperationResponse {
        ok: false,
        code: "policy_denied".into(),
        message: decision
            .reason
            .clone()
            .unwrap_or_else(|| "operation denied by policy".into()),
        data: serde_json::json!({}),
        policy: Some(policy_meta(decision)),
        audit: AuditMeta { recorded: true },
    }
}

fn fail_response(code: &str, message: String) -> OperationResponse {
    OperationResponse {
        ok: false,
        code: code.into(),
        message,
        data: serde_json::json!({}),
        policy: None,
        audit: AuditMeta { recorded: false },
    }
}

fn is_policy_required(op: &str) -> bool {
    matches!(
        op,
        "job.create"
            | "job.list"
            | "job.run"
            | "job.pause"
            | "job.resume"
            | "job.history"
            | "job.spec.plan"
            | "job.spec.confirm"
            | "job.spec.apply"
            | "policy.add"
            | "policy.spec.plan"
            | "policy.spec.confirm"
            | "policy.spec.apply"
            | "memory.search"
            | "memory.fact.add"
            | "memory.fact.update"
            | "memory.fact.get"
            | "memory.fact.compaction.reset"
            | "policy.evaluate"
            | "policy.explain"
            | "audit.query"
            | "audit.append"
            | "session.resolve"
            | "session.list"
            | "js.tool.add"
            | "js.tool.list"
            | "js.tool.delete"
            | "knowledge.ingest"
            | "skill.add"
            | "skill.promote"
            | "fact.delete"
            | "agent.create"
            | "agent.delete"
            | "agent.config"
            | "ingest.events"
            | "ingest.replay"
            | "embedding.status"
            | "embedding.retry_dead"
            | "embedding.backfill.enqueue"
    )
}

fn is_idempotent_mutation(op: &str) -> bool {
    matches!(
        op,
        "job.create"
            | "job.run"
            | "job.pause"
            | "job.resume"
            | "job.spec.plan"
            | "job.spec.confirm"
            | "job.spec.apply"
            | "policy.add"
            | "policy.spec.plan"
            | "policy.spec.confirm"
            | "policy.spec.apply"
            | "memory.fact.add"
            | "memory.fact.update"
            | "memory.fact.compaction.reset"
            | "audit.append"
            | "session.resolve"
            | "js.tool.add"
            | "js.tool.delete"
            | "knowledge.ingest"
            | "skill.add"
            | "skill.promote"
            | "fact.delete"
            | "agent.create"
            | "agent.delete"
            | "agent.config"
            | "ingest.events"
            | "ingest.replay"
            | "embedding.retry_dead"
            | "embedding.backfill.enqueue"
    )
}

#[derive(Debug)]
struct StoredIdempotency {
    id: String,
    request_fingerprint: String,
    response_json: String,
}

fn request_fingerprint(req: &OperationRequest) -> anyhow::Result<String> {
    serde_json::to_string(&serde_json::json!({
        "op": req.op,
        "actor": req.actor.agent_id,
        "args": req.args,
    }))
    .map_err(|e| anyhow::anyhow!("failed to serialize idempotency request fingerprint: {e}"))
}

fn load_idempotency_record(
    conn: &Connection,
    req: &OperationRequest,
    key: &str,
) -> anyhow::Result<Option<StoredIdempotency>> {
    let mut stmt = conn.prepare(
        "SELECT id, request_fingerprint, response_json
         FROM operation_idempotency
         WHERE actor_id = ?1 AND op = ?2 AND idempotency_key = ?3
         LIMIT 1",
    )?;

    let row = stmt.query_row(rusqlite::params![req.actor.agent_id, req.op, key], |r| {
        Ok(StoredIdempotency {
            id: r.get(0)?,
            request_fingerprint: r.get(1)?,
            response_json: r.get(2)?,
        })
    });

    match row {
        Ok(v) => Ok(Some(v)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn store_idempotency_record(
    conn: &Connection,
    req: &OperationRequest,
    key: &str,
    fingerprint: &str,
    response: &OperationResponse,
) -> anyhow::Result<()> {
    let now = chrono::Utc::now().timestamp();
    let response_json = serde_json::to_string(response)
        .map_err(|e| anyhow::anyhow!("failed to serialize idempotent response: {e}"))?;

    conn.execute(
        "INSERT OR REPLACE INTO operation_idempotency
         (id, actor_id, op, idempotency_key, request_fingerprint, response_json, created_at, replay_count)
         VALUES (
            COALESCE(
                (SELECT id FROM operation_idempotency WHERE actor_id = ?1 AND op = ?2 AND idempotency_key = ?3),
                ?4
            ),
            ?1, ?2, ?3, ?5, ?6, ?7,
            COALESCE(
                (SELECT replay_count FROM operation_idempotency WHERE actor_id = ?1 AND op = ?2 AND idempotency_key = ?3),
                0
            )
         )",
        rusqlite::params![
            req.actor.agent_id,
            req.op,
            key,
            uuid::Uuid::new_v4().to_string(),
            fingerprint,
            response_json,
            now,
        ],
    )?;
    Ok(())
}

fn bump_idempotency_replay(conn: &Connection, record_id: &str) -> anyhow::Result<()> {
    let now = chrono::Utc::now().timestamp();
    conn.execute(
        "UPDATE operation_idempotency
         SET replay_count = replay_count + 1, last_replayed_at = ?2
         WHERE id = ?1",
        rusqlite::params![record_id, now],
    )?;
    Ok(())
}

fn record_idempotency_audit_event(
    conn: &Connection,
    req: &OperationRequest,
    state: &str,
    reason: Option<&str>,
) -> anyhow::Result<()> {
    let now = chrono::Utc::now().timestamp();
    conn.execute(
        "INSERT INTO policy_audit (
            id, policy_id, actor, action, resource, effect, reason, correlation_id, session_id, idempotency_key, idempotency_state, created_at
        ) VALUES (?1, NULL, ?2, ?3, ?4, 'audited', ?5, ?6, ?7, ?8, ?9, ?10)",
        rusqlite::params![
            uuid::Uuid::new_v4().to_string(),
            req.actor.agent_id,
            "idempotency",
            req.op,
            reason,
            req.request_id.as_deref().or(req.context.trace_id.as_deref()),
            req.context.session_id.as_deref(),
            req.idempotency_key.as_deref(),
            state,
            now,
        ],
    )?;
    Ok(())
}

#[derive(Debug)]
struct OperationHookRow {
    id: String,
    op_pattern: String,
    hook_type: String,
    config_json: String,
}

fn load_operation_hooks(conn: &Connection, phase: &str) -> anyhow::Result<Vec<OperationHookRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, op_pattern, hook_type, config_json
         FROM operation_hooks
         WHERE enabled = 1 AND phase = ?1
         ORDER BY created_at ASC, id ASC",
    )?;
    let rows = stmt
        .query_map([phase], |r| {
            Ok(OperationHookRow {
                id: r.get(0)?,
                op_pattern: r.get(1)?,
                hook_type: r.get(2)?,
                config_json: r.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn op_pattern_matches(pattern: &str, op: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return pattern == op;
    }

    let mut pos = 0;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        match op[pos..].find(part) {
            Some(found) => {
                if i == 0 && found != 0 {
                    return false;
                }
                pos += found + part.len();
            }
            None => return false,
        }
    }
    if !pattern.ends_with('*') {
        return pos == op.len();
    }
    true
}

fn run_pre_hooks(
    conn: &Connection,
    req: &OperationRequest,
) -> anyhow::Result<Option<OperationResponse>> {
    // Hard guardrail: avoid oversized operation envelopes overwhelming runtime.
    let args_size = req.args.to_string().len();
    if args_size > 2_000_000 {
        return Ok(Some(fail_response(
            "invalid_args",
            format!("operation args too large: {} bytes", args_size),
        )));
    }

    let hooks = load_operation_hooks(conn, "pre")?;
    let args_text = req.args.to_string();
    for hook in hooks {
        if !op_pattern_matches(&hook.op_pattern, &req.op) {
            continue;
        }
        let cfg: serde_json::Value =
            serde_json::from_str(&hook.config_json).unwrap_or_else(|_| serde_json::json!({}));
        match hook.hook_type.as_str() {
            "deny_if_args_contains" => {
                let needle = cfg["needle"].as_str().unwrap_or("");
                if !needle.is_empty() && args_text.contains(needle) {
                    let msg = cfg["message"]
                        .as_str()
                        .unwrap_or("operation blocked by pre-hook")
                        .to_string();
                    tracing::debug!(hook_id = %hook.id, op = %req.op, "operation pre-hook denied request");
                    return Ok(Some(fail_response("hook_denied", msg)));
                }
            }
            "max_args_bytes" => {
                let max = cfg["max_bytes"].as_u64().unwrap_or(u64::MAX) as usize;
                if args_size > max {
                    let msg = cfg["message"]
                        .as_str()
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| {
                            format!(
                                "operation args exceeded hook limit: {} > {}",
                                args_size, max
                            )
                        });
                    tracing::debug!(hook_id = %hook.id, op = %req.op, "operation pre-hook rejected oversized args");
                    return Ok(Some(fail_response("hook_denied", msg)));
                }
            }
            _ => {}
        }
    }

    Ok(None)
}

fn run_post_hooks(
    conn: &Connection,
    req: &OperationRequest,
    resp: &mut OperationResponse,
) -> anyhow::Result<()> {
    // Standardized post-hook: redact accidental secret-like output text.
    resp.message = crate::store::redact::redact(&resp.message);

    let hooks = load_operation_hooks(conn, "post")?;
    for hook in hooks {
        if !op_pattern_matches(&hook.op_pattern, &req.op) {
            continue;
        }
        let cfg: serde_json::Value =
            serde_json::from_str(&hook.config_json).unwrap_or_else(|_| serde_json::json!({}));
        match hook.hook_type.as_str() {
            "append_message_suffix" => {
                if let Some(suffix) = cfg["suffix"].as_str() {
                    resp.message.push_str(suffix);
                }
            }
            "prepend_message_prefix" => {
                if let Some(prefix) = cfg["prefix"].as_str() {
                    resp.message = format!("{prefix}{}", resp.message);
                }
            }
            "truncate_message" => {
                let max_chars = cfg["max_chars"].as_u64().unwrap_or(u64::MAX) as usize;
                let mut out = String::new();
                for (i, ch) in resp.message.chars().enumerate() {
                    if i >= max_chars {
                        break;
                    }
                    out.push(ch);
                }
                if out.len() < resp.message.len() {
                    resp.message = out;
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn annotate_response_metadata(
    req: &OperationRequest,
    resp: &mut OperationResponse,
    idempotency_state: &str,
) {
    let correlation_id = req
        .request_id
        .as_deref()
        .or(req.context.trace_id.as_deref())
        .unwrap_or("")
        .to_string();
    let idempotency_key = req.idempotency_key.clone();
    let meta = serde_json::json!({
        "op": req.op,
        "op_version": req.op_version,
        "correlation_id": correlation_id,
        "idempotency_key": idempotency_key,
        "idempotency_state": idempotency_state,
    });

    if let serde_json::Value::Object(map) = &mut resp.data {
        map.insert("_meta".into(), meta);
    }
}

fn evaluate_policy_with_request_context(
    conn: &Connection,
    policy_req: &crate::policy::PolicyRequest,
    req: &OperationRequest,
) -> anyhow::Result<crate::policy::PolicyDecision> {
    let idempotency_state = if req.idempotency_key.is_some() {
        if is_idempotent_mutation(req.op.as_str()) {
            "provided_enforced"
        } else {
            "provided_unenforced"
        }
    } else {
        "not_provided"
    };
    let audit = crate::policy::PolicyAuditContext {
        session_id: req.context.session_id.as_deref(),
        correlation_id: req
            .request_id
            .as_deref()
            .or(req.context.trace_id.as_deref()),
        idempotency_key: req.idempotency_key.as_deref(),
        idempotency_state: Some(idempotency_state),
    };
    crate::policy::evaluate_with_audit(conn, policy_req, &audit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn setup() -> Connection {
        let conn = crate::db::open_memory().unwrap();
        crate::schema::init_agent_db(&conn).unwrap();
        conn
    }

    fn temp_db_path(prefix: &str) -> PathBuf {
        std::env::temp_dir().join(format!("mp-{prefix}-{}.db", uuid::Uuid::new_v4()))
    }

    fn temp_jsonl_file(prefix: &str, lines: &[&str]) -> PathBuf {
        let path = std::env::temp_dir().join(format!("mp-{prefix}-{}.jsonl", uuid::Uuid::new_v4()));
        let content = lines.join("\n");
        std::fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn job_create_succeeds_with_allow_policy() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, created_at)
             VALUES ('allow-all', 'allow all', 10, 'allow', '*', '*', '*', 1)",
            [],
        )
        .unwrap();

        let req = OperationRequest {
            op: "job.create".into(),
            op_version: Some("v1".into()),
            request_id: None,
            idempotency_key: Some("key-1".into()),
            actor: ActorContext {
                agent_id: "main".into(),
                tenant_id: None,
                user_id: None,
                channel: Some("cli".into()),
            },
            context: OperationContext::default(),
            args: serde_json::json!({
                "name": "daily",
                "schedule": "0 9 * * *",
                "job_type": "prompt",
                "payload": "{\"message\":\"hello\"}"
            }),
        };

        let resp = execute(&conn, &req).unwrap();
        assert!(resp.ok);
        assert_eq!(resp.code, "ok");
        assert_eq!(resp.data["name"], "daily");
    }

    #[test]
    fn job_list_returns_jobs() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, created_at)
             VALUES ('allow-all', 'allow all', 10, 'allow', '*', '*', '*', 1)",
            [],
        )
        .unwrap();

        let id = crate::scheduler::create_job(
            &conn,
            &crate::scheduler::NewJob {
                agent_id: "main".into(),
                name: "nightly".into(),
                description: None,
                schedule: "0 1 * * *".into(),
                next_run_at: chrono::Utc::now().timestamp() + 60,
                job_type: "prompt".into(),
                payload: "{}".into(),
                max_retries: None,
                retry_delay_ms: None,
                timeout_ms: None,
                overlap_policy: None,
            },
        )
        .unwrap();

        let req = OperationRequest {
            op: "job.list".into(),
            op_version: Some("v1".into()),
            request_id: None,
            idempotency_key: None,
            actor: ActorContext {
                agent_id: "main".into(),
                tenant_id: None,
                user_id: None,
                channel: Some("cli".into()),
            },
            context: OperationContext::default(),
            args: serde_json::json!({}),
        };
        let resp = execute(&conn, &req).unwrap();
        assert!(resp.ok);
        let rows = resp.data.as_array().unwrap();
        assert!(rows.iter().any(|r| r["id"] == id));
    }

    #[test]
    fn job_create_denied_by_policy() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, message, created_at)
             VALUES ('deny-job-create', 'deny create', 100, 'deny', '*', 'create', 'job', 'blocked', 1)",
            [],
        )
        .unwrap();

        let req = OperationRequest {
            op: "job.create".into(),
            op_version: Some("v1".into()),
            request_id: None,
            idempotency_key: None,
            actor: ActorContext {
                agent_id: "main".into(),
                tenant_id: None,
                user_id: None,
                channel: Some("cli".into()),
            },
            context: OperationContext::default(),
            args: serde_json::json!({
                "name": "blocked-job",
                "schedule": "* * * * *"
            }),
        };

        let resp = execute(&conn, &req).unwrap();
        assert!(!resp.ok);
        assert_eq!(resp.code, "policy_denied");
    }

    #[test]
    fn job_pause_updates_status() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, created_at)
             VALUES ('allow-all', 'allow all', 10, 'allow', '*', '*', '*', 1)",
            [],
        )
        .unwrap();

        let job_id = crate::scheduler::create_job(
            &conn,
            &crate::scheduler::NewJob {
                agent_id: "main".into(),
                name: "pause-me".into(),
                description: None,
                schedule: "* * * * *".into(),
                next_run_at: chrono::Utc::now().timestamp() + 60,
                job_type: "prompt".into(),
                payload: "{}".into(),
                max_retries: None,
                retry_delay_ms: None,
                timeout_ms: None,
                overlap_policy: None,
            },
        )
        .unwrap();

        let req = OperationRequest {
            op: "job.pause".into(),
            op_version: Some("v1".into()),
            request_id: None,
            idempotency_key: None,
            actor: ActorContext {
                agent_id: "main".into(),
                tenant_id: None,
                user_id: None,
                channel: Some("cli".into()),
            },
            context: OperationContext::default(),
            args: serde_json::json!({ "id": job_id }),
        };

        let resp = execute(&conn, &req).unwrap();
        assert!(resp.ok);
        let paused = crate::scheduler::get_job(&conn, resp.data["id"].as_str().unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(paused.status, "paused");
    }

    #[test]
    fn job_run_not_found() {
        let conn = setup();
        let req = OperationRequest {
            op: "job.run".into(),
            op_version: Some("v1".into()),
            request_id: None,
            idempotency_key: None,
            actor: ActorContext {
                agent_id: "main".into(),
                tenant_id: None,
                user_id: None,
                channel: Some("cli".into()),
            },
            context: OperationContext::default(),
            args: serde_json::json!({ "id": "missing" }),
        };

        let resp = execute(&conn, &req).unwrap();
        assert!(!resp.ok);
        assert_eq!(resp.code, "not_found");
        assert!(resp.policy.is_some());
    }

    #[test]
    fn job_spec_plan_confirm_apply_flow_creates_job() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, created_at)
             VALUES ('allow-all', 'allow all', 10, 'allow', '*', '*', '*', 1)",
            [],
        )
        .unwrap();

        let plan = execute(
            &conn,
            &OperationRequest {
                op: "job.spec.plan".into(),
                op_version: Some("v1".into()),
                request_id: Some("corr-job-spec-plan".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "intent": "daily digest for recent fact changes",
                    "job_name": "digest-spec-job",
                    "schedule": "0 9 * * *",
                    "job_type": "prompt",
                    "payload": {"message":"summarize yesterday changes"},
                    "plan": {"source":"agent"}
                }),
            },
        )
        .unwrap();
        assert!(plan.ok);
        let spec_id = plan.data["spec_id"].as_str().unwrap().to_string();
        assert_eq!(plan.data["status"], "planned");

        let confirm = execute(
            &conn,
            &OperationRequest {
                op: "job.spec.confirm".into(),
                op_version: Some("v1".into()),
                request_id: Some("corr-job-spec-confirm".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "spec_id": spec_id,
                    "confirm": true
                }),
            },
        )
        .unwrap();
        assert!(confirm.ok);
        assert_eq!(confirm.data["status"], "confirmed");

        let apply = execute(
            &conn,
            &OperationRequest {
                op: "job.spec.apply".into(),
                op_version: Some("v1".into()),
                request_id: Some("corr-job-spec-apply".into()),
                idempotency_key: Some("job-spec-apply-1".into()),
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "spec_id": confirm.data["spec_id"]
                }),
            },
        )
        .unwrap();
        assert!(apply.ok);
        assert_eq!(apply.data["status"], "applied");
        let job_id = apply.data["job_id"].as_str().unwrap().to_string();
        let created = crate::scheduler::get_job(&conn, &job_id).unwrap();
        assert!(created.is_some());
    }

    #[test]
    fn job_spec_apply_requires_confirmed_state() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, created_at)
             VALUES ('allow-all', 'allow all', 10, 'allow', '*', '*', '*', 1)",
            [],
        )
        .unwrap();

        let plan = execute(
            &conn,
            &OperationRequest {
                op: "job.spec.plan".into(),
                op_version: Some("v1".into()),
                request_id: Some("corr-job-spec-plan-unconfirmed".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "intent": "cleanup old pointers",
                    "job_name": "cleanup-spec-job",
                    "schedule": "0 2 * * *"
                }),
            },
        )
        .unwrap();
        assert!(plan.ok);

        let apply = execute(
            &conn,
            &OperationRequest {
                op: "job.spec.apply".into(),
                op_version: Some("v1".into()),
                request_id: Some("corr-job-spec-apply-unconfirmed".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "spec_id": plan.data["spec_id"]
                }),
            },
        )
        .unwrap();
        assert!(!apply.ok);
        assert_eq!(apply.code, "invalid_state");
    }

    #[test]
    fn oversized_operation_args_are_rejected_by_pre_hook() {
        let conn = setup();
        let req = OperationRequest {
            op: "job.list".into(),
            op_version: Some("v1".into()),
            request_id: None,
            idempotency_key: None,
            actor: ActorContext {
                agent_id: "main".into(),
                tenant_id: None,
                user_id: None,
                channel: Some("cli".into()),
            },
            context: OperationContext::default(),
            args: serde_json::json!({
                "blob": "x".repeat(2_000_100)
            }),
        };
        let resp = execute(&conn, &req).unwrap();
        assert!(!resp.ok);
        assert_eq!(resp.code, "invalid_args");
        assert!(resp.message.contains("too large"));
    }

    #[test]
    fn configured_pre_hook_can_block_operation() {
        let conn = setup();
        conn.execute(
            "INSERT INTO operation_hooks (id, op_pattern, phase, hook_type, config_json, enabled, created_at)
             VALUES ('h-pre-block', 'job.create', 'pre', 'deny_if_args_contains', '{\"needle\":\"blocked-job\",\"message\":\"blocked by configured hook\"}', 1, 1)",
            [],
        )
        .unwrap();

        let req = OperationRequest {
            op: "job.create".into(),
            op_version: Some("v1".into()),
            request_id: Some("corr-hook-pre".into()),
            idempotency_key: None,
            actor: ActorContext {
                agent_id: "main".into(),
                tenant_id: None,
                user_id: None,
                channel: Some("cli".into()),
            },
            context: OperationContext::default(),
            args: serde_json::json!({
                "name": "blocked-job",
                "schedule": "* * * * *"
            }),
        };
        let resp = execute(&conn, &req).unwrap();
        assert!(!resp.ok);
        assert_eq!(resp.code, "hook_denied");
        assert_eq!(resp.message, "blocked by configured hook");
    }

    #[test]
    fn configured_post_hook_can_mutate_message() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, created_at)
             VALUES ('allow-all', 'allow all', 10, 'allow', '*', '*', '*', 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO operation_hooks (id, op_pattern, phase, hook_type, config_json, enabled, created_at)
             VALUES ('h-post-suffix', 'job.create', 'post', 'append_message_suffix', '{\"suffix\":\" [hooked]\"}', 1, 1)",
            [],
        )
        .unwrap();

        let req = OperationRequest {
            op: "job.create".into(),
            op_version: Some("v1".into()),
            request_id: Some("corr-hook-post".into()),
            idempotency_key: None,
            actor: ActorContext {
                agent_id: "main".into(),
                tenant_id: None,
                user_id: None,
                channel: Some("cli".into()),
            },
            context: OperationContext::default(),
            args: serde_json::json!({
                "name": "hooked-message-job",
                "schedule": "* * * * *"
            }),
        };
        let resp = execute(&conn, &req).unwrap();
        assert!(resp.ok);
        assert!(resp.message.ends_with(" [hooked]"));
    }

    #[test]
    fn configured_hooks_do_not_bypass_policy_denial() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, message, created_at)
             VALUES ('deny-job-create', 'deny create', 100, 'deny', '*', 'create', 'job', 'blocked', 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO operation_hooks (id, op_pattern, phase, hook_type, config_json, enabled, created_at)
             VALUES ('h-post-prefix', 'job.create', 'post', 'prepend_message_prefix', '{\"prefix\":\"HOOK: \"}', 1, 1)",
            [],
        )
        .unwrap();

        let req = OperationRequest {
            op: "job.create".into(),
            op_version: Some("v1".into()),
            request_id: Some("corr-hook-policy".into()),
            idempotency_key: None,
            actor: ActorContext {
                agent_id: "main".into(),
                tenant_id: None,
                user_id: None,
                channel: Some("cli".into()),
            },
            context: OperationContext::default(),
            args: serde_json::json!({
                "name": "must-deny",
                "schedule": "* * * * *"
            }),
        };
        let resp = execute(&conn, &req).unwrap();
        assert!(!resp.ok);
        assert_eq!(resp.code, "policy_denied");
    }

    #[test]
    fn response_contains_idempotency_metadata() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, created_at)
             VALUES ('allow-all', 'allow all', 10, 'allow', '*', '*', '*', 1)",
            [],
        )
        .unwrap();
        let req = OperationRequest {
            op: "job.create".into(),
            op_version: Some("v1".into()),
            request_id: Some("corr-123".into()),
            idempotency_key: Some("idem-123".into()),
            actor: ActorContext {
                agent_id: "main".into(),
                tenant_id: None,
                user_id: None,
                channel: Some("cli".into()),
            },
            context: OperationContext::default(),
            args: serde_json::json!({
                "name": "meta-check",
                "schedule": "* * * * *"
            }),
        };
        let resp = execute(&conn, &req).unwrap();
        assert!(resp.ok);
        assert_eq!(resp.data["_meta"]["correlation_id"], "corr-123");
        assert_eq!(resp.data["_meta"]["idempotency_key"], "idem-123");
        assert_eq!(resp.data["_meta"]["idempotency_state"], "provided_enforced");
    }

    #[test]
    fn idempotency_replays_mutating_response() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, created_at)
             VALUES ('allow-all', 'allow all', 10, 'allow', '*', '*', '*', 1)",
            [],
        )
        .unwrap();

        let req = OperationRequest {
            op: "job.create".into(),
            op_version: Some("v1".into()),
            request_id: Some("corr-idem-first".into()),
            idempotency_key: Some("idem-replay-1".into()),
            actor: ActorContext {
                agent_id: "main".into(),
                tenant_id: None,
                user_id: None,
                channel: Some("cli".into()),
            },
            context: OperationContext::default(),
            args: serde_json::json!({
                "name": "idem-job",
                "schedule": "* * * * *"
            }),
        };

        let first = execute(&conn, &req).unwrap();
        assert!(first.ok);
        let first_id = first.data["id"].as_str().unwrap().to_string();

        let second = execute(
            &conn,
            &OperationRequest {
                request_id: Some("corr-idem-second".into()),
                ..req
            },
        )
        .unwrap();
        assert!(second.ok);
        assert_eq!(second.data["id"], first_id);
        assert_eq!(second.data["_meta"]["idempotency_state"], "replayed");

        let job_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM jobs WHERE name = 'idem-job'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(job_count, 1);
    }

    #[test]
    fn idempotency_conflict_rejects_changed_args() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, created_at)
             VALUES ('allow-all', 'allow all', 10, 'allow', '*', '*', '*', 1)",
            [],
        )
        .unwrap();

        let first = OperationRequest {
            op: "job.create".into(),
            op_version: Some("v1".into()),
            request_id: Some("corr-idem-conflict-first".into()),
            idempotency_key: Some("idem-conflict-1".into()),
            actor: ActorContext {
                agent_id: "main".into(),
                tenant_id: None,
                user_id: None,
                channel: Some("cli".into()),
            },
            context: OperationContext::default(),
            args: serde_json::json!({
                "name": "idem-conflict-job",
                "schedule": "* * * * *"
            }),
        };
        let first_resp = execute(&conn, &first).unwrap();
        assert!(first_resp.ok);

        let second = OperationRequest {
            args: serde_json::json!({
                "name": "idem-conflict-job",
                "schedule": "0 * * * *"
            }),
            request_id: Some("corr-idem-conflict-second".into()),
            ..first
        };
        let second_resp = execute(&conn, &second).unwrap();
        assert!(!second_resp.ok);
        assert_eq!(second_resp.code, "idempotency_conflict");
        assert_eq!(second_resp.data["_meta"]["idempotency_state"], "conflict");
    }

    #[test]
    fn policy_audit_records_idempotency_state() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, created_at)
             VALUES ('allow-all', 'allow all', 10, 'allow', '*', '*', '*', 1)",
            [],
        )
        .unwrap();

        let req = OperationRequest {
            op: "job.create".into(),
            op_version: Some("v1".into()),
            request_id: Some("corr-idem-audit".into()),
            idempotency_key: Some("idem-audit-1".into()),
            actor: ActorContext {
                agent_id: "main".into(),
                tenant_id: None,
                user_id: None,
                channel: Some("cli".into()),
            },
            context: OperationContext::default(),
            args: serde_json::json!({
                "name": "idem-audit-job",
                "schedule": "* * * * *"
            }),
        };

        let resp = execute(&conn, &req).unwrap();
        assert!(resp.ok);

        let (key, state): (Option<String>, Option<String>) = conn
            .query_row(
                "SELECT idempotency_key, idempotency_state
                 FROM policy_audit
                 WHERE correlation_id = 'corr-idem-audit'
                 LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(key.as_deref(), Some("idem-audit-1"));
        assert_eq!(state.as_deref(), Some("provided_enforced"));
    }

    #[test]
    fn policy_add_creates_rule() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, created_at)
             VALUES ('allow-all', 'allow all', 10, 'allow', '*', '*', '*', 1)",
            [],
        )
        .unwrap();

        let req = OperationRequest {
            op: "policy.add".into(),
            op_version: Some("v1".into()),
            request_id: None,
            idempotency_key: None,
            actor: ActorContext {
                agent_id: "main".into(),
                tenant_id: None,
                user_id: None,
                channel: Some("cli".into()),
            },
            context: OperationContext::default(),
            args: serde_json::json!({
                "name": "deny-shell",
                "effect": "deny",
                "actor_pattern": "*",
                "action_pattern": "call",
                "resource_pattern": "tool:shell_*",
                "message": "blocked"
            }),
        };

        let resp = execute(&conn, &req).unwrap();
        assert!(resp.ok);
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM policies WHERE name = 'deny-shell'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn policy_add_accepts_all_fields() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, created_at)
             VALUES ('allow-all', 'allow all', 10, 'allow', '*', '*', '*', 1)",
            [],
        ).unwrap();

        let resp = execute(
            &conn,
            &OperationRequest {
                op: "policy.add".into(),
                op_version: Some("v1".into()),
                request_id: None,
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "name": "whitelist-docs",
                    "effect": "allow",
                    "priority": 100,
                    "action_pattern": "ingest",
                    "resource_pattern": "knowledge:url",
                    "argument_pattern": "https://docs.example.com/*",
                    "message": "Whitelisted domain"
                }),
            },
        )
        .unwrap();
        assert!(resp.ok);
        assert_eq!(resp.data["priority"].as_i64(), Some(100));

        let (pri, arg_pat): (i64, Option<String>) = conn
            .query_row(
                "SELECT priority, argument_pattern FROM policies WHERE name = 'whitelist-docs'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(pri, 100);
        assert_eq!(arg_pat.as_deref(), Some("https://docs.example.com/*"));
    }

    #[test]
    fn policy_spec_plan_confirm_apply_flow() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, created_at)
             VALUES ('allow-all', 'allow all', 10, 'allow', '*', '*', '*', 1)",
            [],
        ).unwrap();

        let actor = ActorContext {
            agent_id: "main".into(),
            tenant_id: None,
            user_id: None,
            channel: Some("agent".into()),
        };

        let plan_resp = execute(
            &conn,
            &OperationRequest {
                op: "policy.spec.plan".into(),
                op_version: Some("v1".into()),
                request_id: None,
                idempotency_key: None,
                actor: actor.clone(),
                context: OperationContext::default(),
                args: serde_json::json!({
                    "intent": "only allow docs.example.com URLs",
                    "policy_name": "whitelist-docs",
                    "effect": "allow",
                    "priority": 100,
                    "action_pattern": "ingest",
                    "resource_pattern": "knowledge:url",
                    "argument_pattern": "https://docs.example.com/*",
                    "message": "Whitelisted"
                }),
            },
        )
        .unwrap();
        assert!(plan_resp.ok);
        assert_eq!(plan_resp.data["status"].as_str(), Some("planned"));
        let spec_id = plan_resp.data["spec_id"].as_str().unwrap().to_string();

        let status: String = conn
            .query_row(
                "SELECT status FROM policy_specs WHERE id = ?1",
                [&spec_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(status, "planned");

        let confirm_resp = execute(
            &conn,
            &OperationRequest {
                op: "policy.spec.confirm".into(),
                op_version: Some("v1".into()),
                request_id: None,
                idempotency_key: None,
                actor: actor.clone(),
                context: OperationContext::default(),
                args: serde_json::json!({ "spec_id": spec_id }),
            },
        )
        .unwrap();
        assert!(confirm_resp.ok);
        assert_eq!(confirm_resp.data["status"].as_str(), Some("confirmed"));

        let apply_resp = execute(
            &conn,
            &OperationRequest {
                op: "policy.spec.apply".into(),
                op_version: Some("v1".into()),
                request_id: None,
                idempotency_key: None,
                actor: actor.clone(),
                context: OperationContext::default(),
                args: serde_json::json!({ "spec_id": spec_id }),
            },
        )
        .unwrap();
        assert!(apply_resp.ok);
        assert_eq!(apply_resp.data["status"].as_str(), Some("applied"));

        let policy_id = apply_resp.data["policy_id"].as_str().unwrap();
        let (name, effect, pri, arg_pat): (String, String, i64, Option<String>) = conn
            .query_row(
                "SELECT name, effect, priority, argument_pattern FROM policies WHERE id = ?1",
                [policy_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap();
        assert_eq!(name, "whitelist-docs");
        assert_eq!(effect, "allow");
        assert_eq!(pri, 100);
        assert_eq!(arg_pat.as_deref(), Some("https://docs.example.com/*"));
    }

    #[test]
    fn policy_spec_apply_requires_confirmed_state() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, created_at)
             VALUES ('allow-all', 'allow all', 10, 'allow', '*', '*', '*', 1)",
            [],
        ).unwrap();

        let actor = ActorContext {
            agent_id: "main".into(),
            tenant_id: None,
            user_id: None,
            channel: Some("agent".into()),
        };

        let plan_resp = execute(
            &conn,
            &OperationRequest {
                op: "policy.spec.plan".into(),
                op_version: Some("v1".into()),
                request_id: None,
                idempotency_key: None,
                actor: actor.clone(),
                context: OperationContext::default(),
                args: serde_json::json!({
                    "intent": "test",
                    "policy_name": "test-policy",
                    "effect": "deny",
                }),
            },
        )
        .unwrap();
        assert!(plan_resp.ok);
        let spec_id = plan_resp.data["spec_id"].as_str().unwrap().to_string();

        let apply_resp = execute(
            &conn,
            &OperationRequest {
                op: "policy.spec.apply".into(),
                op_version: Some("v1".into()),
                request_id: None,
                idempotency_key: None,
                actor: actor.clone(),
                context: OperationContext::default(),
                args: serde_json::json!({ "spec_id": spec_id }),
            },
        )
        .unwrap();
        assert!(!apply_resp.ok, "apply should fail on unconfirmed spec");
        assert_eq!(apply_resp.code, "invalid_state");
    }

    #[test]
    fn memory_fact_add_update_and_search_work() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, created_at)
             VALUES ('allow-all', 'allow all', 10, 'allow', '*', '*', '*', 1)",
            [],
        )
        .unwrap();

        let add = execute(
            &conn,
            &OperationRequest {
                op: "memory.fact.add".into(),
                op_version: Some("v1".into()),
                request_id: Some("corr-memory-add".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "content": "Stripe webhooks power billing events",
                    "summary": "billing uses Stripe webhooks",
                    "pointer": "billing:stripe-webhooks"
                }),
            },
        )
        .unwrap();
        assert!(add.ok);
        let fact_id = add.data["id"].as_str().unwrap().to_string();

        let update = execute(
            &conn,
            &OperationRequest {
                op: "memory.fact.update".into(),
                op_version: Some("v1".into()),
                request_id: Some("corr-memory-update".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "id": fact_id,
                    "content": "Stripe webhooks and retries power billing events",
                    "summary": "billing uses Stripe webhooks + retries",
                    "pointer": "billing:stripe-webhooks-retries"
                }),
            },
        )
        .unwrap();
        assert!(update.ok);

        let search = execute(
            &conn,
            &OperationRequest {
                op: "memory.search".into(),
                op_version: Some("v1".into()),
                request_id: Some("corr-memory-search".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "query": "Stripe",
                    "limit": 10
                }),
            },
        )
        .unwrap();
        assert!(search.ok);
        let rows = search.data.as_array().unwrap();
        assert!(!rows.is_empty());
    }

    #[test]
    fn memory_fact_get_and_compaction_reset_work() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, created_at)
             VALUES ('allow-all', 'allow all', 10, 'allow', '*', '*', '*', 1)",
            [],
        )
        .unwrap();

        let add = execute(
            &conn,
            &OperationRequest {
                op: "memory.fact.add".into(),
                op_version: Some("v1".into()),
                request_id: Some("corr-memory-add-2".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "content": "Checkout service retries payment intents before marking failure",
                    "summary": "checkout retries payment intents",
                    "pointer": "checkout:payment-retries"
                }),
            },
        )
        .unwrap();
        assert!(add.ok);
        let fact_id = add.data["id"].as_str().unwrap().to_string();

        // Force compaction so reset has something to clear.
        let compacted = crate::store::facts::compact_for_context(&conn, "main").unwrap();
        assert!(compacted >= 1);

        let get = execute(
            &conn,
            &OperationRequest {
                op: "memory.fact.get".into(),
                op_version: Some("v1".into()),
                request_id: Some("corr-memory-get".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "id": fact_id
                }),
            },
        )
        .unwrap();
        assert!(get.ok);
        assert_eq!(get.data["pointer"], "checkout:payment-retries");
        assert!(get.data["compaction_level"].as_i64().unwrap_or(0) >= 1);

        let reset = execute(
            &conn,
            &OperationRequest {
                op: "memory.fact.compaction.reset".into(),
                op_version: Some("v1".into()),
                request_id: Some("corr-memory-reset".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "id": get.data["id"],
                    "reason": "test reset"
                }),
            },
        )
        .unwrap();
        assert!(reset.ok);
        assert_eq!(reset.data["compaction_level"], 0);
        assert!(reset.data["context_compact"].is_null());
    }

    #[test]
    fn policy_evaluate_and_explain_work() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, message, created_at)
             VALUES ('deny-shell', 'deny shell', 100, 'deny', '*', 'call', 'tool:shell_*', 'Shell blocked', 1)",
            [],
        )
        .unwrap();

        let eval = execute(
            &conn,
            &OperationRequest {
                op: "policy.evaluate".into(),
                op_version: Some("v1".into()),
                request_id: Some("corr-policy-eval".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "actor": "agent:main",
                    "action": "call",
                    "resource": "tool:shell_exec"
                }),
            },
        )
        .unwrap();
        assert!(eval.ok);
        assert_eq!(eval.data["effect"], "deny");

        let explain = execute(
            &conn,
            &OperationRequest {
                op: "policy.explain".into(),
                op_version: Some("v1".into()),
                request_id: Some("corr-policy-explain".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "actor": "agent:main",
                    "action": "call",
                    "resource": "tool:shell_exec"
                }),
            },
        )
        .unwrap();
        assert!(explain.ok);
        assert_eq!(explain.data["effect"], "deny");
    }

    #[test]
    fn audit_append_and_query_work() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, created_at)
             VALUES ('allow-all', 'allow all', 10, 'allow', '*', '*', '*', 1)",
            [],
        )
        .unwrap();

        let append = execute(
            &conn,
            &OperationRequest {
                op: "audit.append".into(),
                op_version: Some("v1".into()),
                request_id: Some("corr-audit-append".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "actor": "agent:main",
                    "action": "test.append",
                    "resource": "audit",
                    "effect": "audited",
                    "reason": "manual append"
                }),
            },
        )
        .unwrap();
        assert!(append.ok);

        let query = execute(
            &conn,
            &OperationRequest {
                op: "audit.query".into(),
                op_version: Some("v1".into()),
                request_id: Some("corr-audit-query".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "query": "manual append",
                    "limit": 10
                }),
            },
        )
        .unwrap();
        assert!(query.ok);
        let rows = query.data.as_array().unwrap();
        assert!(!rows.is_empty());
    }

    #[test]
    fn session_resolve_and_list_work() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, created_at)
             VALUES ('allow-all', 'allow all', 10, 'allow', '*', '*', '*', 1)",
            [],
        )
        .unwrap();

        let resolve = execute(
            &conn,
            &OperationRequest {
                op: "session.resolve".into(),
                op_version: Some("v1".into()),
                request_id: Some("corr-session-resolve".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "channel": "cli"
                }),
            },
        )
        .unwrap();
        assert!(resolve.ok);

        let list = execute(
            &conn,
            &OperationRequest {
                op: "session.list".into(),
                op_version: Some("v1".into()),
                request_id: Some("corr-session-list".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "limit": 10
                }),
            },
        )
        .unwrap();
        assert!(list.ok);
        let rows = list.data.as_array().unwrap();
        assert!(!rows.is_empty());
    }

    #[test]
    fn js_tool_add_list_delete_work() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, created_at)
             VALUES ('allow-all', 'allow all', 10, 'allow', '*', '*', '*', 1)",
            [],
        )
        .unwrap();

        let add = execute(
            &conn,
            &OperationRequest {
                op: "js.tool.add".into(),
                op_version: Some("v1".into()),
                request_id: Some("corr-js-add".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "name": "js_calc",
                    "description": "test tool",
                    "script": "function run(args){ return {ok:true}; }"
                }),
            },
        )
        .unwrap();
        assert!(add.ok);

        let list = execute(
            &conn,
            &OperationRequest {
                op: "js.tool.list".into(),
                op_version: Some("v1".into()),
                request_id: Some("corr-js-list".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({}),
            },
        )
        .unwrap();
        assert!(list.ok);
        let rows = list.data.as_array().unwrap();
        assert!(rows.iter().any(|r| r["name"] == "js_calc"));

        let del = execute(
            &conn,
            &OperationRequest {
                op: "js.tool.delete".into(),
                op_version: Some("v1".into()),
                request_id: Some("corr-js-del".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "name": "js_calc"
                }),
            },
        )
        .unwrap();
        assert!(del.ok);
    }

    #[test]
    fn knowledge_ingest_creates_document() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, created_at)
             VALUES ('allow-all', 'allow all', 10, 'allow', '*', '*', '*', 1)",
            [],
        )
        .unwrap();

        let req = OperationRequest {
            op: "knowledge.ingest".into(),
            op_version: Some("v1".into()),
            request_id: None,
            idempotency_key: None,
            actor: ActorContext {
                agent_id: "main".into(),
                tenant_id: None,
                user_id: None,
                channel: Some("cli".into()),
            },
            context: OperationContext::default(),
            args: serde_json::json!({
                "title": "Test",
                "content": "# Header\nBody"
            }),
        };

        let resp = execute(&conn, &req).unwrap();
        assert!(resp.ok);
        assert!(resp.data["document_id"].as_str().is_some());
    }

    #[test]
    fn skill_add_and_promote_work() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, created_at)
             VALUES ('allow-all', 'allow all', 10, 'allow', '*', '*', '*', 1)",
            [],
        )
        .unwrap();

        let add_req = OperationRequest {
            op: "skill.add".into(),
            op_version: Some("v1".into()),
            request_id: Some("corr-skill-add".into()),
            idempotency_key: None,
            actor: ActorContext {
                agent_id: "main".into(),
                tenant_id: None,
                user_id: None,
                channel: Some("cli".into()),
            },
            context: OperationContext::default(),
            args: serde_json::json!({
                "name": "SkillTest",
                "description": "test",
                "content": "Do the thing"
            }),
        };
        let add_resp = execute(&conn, &add_req).unwrap();
        assert!(add_resp.ok);
        let skill_id = add_resp.data["id"].as_str().unwrap().to_string();

        let promote_req = OperationRequest {
            op: "skill.promote".into(),
            op_version: Some("v1".into()),
            request_id: Some("corr-skill-promote".into()),
            idempotency_key: None,
            actor: ActorContext {
                agent_id: "main".into(),
                tenant_id: None,
                user_id: None,
                channel: Some("cli".into()),
            },
            context: OperationContext::default(),
            args: serde_json::json!({ "id": skill_id }),
        };
        let promote_resp = execute(&conn, &promote_req).unwrap();
        assert!(promote_resp.ok);
        assert_eq!(promote_resp.data["promoted"], true);
    }

    #[test]
    fn fact_delete_returns_not_found_for_missing_fact() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, created_at)
             VALUES ('allow-all', 'allow all', 10, 'allow', '*', '*', '*', 1)",
            [],
        )
        .unwrap();

        let req = OperationRequest {
            op: "fact.delete".into(),
            op_version: Some("v1".into()),
            request_id: Some("corr-fact-delete-missing".into()),
            idempotency_key: None,
            actor: ActorContext {
                agent_id: "main".into(),
                tenant_id: None,
                user_id: None,
                channel: Some("cli".into()),
            },
            context: OperationContext::default(),
            args: serde_json::json!({ "id": "missing-fact" }),
        };
        let resp = execute(&conn, &req).unwrap();
        assert!(!resp.ok);
        assert_eq!(resp.code, "not_found");
    }

    #[test]
    fn agent_create_config_delete_work() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, created_at)
             VALUES ('allow-all', 'allow all', 10, 'allow', '*', '*', '*', 1)",
            [],
        )
        .unwrap();

        let meta_path = temp_db_path("meta");
        let agent_path = temp_db_path("agent");
        let meta_path_s = meta_path.to_string_lossy().to_string();
        let agent_path_s = agent_path.to_string_lossy().to_string();

        let create_resp = execute(
            &conn,
            &OperationRequest {
                op: "agent.create".into(),
                op_version: Some("v1".into()),
                request_id: Some("corr-agent-create".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "name": "agent-x",
                    "metadata_db_path": meta_path_s,
                    "agent_db_path": agent_path_s,
                }),
            },
        )
        .unwrap();
        assert!(create_resp.ok);

        let config_resp = execute(
            &conn,
            &OperationRequest {
                op: "agent.config".into(),
                op_version: Some("v1".into()),
                request_id: Some("corr-agent-config".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "name": "agent-x",
                    "key": "trust_level",
                    "value": "elevated",
                    "metadata_db_path": meta_path.to_string_lossy().to_string(),
                }),
            },
        )
        .unwrap();
        assert!(config_resp.ok);

        let delete_resp = execute(
            &conn,
            &OperationRequest {
                op: "agent.delete".into(),
                op_version: Some("v1".into()),
                request_id: Some("corr-agent-delete".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "name": "agent-x",
                    "metadata_db_path": meta_path.to_string_lossy().to_string(),
                }),
            },
        )
        .unwrap();
        assert!(delete_resp.ok);

        let _ = std::fs::remove_file(meta_path);
        let _ = std::fs::remove_file(agent_path);
    }

    #[test]
    fn ingest_events_is_replay_safe() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, created_at)
             VALUES ('allow-all', 'allow all', 10, 'allow', '*', '*', '*', 1)",
            [],
        )
        .unwrap();
        let file = temp_jsonl_file(
            "ingest-events",
            &[
                r#"{"event_id":"e1","type":"session.state","session_id":"s1","timestamp":100}"#,
                r#"{"event_id":"e2","type":"message.processed","session_id":"s1","role":"assistant","content":"hello","timestamp":101}"#,
            ],
        );

        let first = execute(
            &conn,
            &OperationRequest {
                op: "ingest.events".into(),
                op_version: Some("v1".into()),
                request_id: Some("corr-ingest-first".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "source": "openclaw",
                    "file_path": file.to_string_lossy().to_string(),
                    "replay": true
                }),
            },
        )
        .unwrap();
        assert!(first.ok);
        assert_eq!(first.data["inserted_count"], 2);

        let second = execute(
            &conn,
            &OperationRequest {
                op: "ingest.events".into(),
                op_version: Some("v1".into()),
                request_id: Some("corr-ingest-second".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "source": "openclaw",
                    "file_path": file.to_string_lossy().to_string(),
                    "replay": true
                }),
            },
        )
        .unwrap();
        assert!(second.ok);
        assert_eq!(second.data["inserted_count"], 0);
        assert_eq!(second.data["deduped_count"], 2);

        let _ = std::fs::remove_file(file);
    }

    #[test]
    fn ingest_status_and_replay_work() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, created_at)
             VALUES ('allow-all', 'allow all', 10, 'allow', '*', '*', '*', 1)",
            [],
        )
        .unwrap();
        let file = temp_jsonl_file(
            "ingest-status-replay",
            &[
                r#"{"event_id":"sre1","type":"session.state","session_id":"sre-s1","timestamp":100}"#,
                r#"{"event_id":"sre2","type":"message.processed","session_id":"sre-s1","role":"assistant","content":"hello","timestamp":101}"#,
            ],
        );
        let ingest = execute(
            &conn,
            &OperationRequest {
                op: "ingest.events".into(),
                op_version: Some("v1".into()),
                request_id: Some("corr-ingest-status-source".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "source": "openclaw",
                    "file_path": file.to_string_lossy().to_string(),
                    "replay": true
                }),
            },
        )
        .unwrap();
        assert!(ingest.ok);

        let status = execute(
            &conn,
            &OperationRequest {
                op: "ingest.status".into(),
                op_version: Some("v1".into()),
                request_id: Some("corr-ingest-status".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "source": "openclaw",
                    "limit": 5
                }),
            },
        )
        .unwrap();
        assert!(status.ok);
        let rows = status.data.as_array().unwrap();
        assert!(!rows.is_empty());
        let first_run_id = rows[0]["id"].as_str().unwrap().to_string();

        let replay = execute(
            &conn,
            &OperationRequest {
                op: "ingest.replay".into(),
                op_version: Some("v1".into()),
                request_id: Some("corr-ingest-replay".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "run_id": first_run_id
                }),
            },
        )
        .unwrap();
        assert!(replay.ok);
        assert_eq!(replay.data["inserted_count"], 0);
        assert_eq!(replay.data["deduped_count"], 2);

        let preflight = execute(
            &conn,
            &OperationRequest {
                op: "ingest.replay".into(),
                op_version: Some("v1".into()),
                request_id: Some("corr-ingest-replay-dry".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "run_id": first_run_id,
                    "dry_run": true
                }),
            },
        )
        .unwrap();
        assert!(preflight.ok);
        assert_eq!(preflight.data["would_insert_count"], 0);
        assert_eq!(preflight.data["would_dedupe_count"], 2);

        let _ = std::fs::remove_file(file);
    }

    #[test]
    fn ingest_status_supports_source_and_file_filters() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, created_at)
             VALUES ('allow-all', 'allow all', 10, 'allow', '*', '*', '*', 1)",
            [],
        )
        .unwrap();
        let file_a = temp_jsonl_file(
            "ingest-status-filter-a",
            &[
                r#"{"event_id":"sfa1","type":"session.state","session_id":"sfa-s1","timestamp":100}"#,
            ],
        );
        let file_b = temp_jsonl_file(
            "ingest-status-filter-b",
            &[
                r#"{"event_id":"sfb1","type":"session.state","session_id":"sfb-s1","timestamp":100}"#,
            ],
        );

        for (source, file) in [("openclaw", &file_a), ("custom", &file_b)] {
            let ingest = execute(
                &conn,
                &OperationRequest {
                    op: "ingest.events".into(),
                    op_version: Some("v1".into()),
                    request_id: Some(format!("corr-ingest-status-filter-{source}")),
                    idempotency_key: None,
                    actor: ActorContext {
                        agent_id: "main".into(),
                        tenant_id: None,
                        user_id: None,
                        channel: Some("cli".into()),
                    },
                    context: OperationContext::default(),
                    args: serde_json::json!({
                        "source": source,
                        "file_path": file.to_string_lossy().to_string(),
                        "replay": true
                    }),
                },
            )
            .unwrap();
            assert!(ingest.ok);
        }

        let by_source = execute(
            &conn,
            &OperationRequest {
                op: "ingest.status".into(),
                op_version: Some("v1".into()),
                request_id: Some("corr-ingest-status-filter-source".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "source": "openclaw",
                    "limit": 10
                }),
            },
        )
        .unwrap();
        assert!(by_source.ok);
        let source_rows = by_source.data.as_array().cloned().unwrap_or_default();
        assert!(!source_rows.is_empty());
        assert!(source_rows.iter().all(|r| r["source"] == "openclaw"));

        let by_path = execute(
            &conn,
            &OperationRequest {
                op: "ingest.status".into(),
                op_version: Some("v1".into()),
                request_id: Some("corr-ingest-status-filter-path".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "file_path_like": "ingest-status-filter-b",
                    "limit": 10
                }),
            },
        )
        .unwrap();
        assert!(by_path.ok);
        let path_rows = by_path.data.as_array().cloned().unwrap_or_default();
        assert_eq!(path_rows.len(), 1);
        assert_eq!(path_rows[0]["source"], "custom");

        let _ = std::fs::remove_file(file_a);
        let _ = std::fs::remove_file(file_b);
    }

    #[test]
    fn ingest_projects_priority_families() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, created_at)
             VALUES ('allow-all', 'allow all', 10, 'allow', '*', '*', '*', 1)",
            [],
        )
        .unwrap();
        let file = temp_jsonl_file(
            "ingest-priority-families",
            &[
                r#"{"event_id":"u1","type":"model.usage","session_id":"pf-s1","provider":"anthropic","model":"claude","channel":"cli","input_tokens":10,"output_tokens":5,"cost_usd":0.01,"duration_ms":120,"correlation_id":"corr-u1"}"#,
                r#"{"event_id":"r1","type":"run.attempt","session_id":"pf-s1","status":"ok","output":"done","duration_ms":50}"#,
                r#"{"event_id":"w1","type":"webhook.error","session_id":"pf-s1","provider":"stripe","endpoint":"/hook","error":"bad sig"}"#,
            ],
        );

        let resp = execute(
            &conn,
            &OperationRequest {
                op: "ingest.events".into(),
                op_version: Some("v1".into()),
                request_id: Some("corr-ingest-priority".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "source": "openclaw",
                    "file_path": file.to_string_lossy().to_string(),
                    "replay": true
                }),
            },
        )
        .unwrap();
        assert!(resp.ok);
        assert_eq!(resp.data["projected_count"], 3);

        let usage_calls: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM tool_calls WHERE tool_name LIKE 'model.usage:%'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(usage_calls, 1);
        let usage_projection: (String, String, i64, i64, i64, f64, String) = conn
            .query_row(
                "SELECT
                    normalized_provider,
                    normalized_model,
                    normalized_input_tokens,
                    normalized_output_tokens,
                    normalized_total_tokens,
                    normalized_cost_usd,
                    normalized_correlation_id
                 FROM external_events
                 WHERE event_type = 'model.usage'
                 LIMIT 1",
                [],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                        r.get(6)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(usage_projection.0, "anthropic");
        assert_eq!(usage_projection.1, "claude");
        assert_eq!(usage_projection.2, 10);
        assert_eq!(usage_projection.3, 5);
        assert_eq!(usage_projection.4, 15);
        assert!((usage_projection.5 - 0.01).abs() < 1e-9);
        assert_eq!(usage_projection.6, "corr-u1");

        let run_calls: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM tool_calls WHERE tool_name = 'run.attempt'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(run_calls, 1);

        let webhook_audit: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM policy_audit WHERE action = 'webhook.error'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(webhook_audit, 1);

        let _ = std::fs::remove_file(file);
    }

    #[test]
    fn ingest_message_projection_promotes_durable_facts() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, created_at)
             VALUES ('allow-all', 'allow all', 10, 'allow', '*', '*', '*', 1)",
            [],
        )
        .unwrap();
        let file = temp_jsonl_file(
            "ingest-message-facts",
            &[
                r#"{"event_id":"mf1","type":"message.processed","session_id":"mf-s1","role":"assistant","content":"Payments retries use exponential backoff and cap at three attempts before raising an operator alert.","timestamp":1710000000}"#,
            ],
        );

        let resp = execute(
            &conn,
            &OperationRequest {
                op: "ingest.events".into(),
                op_version: Some("v1".into()),
                request_id: Some("corr-ingest-message-facts".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "source": "openclaw",
                    "file_path": file.to_string_lossy().to_string(),
                    "replay": true
                }),
            },
        )
        .unwrap();
        assert!(resp.ok);

        let fact_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM facts WHERE agent_id = 'main'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(fact_count >= 1);
        let source_msg_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM facts WHERE source_message_id = 'ext:openclaw:msg:mf1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(source_msg_count >= 1);

        let _ = std::fs::remove_file(file);
    }

    #[test]
    fn canonical_mutating_ops_return_policy_denied_consistently() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, message, created_at)
             VALUES ('deny-all', 'deny all', 100, 'deny', '*', '*', '*', 'blocked', 1)",
            [],
        )
        .unwrap();

        let ops = vec![
            OperationRequest {
                op: "job.create".into(),
                op_version: Some("v1".into()),
                request_id: Some("deny-job-create".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({ "name": "x", "schedule": "* * * * *" }),
            },
            OperationRequest {
                op: "policy.add".into(),
                op_version: Some("v1".into()),
                request_id: Some("deny-policy-add".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({ "name": "x", "effect": "deny" }),
            },
            OperationRequest {
                op: "knowledge.ingest".into(),
                op_version: Some("v1".into()),
                request_id: Some("deny-knowledge-ingest".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({ "content": "x" }),
            },
            OperationRequest {
                op: "skill.add".into(),
                op_version: Some("v1".into()),
                request_id: Some("deny-skill-add".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({ "name": "x", "content": "x" }),
            },
            OperationRequest {
                op: "fact.delete".into(),
                op_version: Some("v1".into()),
                request_id: Some("deny-fact-delete".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({ "id": "any" }),
            },
            OperationRequest {
                op: "agent.create".into(),
                op_version: Some("v1".into()),
                request_id: Some("deny-agent-create".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "name": "x",
                    "metadata_db_path": "/tmp/ignored.db",
                    "agent_db_path": "/tmp/ignored-agent.db"
                }),
            },
            OperationRequest {
                op: "agent.config".into(),
                op_version: Some("v1".into()),
                request_id: Some("deny-agent-config".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "name": "x",
                    "key": "persona",
                    "value": "x",
                    "metadata_db_path": "/tmp/ignored.db"
                }),
            },
            OperationRequest {
                op: "agent.delete".into(),
                op_version: Some("v1".into()),
                request_id: Some("deny-agent-delete".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "name": "x",
                    "metadata_db_path": "/tmp/ignored.db"
                }),
            },
            OperationRequest {
                op: "ingest.events".into(),
                op_version: Some("v1".into()),
                request_id: Some("deny-ingest-events".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "source": "openclaw",
                    "file_path": "/tmp/ignored.jsonl",
                    "replay": true
                }),
            },
            OperationRequest {
                op: "ingest.replay".into(),
                op_version: Some("v1".into()),
                request_id: Some("deny-ingest-replay".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "run_id": "any"
                }),
            },
        ];

        for req in ops {
            let resp = execute(&conn, &req).unwrap();
            assert!(!resp.ok);
            assert_eq!(resp.code, "policy_denied");
        }
    }

    #[test]
    fn canonical_ops_record_correlation_id_in_policy_audit() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, created_at)
             VALUES ('allow-all', 'allow all', 10, 'allow', '*', '*', '*', 1)",
            [],
        )
        .unwrap();

        let fact_id = crate::store::facts::add(
            &conn,
            &crate::store::facts::NewFact {
                agent_id: "main".into(),
                content: "fact".into(),
                summary: "fact".into(),
                pointer: "fact".into(),
                keywords: None,
                source_message_id: None,
                confidence: 1.0,
            },
            Some("seed"),
        )
        .unwrap();

        let reqs = vec![
            OperationRequest {
                op: "job.create".into(),
                op_version: Some("v1".into()),
                request_id: Some("corr-job-create".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "name": "corr-job",
                    "schedule": "* * * * *",
                    "job_type": "prompt",
                    "payload": "{}"
                }),
            },
            OperationRequest {
                op: "policy.add".into(),
                op_version: Some("v1".into()),
                request_id: Some("corr-policy-add".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "name": "corr-policy",
                    "effect": "audit",
                }),
            },
            OperationRequest {
                op: "knowledge.ingest".into(),
                op_version: Some("v1".into()),
                request_id: Some("corr-knowledge-ingest".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "title": "Correlation Test",
                    "content": "hello"
                }),
            },
            OperationRequest {
                op: "skill.add".into(),
                op_version: Some("v1".into()),
                request_id: Some("corr-skill-add-2".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "name": "corr-skill",
                    "content": "hello"
                }),
            },
            OperationRequest {
                op: "fact.delete".into(),
                op_version: Some("v1".into()),
                request_id: Some("corr-fact-delete".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "id": fact_id,
                    "reason": "test"
                }),
            },
        ];

        let meta_path = temp_db_path("meta-correlation");
        let agent_path = temp_db_path("agent-correlation");
        let meta_path_s = meta_path.to_string_lossy().to_string();
        let agent_path_s = agent_path.to_string_lossy().to_string();
        let create_agent_req = OperationRequest {
            op: "agent.create".into(),
            op_version: Some("v1".into()),
            request_id: Some("corr-agent-create-2".into()),
            idempotency_key: None,
            actor: ActorContext {
                agent_id: "main".into(),
                tenant_id: None,
                user_id: None,
                channel: Some("cli".into()),
            },
            context: OperationContext::default(),
            args: serde_json::json!({
                "name": "corr-agent",
                "metadata_db_path": meta_path_s,
                "agent_db_path": agent_path_s
            }),
        };
        let config_agent_req = OperationRequest {
            op: "agent.config".into(),
            op_version: Some("v1".into()),
            request_id: Some("corr-agent-config-2".into()),
            idempotency_key: None,
            actor: ActorContext {
                agent_id: "main".into(),
                tenant_id: None,
                user_id: None,
                channel: Some("cli".into()),
            },
            context: OperationContext::default(),
            args: serde_json::json!({
                "name": "corr-agent",
                "key": "persona",
                "value": "Hello",
                "metadata_db_path": meta_path.to_string_lossy().to_string()
            }),
        };
        let delete_agent_req = OperationRequest {
            op: "agent.delete".into(),
            op_version: Some("v1".into()),
            request_id: Some("corr-agent-delete-2".into()),
            idempotency_key: None,
            actor: ActorContext {
                agent_id: "main".into(),
                tenant_id: None,
                user_id: None,
                channel: Some("cli".into()),
            },
            context: OperationContext::default(),
            args: serde_json::json!({
                "name": "corr-agent",
                "metadata_db_path": meta_path.to_string_lossy().to_string()
            }),
        };
        let ingest_file = temp_jsonl_file(
            "ingest-correlation",
            &[
                r#"{"event_id":"corr-e1","type":"session.state","session_id":"corr-s1","timestamp":100}"#,
            ],
        );
        let ingest_req = OperationRequest {
            op: "ingest.events".into(),
            op_version: Some("v1".into()),
            request_id: Some("corr-ingest-events-2".into()),
            idempotency_key: None,
            actor: ActorContext {
                agent_id: "main".into(),
                tenant_id: None,
                user_id: None,
                channel: Some("cli".into()),
            },
            context: OperationContext::default(),
            args: serde_json::json!({
                "source": "openclaw",
                "file_path": ingest_file.to_string_lossy().to_string(),
                "replay": true
            }),
        };

        for req in reqs {
            let resp = execute(&conn, &req).unwrap();
            assert!(resp.ok, "operation {} should succeed", req.op);
            let corr = req.request_id.unwrap();
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM policy_audit WHERE correlation_id = ?1",
                    [corr],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(count, 1, "expected one policy audit row for correlation id");
        }

        for req in [
            create_agent_req,
            config_agent_req,
            ingest_req,
            delete_agent_req,
        ] {
            let resp = execute(&conn, &req).unwrap();
            assert!(resp.ok, "operation {} should succeed", req.op);
            let corr = req.request_id.unwrap();
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM policy_audit WHERE correlation_id = ?1",
                    [corr],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(count, 1, "expected one policy audit row for correlation id");
        }

        let _ = std::fs::remove_file(meta_path);
        let _ = std::fs::remove_file(agent_path);
        let _ = std::fs::remove_file(ingest_file);
    }
}
