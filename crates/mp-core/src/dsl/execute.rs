use rusqlite::Connection;
use serde_json::json;
use std::time::Instant;

use super::ast::*;
use crate::operations::{
    ActorContext, AuditMeta, OperationContext, OperationRequest, OperationResponse, PolicyMeta,
};
use crate::policy::{self, Effect, PolicyAuditContext, PolicyDecision, PolicyRequest};

pub struct ExecuteContext {
    pub agent_id: String,
    pub channel: Option<String>,
    pub session_id: Option<String>,
    pub trace_id: Option<String>,
}

pub struct ExecuteResult {
    pub response: OperationResponse,
    pub statement_results: Vec<StatementResult>,
}

pub struct StatementResult {
    pub ok: bool,
    pub code: String,
    pub message: String,
    pub data: serde_json::Value,
    pub raw: String,
    pub policy: Option<PolicyMeta>,
}

pub fn execute_program(
    conn: &Connection,
    program: &Program,
    ctx: &ExecuteContext,
) -> anyhow::Result<ExecuteResult> {
    let start = Instant::now();
    let mut statement_results = Vec::new();
    let mut total_rows = 0usize;

    for stmt in &program.statements {
        let result = execute_statement(conn, stmt, ctx)?;
        if !result.ok {
            return Ok(ExecuteResult {
                response: OperationResponse {
                    ok: false,
                    code: result.code.clone(),
                    message: result.message.clone(),
                    data: json!({
                        "results": statement_results.iter().map(|r: &StatementResult| &r.data).collect::<Vec<_>>(),
                        "failed_statement": result.data,
                        "meta": {
                            "statements": statement_results.len() + 1,
                            "execution_ms": start.elapsed().as_millis() as u64,
                        }
                    }),
                    policy: None,
                    audit: AuditMeta { recorded: true },
                },
                statement_results,
            });
        }
        if let Some(rows) = result.data.get("rows").and_then(|v| v.as_array()) {
            total_rows += rows.len();
        } else if let Some(count) = result.data.get("count").and_then(|v| v.as_u64()) {
            total_rows += count as usize;
        }
        statement_results.push(result);
    }

    let results_data: Vec<_> = statement_results.iter().map(|r| &r.data).collect();
    let elapsed = start.elapsed().as_millis() as u64;

    Ok(ExecuteResult {
        response: OperationResponse {
            ok: true,
            code: "ok".into(),
            message: format!("{} statement(s) executed", statement_results.len()),
            data: json!({
                "results": results_data,
                "meta": {
                    "statements": statement_results.len(),
                    "total_rows": total_rows,
                    "execution_ms": elapsed,
                }
            }),
            policy: None,
            audit: AuditMeta { recorded: true },
        },
        statement_results,
    })
}

fn execute_statement(
    conn: &Connection,
    stmt: &Statement,
    ctx: &ExecuteContext,
) -> anyhow::Result<StatementResult> {
    // Per-statement policy check before execution
    let (action, resource) = head_to_policy_tuple(&stmt.head);
    let decision = check_statement_policy(conn, ctx, action, resource, &stmt.raw)?;

    if matches!(decision.effect, Effect::Deny) {
        return Ok(StatementResult {
            ok: false,
            code: "policy_denied".into(),
            message: decision
                .reason
                .unwrap_or_else(|| "denied by policy".into()),
            data: json!({"policy_id": decision.policy_id, "statement": stmt.raw}),
            raw: stmt.raw.clone(),
            policy: Some(PolicyMeta {
                effect: "deny".into(),
                policy_id: decision.policy_id,
                reason: None,
            }),
        });
    }

    let policy_meta = Some(PolicyMeta {
        effect: match decision.effect {
            Effect::Allow => "allow",
            Effect::Audit => "audit",
            Effect::Deny => "deny",
        }
        .into(),
        policy_id: decision.policy_id,
        reason: decision.reason,
    });

    let mut result = execute_head(conn, &stmt.head, ctx, &stmt.raw)?;
    result.policy = policy_meta;
    if !result.ok {
        return Ok(result);
    }

    for stage in &stmt.pipeline {
        result = apply_pipe_stage(result, stage)?;
    }

    Ok(result)
}

