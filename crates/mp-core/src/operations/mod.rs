mod activity;
mod agent_ops;
mod ingest;
mod job;
mod knowledge;
mod memory;
mod policy;
mod session;

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
        "job.create" => job::op_job_create(conn, req),
        "job.list" => job::op_job_list(conn, req),
        "job.run" => job::op_job_run(conn, req),
        "job.pause" => job::op_job_pause(conn, req),
        "job.resume" => job::op_job_resume(conn, req),
        "job.history" => job::op_job_history(conn, req),
        "job.spec.plan" => job::op_job_spec_plan(conn, req),
        "job.spec.confirm" => job::op_job_spec_confirm(conn, req),
        "job.spec.apply" => job::op_job_spec_apply(conn, req),
        "policy.add" => policy::op_policy_add(conn, req),
        "policy.list" => policy::op_policy_list(conn, req),
        "policy.disable" => policy::op_policy_disable(conn, req),
        "policy.spec.plan" => policy::op_policy_spec_plan(conn, req),
        "policy.spec.confirm" => policy::op_policy_spec_confirm(conn, req),
        "policy.spec.apply" => policy::op_policy_spec_apply(conn, req),
        "knowledge.ingest" => knowledge::op_knowledge_ingest(conn, req),
        "knowledge.search" => knowledge::op_knowledge_search(conn, req),
        "knowledge.list" => knowledge::op_knowledge_list(conn, req),
        "memory.search" => memory::op_memory_search(conn, req),
        "memory.fact.add" => memory::op_memory_fact_add(conn, req),
        "memory.fact.update" => memory::op_memory_fact_update(conn, req),
        "memory.fact.get" => memory::op_memory_fact_get(conn, req),
        "memory.fact.compaction.reset" => memory::op_memory_fact_compaction_reset(conn, req),
        "skill.add" => memory::op_skill_add(conn, req),
        "skill.promote" => memory::op_skill_promote(conn, req),
        "fact.delete" => memory::op_fact_delete(conn, req),
        "policy.evaluate" => policy::op_policy_evaluate(conn, req),
        "policy.explain" => policy::op_policy_explain(conn, req),
        "activity.query" => activity::op_activity_query(conn, req),
        "audit.query" => activity::op_audit_query(conn, req),
        "audit.append" => activity::op_audit_append(conn, req),
        "session.resolve" => session::op_session_resolve(conn, req),
        "session.list" => session::op_session_list(conn, req),
        "js.tool.add" => session::op_js_tool_add(conn, req),
        "js.tool.list" => session::op_js_tool_list(conn, req),
        "js.tool.delete" => session::op_js_tool_delete(conn, req),
        "agent.create" => agent_ops::op_agent_create(conn, req),
        "agent.delete" => agent_ops::op_agent_delete(conn, req),
        "agent.config" => agent_ops::op_agent_config(conn, req),
        "ingest.events" => ingest::op_ingest_events(conn, req),
        "ingest.status" => ingest::op_ingest_status(conn, req),
        "ingest.replay" => ingest::op_ingest_replay(conn, req),
        "embedding.status" => ingest::op_embedding_status(conn, req),
        "embedding.retry_dead" => ingest::op_embedding_retry_dead(conn, req),
        "embedding.backfill.enqueue" => ingest::op_embedding_backfill_enqueue(conn, req),
        _ => Ok(fail_response(
            "invalid_args",
            format!("unknown operation '{}'", req.op),
        )),
    }
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
    let reason = decision
        .reason
        .clone()
        .unwrap_or_else(|| "operation denied by policy".into());

    let hint = if decision.policy_id.is_none() {
        "No matching policy rule exists. Add an allow rule with: \
         mp policy add --effect allow --action '<action>' --resource '<resource>'"
    } else {
        "An explicit deny rule matched. Review policies with: mp policy list"
    };

    OperationResponse {
        ok: false,
        code: "policy_denied".into(),
        message: reason,
        data: serde_json::json!({
            "policy_id": decision.policy_id,
            "hint": hint,
        }),
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
            | "policy.list"
            | "policy.disable"
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
            | "activity.query"
            | "audit.query"
            | "audit.append"
            | "session.resolve"
            | "session.list"
            | "js.tool.add"
            | "js.tool.list"
            | "js.tool.delete"
            | "knowledge.ingest"
            | "knowledge.search"
            | "knowledge.list"
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
            | "policy.disable"
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


#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::path::PathBuf;
    use std::thread;

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

    fn start_http_fixture(
        status_line: &str,
        content_type: &str,
        body: &str,
    ) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let status = status_line.to_string();
        let ctype = content_type.to_string();
        let payload = body.to_string();
        let handle = thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0u8; 1024];
                let _ = stream.read(&mut buf);
                let response = format!(
                    "{status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
                    payload.len()
                );
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();
            }
        });
        (format!("http://{addr}/doc"), handle)
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
    fn memory_search_accepts_query_embedding_arg() {
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
                request_id: Some("corr-memory-add-embed".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "content": "Kafka backs event ingestion",
                    "summary": "event ingestion on kafka",
                    "pointer": "kafka-ingestion"
                }),
            },
        )
        .unwrap();
        assert!(add.ok);

        let search = execute(
            &conn,
            &OperationRequest {
                op: "memory.search".into(),
                op_version: Some("v1".into()),
                request_id: Some("corr-memory-search-embed".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "query": "kafka",
                    "limit": 10,
                    "__query_embedding": [1.0, 0.0, 0.0]
                }),
            },
        )
        .unwrap();
        assert!(search.ok);
        assert!(!search.data.as_array().unwrap().is_empty());
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
    fn audit_query_supports_since_until_filters() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, created_at)
             VALUES ('allow-all', 'allow all', 10, 'allow', '*', '*', '*', 1)",
            [],
        )
        .unwrap();

        let mk_append = |created_at: i64, reason: &str| {
            execute(
                &conn,
                &OperationRequest {
                    op: "audit.append".into(),
                    op_version: Some("v1".into()),
                    request_id: Some(format!("corr-audit-append-{created_at}")),
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
                        "reason": reason,
                        "created_at": created_at
                    }),
                },
            )
            .unwrap();
        };

        mk_append(1_700_000_000, "older entry");
        mk_append(1_700_000_100, "newer entry");

        let query = execute(
            &conn,
            &OperationRequest {
                op: "audit.query".into(),
                op_version: Some("v1".into()),
                request_id: Some("corr-audit-query-window".into()),
                idempotency_key: None,
                actor: ActorContext {
                    agent_id: "main".into(),
                    tenant_id: None,
                    user_id: None,
                    channel: Some("cli".into()),
                },
                context: OperationContext::default(),
                args: serde_json::json!({
                    "since": 1_700_000_050i64,
                    "until": 1_700_000_200i64,
                    "limit": 10
                }),
            },
        )
        .unwrap();

        assert!(query.ok);
        let rows = query.data.as_array().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["reason"].as_str(), Some("newer entry"));
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
    fn knowledge_ingest_fetches_http_when_content_missing() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, created_at)
             VALUES ('allow-all', 'allow all', 10, 'allow', '*', '*', '*', 1)",
            [],
        )
        .unwrap();
        let (url, server_thread) = start_http_fixture(
            "HTTP/1.1 200 OK",
            "text/html; charset=utf-8",
            "<html><head><title>Fixture Title</title></head><body><nav>Menu</nav><article><h1>Hello</h1><p>Body text</p></article></body></html>",
        );

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
                "path": url,
            }),
        };

        let resp = execute(&conn, &req).unwrap();
        server_thread.join().unwrap();
        assert!(resp.ok, "knowledge.ingest URL should succeed: {}", resp.message);
        let doc_id = resp.data["document_id"].as_str().unwrap();
        let doc = crate::store::knowledge::get_document(&conn, doc_id)
            .unwrap()
            .unwrap();
        assert_eq!(doc.title.as_deref(), Some("Fixture Title"));
    }

    #[test]
    fn knowledge_ingest_http_error_is_reported() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, created_at)
             VALUES ('allow-all', 'allow all', 10, 'allow', '*', '*', '*', 1)",
            [],
        )
        .unwrap();
        let (url, server_thread) = start_http_fixture(
            "HTTP/1.1 404 Not Found",
            "text/plain",
            "missing",
        );

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
                "path": url,
            }),
        };

        let err = execute(&conn, &req).unwrap_err();
        server_thread.join().unwrap();
        assert!(err.to_string().contains("HTTP 404 Not Found"));
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
                scope: "shared".into(),
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