/// Map a parsed Head to the (action, resource) tuple for policy evaluation.
fn head_to_policy_tuple(head: &Head) -> (&'static str, &'static str) {
    match head {
        Head::Search(s) => match s.store {
            Store::Facts => ("search", "memory"),
            Store::Knowledge => ("search", "knowledge"),
            Store::Log => ("search", "log"),
            Store::Audit => ("search", "audit"),
        },
        Head::Insert(ins) => match ins.store {
            Store::Facts => ("add", "fact"),
            _ => ("add", "memory"),
        },
        Head::Update(_) => ("update", "fact"),
        Head::Delete(_) => ("delete", "fact"),
        Head::Ingest(_) => ("ingest", "knowledge"),
        Head::CreatePolicy(_) => ("create", "policy"),
        Head::EvaluatePolicy(_) | Head::ExplainPolicy(_) => ("evaluate", "policy"),
        Head::CreateJob(_) => ("create", "job"),
        Head::RunJob(_) => ("run", "job"),
        Head::PauseJob(_) => ("pause", "job"),
        Head::ResumeJob(_) => ("resume", "job"),
        Head::ListJobs => ("list", "job"),
        Head::HistoryJob(_) => ("history", "job"),
        Head::CreateAgent(_) => ("create", "agent"),
        Head::DeleteAgent(_) => ("delete", "agent"),
        Head::ConfigAgent(_) => ("config", "agent"),
        Head::ResolveSession(_) => ("resolve", "session"),
        Head::ListSessions => ("list", "session"),
        Head::CreateSkill(_) => ("create", "skill"),
        Head::PromoteSkill(_) => ("promote", "skill"),
        Head::CreateTool(_) => ("create", "tool"),
        Head::ListTools => ("list", "tool"),
        Head::DeleteTool(_) => ("delete", "tool"),
        Head::EmbeddingStatus => ("status", "embedding"),
        Head::EmbeddingRetryDead => ("retry", "embedding"),
        Head::EmbeddingBackfill => ("backfill", "embedding"),
    }
}

fn check_statement_policy(
    conn: &Connection,
    ctx: &ExecuteContext,
    action: &str,
    resource: &str,
    raw_statement: &str,
) -> anyhow::Result<PolicyDecision> {
    let audit_ctx = PolicyAuditContext {
        session_id: ctx.session_id.as_deref(),
        correlation_id: ctx.trace_id.as_deref(),
        idempotency_key: None,
        idempotency_state: None,
    };

    policy::evaluate_with_audit(
        conn,
        &PolicyRequest {
            actor: &ctx.agent_id,
            action,
            resource,
            sql_content: Some(raw_statement),
            channel: ctx.channel.as_deref(),
            arguments: None,
        },
        &audit_ctx,
    )
}

fn execute_head(
    conn: &Connection,
    head: &Head,
    ctx: &ExecuteContext,
    raw: &str,
) -> anyhow::Result<StatementResult> {
    match head {
        Head::Search(s) => execute_search(conn, s, ctx),
        Head::Insert(ins) => {
            let op_req = build_op_request(ctx, "memory.fact.add", insert_to_args(ins));
            dispatch_and_wrap(conn, &op_req, raw)
        }
        Head::Update(u) => {
            let op_req = build_op_request(ctx, "memory.fact.update", update_to_args(u));
            dispatch_and_wrap(conn, &op_req, raw)
        }
        Head::Delete(d) => execute_delete(conn, d, ctx, raw),
        Head::Ingest(i) => {
            let mut args = json!({"url": i.url});
            if let Some(ref name) = i.name {
                args["name"] = json!(name);
            }
            let op_req = build_op_request(ctx, "knowledge.ingest", args);
            dispatch_and_wrap(conn, &op_req, raw)
        }
        Head::CreatePolicy(cp) => {
            let args = json!({
                "effect": cp.effect.as_str(),
                "action_pattern": cp.action,
                "resource_pattern": cp.resource,
                "actor_pattern": cp.agent.as_deref().unwrap_or("*"),
                "message": cp.message,
            });
            let op_req = build_op_request(ctx, "policy.add", args);
            dispatch_and_wrap(conn, &op_req, raw)
        }
        Head::EvaluatePolicy(ep) => {
            let args = json!({
                "actor": ep.actor,
                "action": ep.action,
                "resource": ep.resource,
            });
            let op_req = build_op_request(ctx, "policy.evaluate", args);
            dispatch_and_wrap(conn, &op_req, raw)
        }
        Head::ExplainPolicy(ep) => {
            let args = json!({
                "actor": ep.actor,
                "action": ep.action,
                "resource": ep.resource,
            });
            let op_req = build_op_request(ctx, "policy.explain", args);
            dispatch_and_wrap(conn, &op_req, raw)
        }
        Head::CreateJob(j) => {
            let mut args = json!({
                "name": j.name,
                "schedule": j.schedule,
            });
            if let Some(ref t) = j.job_type {
                args["job_type"] = json!(t);
            }
            if let Some(ref p) = j.payload {
                args["payload"] = json!(p);
            }
            let op_req = build_op_request(ctx, "job.create", args);
            dispatch_and_wrap(conn, &op_req, raw)
        }
        Head::RunJob(s) => {
            let op_req = build_op_request(ctx, "job.run", json!({"name": s.value}));
            dispatch_and_wrap(conn, &op_req, raw)
        }
        Head::PauseJob(s) => {
            let op_req = build_op_request(ctx, "job.pause", json!({"name": s.value}));
            dispatch_and_wrap(conn, &op_req, raw)
        }
        Head::ResumeJob(s) => {
            let op_req = build_op_request(ctx, "job.resume", json!({"name": s.value}));
            dispatch_and_wrap(conn, &op_req, raw)
        }
        Head::ListJobs => {
            let op_req = build_op_request(ctx, "job.list", json!({}));
            dispatch_and_wrap(conn, &op_req, raw)
        }
        Head::HistoryJob(s) => {
            let op_req = build_op_request(ctx, "job.history", json!({"name": s.value}));
            dispatch_and_wrap(conn, &op_req, raw)
        }
        Head::CreateAgent(a) => {
            let mut args = json!({"name": a.name});
            for (k, v) in &a.config {
                args[k] = literal_to_json(v);
            }
            let op_req = build_op_request(ctx, "agent.create", args);
            dispatch_and_wrap(conn, &op_req, raw)
        }
        Head::DeleteAgent(s) => {
            let op_req = build_op_request(ctx, "agent.delete", json!({"name": s.value}));
            dispatch_and_wrap(conn, &op_req, raw)
        }
        Head::ConfigAgent(ca) => {
            let mut args = json!({"name": ca.name});
            for (k, v) in &ca.assignments {
                args[k] = literal_to_json(v);
            }
            let op_req = build_op_request(ctx, "agent.config", args);
            dispatch_and_wrap(conn, &op_req, raw)
        }
        Head::ResolveSession(s) => {
            let mut args = json!({});
            if let Some(ref id) = s.value {
                args["id"] = json!(id);
            }
            let op_req = build_op_request(ctx, "session.resolve", args);
            dispatch_and_wrap(conn, &op_req, raw)
        }
        Head::ListSessions => {
            let op_req = build_op_request(ctx, "session.list", json!({}));
            dispatch_and_wrap(conn, &op_req, raw)
        }
        Head::CreateSkill(s) => {
            let op_req = build_op_request(ctx, "skill.add", json!({"content": s.value}));
            dispatch_and_wrap(conn, &op_req, raw)
        }
        Head::PromoteSkill(s) => {
            let op_req = build_op_request(ctx, "skill.promote", json!({"id": s.value}));
            dispatch_and_wrap(conn, &op_req, raw)
        }
        Head::CreateTool(t) => {
            let args = json!({
                "name": t.name,
                "language": t.language,
                "body": t.body,
            });
            let op_req = build_op_request(ctx, "js.tool.add", args);
            dispatch_and_wrap(conn, &op_req, raw)
        }
        Head::ListTools => {
            let op_req = build_op_request(ctx, "js.tool.list", json!({}));
            dispatch_and_wrap(conn, &op_req, raw)
        }
        Head::DeleteTool(s) => {
            let op_req = build_op_request(ctx, "js.tool.delete", json!({"name": s.value}));
            dispatch_and_wrap(conn, &op_req, raw)
        }
        Head::EmbeddingStatus => {
            let op_req = build_op_request(ctx, "embedding.status", json!({}));
            dispatch_and_wrap(conn, &op_req, raw)
        }
        Head::EmbeddingRetryDead => {
            let op_req = build_op_request(ctx, "embedding.retry_dead", json!({}));
            dispatch_and_wrap(conn, &op_req, raw)
        }
        Head::EmbeddingBackfill => {
            let op_req = build_op_request(ctx, "embedding.backfill.enqueue", json!({}));
            dispatch_and_wrap(conn, &op_req, raw)
        }
    }
}

// ── SEARCH: direct path to search functions ──

fn execute_search(
    conn: &Connection,
    search: &SearchHead,
    ctx: &ExecuteContext,
) -> anyhow::Result<StatementResult> {
    let query_text = build_search_query(search);
    let limit = 500; // We'll apply TAKE in pipeline; fetch max here
    let rows = crate::search::search(conn, &query_text, &ctx.agent_id, limit, None, None)?;

    let mut results: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|r| {
            json!({
                "id": r.id,
                "store": format!("{:?}", r.store),
                "content": r.content,
                "score": r.score,
            })
        })
        .collect();

    // Apply conditions as post-filters (temporal, scope, agent, field comparisons)
    results = apply_conditions(results, &search.conditions);

    Ok(StatementResult {
        ok: true,
        code: "ok".into(),
        message: format!("{} results", results.len()),
        data: json!({"rows": results}),
        raw: String::new(),
        policy: None,
    })
}

fn build_search_query(search: &SearchHead) -> String {
    if let Some(ref q) = search.query {
        return q.clone();
    }
    // Build query from field conditions for keyword matching
    let mut parts = Vec::new();
    for cond in &search.conditions {
        if let Condition::Cmp { field, op: CmpOp::Eq, value: Literal::Str(s) } = cond {
            if field != "id" {
                parts.push(s.clone());
            }
        }
        if let Condition::Like { pattern, .. } = cond {
            parts.push(pattern.replace('%', ""));
        }
    }
    if parts.is_empty() {
        "*".to_string()
    } else {
        parts.join(" ")
    }
}

fn apply_conditions(
    mut rows: Vec<serde_json::Value>,
    conditions: &[Condition],
) -> Vec<serde_json::Value> {
    for cond in conditions {
        match cond {
            Condition::Since(dur) => {
                let cutoff = chrono::Utc::now().timestamp() - dur.to_seconds();
                rows.retain(|r| {
                    r.get("created_at")
                        .and_then(|v| v.as_i64())
                        .map(|ts| ts >= cutoff)
                        .unwrap_or(true) // keep if no timestamp field
                });
            }
            Condition::Before(dur) => {
                let cutoff = chrono::Utc::now().timestamp() - dur.to_seconds();
                rows.retain(|r| {
                    r.get("created_at")
                        .and_then(|v| v.as_i64())
                        .map(|ts| ts < cutoff)
                        .unwrap_or(true)
                });
            }
            Condition::Cmp { field, op, value } => {
                rows.retain(|r| matches_cmp(r, field, op, value));
            }
            Condition::Like { field, pattern } => {
                rows.retain(|r| matches_like(r, field, pattern));
            }
            Condition::Scope(_) | Condition::Agent(_) => {
                // Scope and agent filtering are handled by the search
                // function's agent_id parameter; additional scoping deferred.
            }
        }
    }
    rows
}

fn matches_cmp(row: &serde_json::Value, field: &str, op: &CmpOp, value: &Literal) -> bool {
    let Some(field_val) = row.get(field) else {
        return true; // don't filter out rows missing the field
    };
    match value {
        Literal::Str(s) => {
            let Some(fv) = field_val.as_str() else { return true };
            match op {
                CmpOp::Eq => fv == s,
                CmpOp::Ne => fv != s,
                _ => true,
            }
        }
        Literal::Int(n) => {
            let fv = field_val.as_f64().or_else(|| field_val.as_i64().map(|v| v as f64));
            let Some(fv) = fv else { return true };
            let nf = *n as f64;
            match op {
                CmpOp::Eq => (fv - nf).abs() < f64::EPSILON,
                CmpOp::Ne => (fv - nf).abs() >= f64::EPSILON,
                CmpOp::Gt => fv > nf,
                CmpOp::Lt => fv < nf,
                CmpOp::Ge => fv >= nf,
                CmpOp::Le => fv <= nf,
            }
        }
        Literal::Float(n) => {
            let Some(fv) = field_val.as_f64() else { return true };
            match op {
                CmpOp::Eq => (fv - n).abs() < f64::EPSILON,
                CmpOp::Ne => (fv - n).abs() >= f64::EPSILON,
                CmpOp::Gt => fv > *n,
                CmpOp::Lt => fv < *n,
                CmpOp::Ge => fv >= *n,
                CmpOp::Le => fv <= *n,
            }
        }
        Literal::Bool(b) => {
            let Some(fv) = field_val.as_bool() else { return true };
            match op {
                CmpOp::Eq => fv == *b,
                CmpOp::Ne => fv != *b,
                _ => true,
            }
        }
    }
}

fn matches_like(row: &serde_json::Value, field: &str, pattern: &str) -> bool {
    let Some(fv) = row.get(field).and_then(|v| v.as_str()) else {
        return true;
    };
    let lower = fv.to_lowercase();
    let pat = pattern.to_lowercase();

    if pat.starts_with('%') && pat.ends_with('%') {
        lower.contains(&pat[1..pat.len() - 1])
    } else if pat.starts_with('%') {
        lower.ends_with(&pat[1..])
    } else if pat.ends_with('%') {
        lower.starts_with(&pat[..pat.len() - 1])
    } else {
        lower == pat
    }
}

// ── DELETE: iterate matching facts and soft-delete each ──

fn execute_delete(
    conn: &Connection,
    delete: &DeleteHead,
    ctx: &ExecuteContext,
    raw: &str,
) -> anyhow::Result<StatementResult> {
    // If there's an id = "..." condition, delete directly
    for cond in &delete.conditions {
        if let Condition::Cmp {
            field,
            op: CmpOp::Eq,
            value: Literal::Str(id),
        } = cond
        {
            if field == "id" {
                let op_req = build_op_request(ctx, "fact.delete", json!({"id": id}));
                return dispatch_and_wrap(conn, &op_req, raw);
            }
        }
    }

    // Otherwise, search for matching facts and delete each
    let search = SearchHead {
        store: Store::Facts,
        query: None,
        conditions: delete.conditions.clone(),
        mode: SearchMode::Fts,
    };
    let search_result = execute_search(conn, &search, ctx)?;
    let rows = search_result
        .data
        .get("rows")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut deleted = 0;
    for row in &rows {
        if let Some(id) = row.get("id").and_then(|v| v.as_str()) {
            let op_req = build_op_request(
                ctx,
                "fact.delete",
                json!({"id": id, "reason": "deleted via MPQ"}),
            );
            let resp = crate::operations::execute(conn, &op_req)?;
            if resp.ok {
                deleted += 1;
            }
        }
    }

    Ok(StatementResult {
        ok: true,
        code: "ok".into(),
        message: format!("{deleted} fact(s) deleted"),
        data: json!({"deleted": deleted}),
        raw: raw.to_string(),
        policy: None,
    })
}

// ── Pipeline stage application ──

fn apply_pipe_stage(
    mut result: StatementResult,
    stage: &PipeStage,
) -> anyhow::Result<StatementResult> {
    match stage {
        PipeStage::Sort { field, order } => {
            if let Some(rows) = result.data.get_mut("rows").and_then(|v| v.as_array_mut()) {
                rows.sort_by(|a, b| {
                    let va = a.get(field).and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let vb = b.get(field).and_then(|v| v.as_f64()).unwrap_or(0.0);
                    match order {
                        SortOrder::Asc => va.partial_cmp(&vb).unwrap_or(std::cmp::Ordering::Equal),
                        SortOrder::Desc => vb.partial_cmp(&va).unwrap_or(std::cmp::Ordering::Equal),
                    }
                });
            }
            Ok(result)
        }
        PipeStage::Take(n) => {
            if let Some(rows) = result.data.get_mut("rows").and_then(|v| v.as_array_mut()) {
                rows.truncate(*n);
                result.message = format!("{} results", rows.len());
            }
            Ok(result)
        }
        PipeStage::Offset(n) => {
            if let Some(rows) = result.data.get_mut("rows").and_then(|v| v.as_array_mut()) {
                if *n < rows.len() {
                    *rows = rows[*n..].to_vec();
                } else {
                    rows.clear();
                }
                result.message = format!("{} results", rows.len());
            }
            Ok(result)
        }
        PipeStage::Count => {
            let count = result
                .data
                .get("rows")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .or_else(|| result.data.get("count").and_then(|v| v.as_u64()).map(|n| n as usize))
                .unwrap_or(0);
            result.data = json!({"count": count});
            result.message = format!("count: {count}");
            Ok(result)
        }
        PipeStage::Process => {
            // PROCESS is a pass-through that triggers processing in embedding backfill
            // context. For general pipelines, it's a no-op.
            Ok(result)
        }
    }
}

// ── Helpers ──

fn build_op_request(ctx: &ExecuteContext, op: &str, args: serde_json::Value) -> OperationRequest {
    OperationRequest {
        op: op.to_string(),
        op_version: Some("v1".into()),
        request_id: Some(uuid::Uuid::new_v4().to_string()),
        idempotency_key: None,
        actor: ActorContext {
            agent_id: ctx.agent_id.clone(),
            tenant_id: None,
            user_id: None,
            channel: ctx.channel.clone(),
        },
        context: OperationContext {
            session_id: ctx.session_id.clone(),
            trace_id: ctx.trace_id.clone(),
            timestamp: Some(chrono::Utc::now().timestamp()),
        },
        args,
    }
}

fn dispatch_and_wrap(
    conn: &Connection,
    req: &OperationRequest,
    _raw: &str,
) -> anyhow::Result<StatementResult> {
    let resp = crate::operations::execute(conn, req)?;
    Ok(StatementResult {
        ok: resp.ok,
        code: resp.code,
        message: resp.message,
        data: resp.data,
        raw: String::new(),
        policy: None,
    })
}

fn insert_to_args(ins: &InsertHead) -> serde_json::Value {
    let mut args = json!({
        "content": ins.content,
        "summary": ins.content,
        "pointer": ins.content,
    });
    for (k, v) in &ins.fields {
        args[k] = literal_to_json(v);
    }
    args
}

fn update_to_args(u: &UpdateHead) -> serde_json::Value {
    let mut args = json!({});
    // Extract id from conditions
    for cond in &u.conditions {
        if let Condition::Cmp {
            field,
            op: CmpOp::Eq,
            value: Literal::Str(id),
        } = cond
        {
            if field == "id" {
                args["id"] = json!(id);
            }
        }
    }
    for (k, v) in &u.assignments {
        args[k] = literal_to_json(v);
    }
    args
}

fn literal_to_json(lit: &Literal) -> serde_json::Value {
    match lit {
        Literal::Str(s) => json!(s),
        Literal::Int(n) => json!(n),
        Literal::Float(n) => json!(n),
        Literal::Bool(b) => json!(b),
    }
}
