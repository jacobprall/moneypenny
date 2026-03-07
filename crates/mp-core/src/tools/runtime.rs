use rusqlite::{Connection, params};
use super::registry::ToolResult;

/// Dispatch a runtime tool call by name.
/// Runtime tools operate on the agent's own database — memory, knowledge,
/// scheduling, policy, and audit — giving the agent self-awareness.
pub fn dispatch(conn: &Connection, agent_id: &str, session_id: &str, tool_name: &str, arguments: &str) -> anyhow::Result<ToolResult> {
    match tool_name {
        "memory_search"     => memory_search(conn, agent_id, arguments),
        "fact_add"          => fact_add(conn, agent_id, arguments),
        "fact_update"       => fact_update(conn, arguments),
        "fact_list"         => fact_list(conn, agent_id),
        "scratch_set"       => scratch_set(conn, session_id, arguments),
        "scratch_get"       => scratch_get(conn, session_id, arguments),
        "knowledge_ingest"  => knowledge_ingest(conn, arguments),
        "knowledge_list"    => knowledge_list(conn),
        "job_create"        => job_create(conn, agent_id, arguments),
        "job_list"          => job_list(conn, agent_id),
        "job_pause"         => job_pause(conn, arguments),
        "job_resume"        => job_resume(conn, arguments),
        "policy_list"       => policy_list(conn),
        "audit_query"       => audit_query(conn, session_id, arguments),
        _ => anyhow::bail!("unknown runtime tool: {tool_name}"),
    }
}

/// Returns true if the given tool name is a runtime tool.
pub fn is_runtime_tool(name: &str) -> bool {
    matches!(name,
        "memory_search" | "fact_add" | "fact_update" | "fact_list"
        | "scratch_set" | "scratch_get"
        | "knowledge_ingest" | "knowledge_list"
        | "job_create" | "job_list" | "job_pause" | "job_resume"
        | "policy_list" | "audit_query"
    )
}

// =========================================================================
// Memory
// =========================================================================

fn memory_search(conn: &Connection, agent_id: &str, arguments: &str) -> anyhow::Result<ToolResult> {
    let args: serde_json::Value = serde_json::from_str(arguments)?;
    let query = args["query"].as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'query'"))?;
    let limit = args["limit"].as_u64().unwrap_or(10) as usize;

    let results = crate::search::search(conn, query, agent_id, limit, None)?;

    let output: Vec<serde_json::Value> = results.iter().map(|r| {
        serde_json::json!({
            "id": r.id,
            "store": format!("{:?}", r.store),
            "content": r.content,
            "score": r.score,
        })
    }).collect();

    Ok(ToolResult {
        output: serde_json::to_string_pretty(&output)?,
        success: true,
        duration_ms: 0,
    })
}

fn fact_add(conn: &Connection, agent_id: &str, arguments: &str) -> anyhow::Result<ToolResult> {
    let args: serde_json::Value = serde_json::from_str(arguments)?;
    let content = args["content"].as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'content'"))?;
    let summary = args["summary"].as_str().unwrap_or(content);
    let pointer = args["pointer"].as_str().unwrap_or(content);
    let keywords = args["keywords"].as_str();
    let confidence = args["confidence"].as_f64().unwrap_or(1.0);

    let fact = crate::store::facts::NewFact {
        agent_id: agent_id.to_string(),
        content: content.to_string(),
        summary: summary.to_string(),
        pointer: pointer.to_string(),
        keywords: keywords.map(|s| s.to_string()),
        source_message_id: None,
        confidence,
    };

    let id = crate::store::facts::add(conn, &fact, Some("added via agent tool"))?;

    Ok(ToolResult {
        output: serde_json::json!({"id": id, "status": "created"}).to_string(),
        success: true,
        duration_ms: 0,
    })
}

fn fact_update(conn: &Connection, arguments: &str) -> anyhow::Result<ToolResult> {
    let args: serde_json::Value = serde_json::from_str(arguments)?;
    let fact_id = args["id"].as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'id'"))?;
    let content = args["content"].as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'content'"))?;
    let summary = args["summary"].as_str().unwrap_or(content);
    let pointer = args["pointer"].as_str().unwrap_or(content);

    crate::store::facts::update(
        conn, fact_id, content, summary, pointer,
        Some("updated via agent tool"), None,
    )?;

    Ok(ToolResult {
        output: serde_json::json!({"id": fact_id, "status": "updated"}).to_string(),
        success: true,
        duration_ms: 0,
    })
}

fn fact_list(conn: &Connection, agent_id: &str) -> anyhow::Result<ToolResult> {
    let facts = crate::store::facts::list_active(conn, agent_id)?;

    let output: Vec<serde_json::Value> = facts.iter().map(|f| {
        serde_json::json!({
            "id": f.id,
            "content": f.content,
            "summary": f.summary,
            "confidence": f.confidence,
            "version": f.version,
        })
    }).collect();

    Ok(ToolResult {
        output: serde_json::to_string_pretty(&output)?,
        success: true,
        duration_ms: 0,
    })
}

// =========================================================================
// Scratch
// =========================================================================

fn scratch_set(conn: &Connection, session_id: &str, arguments: &str) -> anyhow::Result<ToolResult> {
    let args: serde_json::Value = serde_json::from_str(arguments)?;
    let key = args["key"].as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'key'"))?;
    let content = args["content"].as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'content'"))?;

    let id = crate::store::scratch::set(conn, session_id, key, content)?;

    Ok(ToolResult {
        output: serde_json::json!({"id": id, "key": key, "status": "set"}).to_string(),
        success: true,
        duration_ms: 0,
    })
}

fn scratch_get(conn: &Connection, session_id: &str, arguments: &str) -> anyhow::Result<ToolResult> {
    let args: serde_json::Value = serde_json::from_str(arguments)?;
    let key = args["key"].as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'key'"))?;

    match crate::store::scratch::get(conn, session_id, key)? {
        Some(entry) => Ok(ToolResult {
            output: serde_json::json!({
                "key": entry.key,
                "content": entry.content,
            }).to_string(),
            success: true,
            duration_ms: 0,
        }),
        None => Ok(ToolResult {
            output: serde_json::json!({"key": key, "content": null}).to_string(),
            success: true,
            duration_ms: 0,
        }),
    }
}

// =========================================================================
// Knowledge
// =========================================================================

fn knowledge_ingest(conn: &Connection, arguments: &str) -> anyhow::Result<ToolResult> {
    let args: serde_json::Value = serde_json::from_str(arguments)?;
    let content = args["content"].as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'content'"))?;
    let title = args["title"].as_str();
    let path = args["path"].as_str();

    let (doc_id, chunk_count) = crate::store::knowledge::ingest(
        conn, path, title, content, None,
    )?;

    Ok(ToolResult {
        output: serde_json::json!({
            "document_id": doc_id,
            "chunks_created": chunk_count,
            "status": "ingested",
        }).to_string(),
        success: true,
        duration_ms: 0,
    })
}

fn knowledge_list(conn: &Connection) -> anyhow::Result<ToolResult> {
    let docs = crate::store::knowledge::list_documents(conn)?;

    let output: Vec<serde_json::Value> = docs.iter().map(|d| {
        serde_json::json!({
            "id": d.id,
            "title": d.title,
            "path": d.path,
        })
    }).collect();

    Ok(ToolResult {
        output: serde_json::to_string_pretty(&output)?,
        success: true,
        duration_ms: 0,
    })
}

// =========================================================================
// Scheduling
// =========================================================================

fn job_create(conn: &Connection, agent_id: &str, arguments: &str) -> anyhow::Result<ToolResult> {
    let args: serde_json::Value = serde_json::from_str(arguments)?;
    let name = args["name"].as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'name'"))?;
    let schedule = args["schedule"].as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'schedule' (cron expression)"))?;
    let job_type = args["job_type"].as_str().unwrap_or("prompt");
    let payload = args["payload"].as_str().unwrap_or("{}");
    let description = args["description"].as_str();

    let now = chrono::Utc::now().timestamp();

    let job = crate::scheduler::NewJob {
        agent_id: agent_id.to_string(),
        name: name.to_string(),
        description: description.map(|s| s.to_string()),
        schedule: schedule.to_string(),
        next_run_at: now + 60,
        job_type: job_type.to_string(),
        payload: payload.to_string(),
        max_retries: args["max_retries"].as_i64(),
        retry_delay_ms: args["retry_delay_ms"].as_i64(),
        timeout_ms: args["timeout_ms"].as_i64(),
        overlap_policy: args["overlap_policy"].as_str().map(|s| s.to_string()),
    };

    let id = crate::scheduler::create_job(conn, &job)?;

    Ok(ToolResult {
        output: serde_json::json!({"id": id, "name": name, "schedule": schedule, "status": "created"}).to_string(),
        success: true,
        duration_ms: 0,
    })
}

fn job_list(conn: &Connection, agent_id: &str) -> anyhow::Result<ToolResult> {
    let jobs = crate::scheduler::list_jobs(conn, Some(agent_id))?;

    let output: Vec<serde_json::Value> = jobs.iter().map(|j| {
        serde_json::json!({
            "id": j.id,
            "name": j.name,
            "schedule": j.schedule,
            "status": j.status,
            "enabled": j.enabled,
            "job_type": j.job_type,
        })
    }).collect();

    Ok(ToolResult {
        output: serde_json::to_string_pretty(&output)?,
        success: true,
        duration_ms: 0,
    })
}

fn job_pause(conn: &Connection, arguments: &str) -> anyhow::Result<ToolResult> {
    let args: serde_json::Value = serde_json::from_str(arguments)?;
    let job_id = args["id"].as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'id'"))?;

    crate::scheduler::pause_job(conn, job_id)?;

    Ok(ToolResult {
        output: serde_json::json!({"id": job_id, "status": "paused"}).to_string(),
        success: true,
        duration_ms: 0,
    })
}

fn job_resume(conn: &Connection, arguments: &str) -> anyhow::Result<ToolResult> {
    let args: serde_json::Value = serde_json::from_str(arguments)?;
    let job_id = args["id"].as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'id'"))?;

    crate::scheduler::resume_job(conn, job_id)?;

    Ok(ToolResult {
        output: serde_json::json!({"id": job_id, "status": "active"}).to_string(),
        success: true,
        duration_ms: 0,
    })
}

// =========================================================================
// Governance
// =========================================================================

fn policy_list(conn: &Connection) -> anyhow::Result<ToolResult> {
    let mut stmt = conn.prepare(
        "SELECT id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, message, enabled
         FROM policies ORDER BY priority DESC"
    )?;
    let policies = stmt.query_map([], |r| {
        Ok(serde_json::json!({
            "id": r.get::<_, String>(0)?,
            "name": r.get::<_, String>(1)?,
            "priority": r.get::<_, i64>(2)?,
            "effect": r.get::<_, String>(3)?,
            "actor_pattern": r.get::<_, Option<String>>(4)?,
            "action_pattern": r.get::<_, Option<String>>(5)?,
            "resource_pattern": r.get::<_, Option<String>>(6)?,
            "message": r.get::<_, Option<String>>(7)?,
            "enabled": r.get::<_, i64>(8)? != 0,
        }))
    })?.collect::<Result<Vec<_>, _>>()?;

    Ok(ToolResult {
        output: serde_json::to_string_pretty(&policies)?,
        success: true,
        duration_ms: 0,
    })
}

fn audit_query(conn: &Connection, session_id: &str, arguments: &str) -> anyhow::Result<ToolResult> {
    let args: serde_json::Value = serde_json::from_str(arguments)?;
    let limit = args["limit"].as_u64().unwrap_or(20) as usize;
    let scope = args["scope"].as_str().unwrap_or("session");

    let entries = if scope == "session" {
        let mut stmt = conn.prepare(
            "SELECT id, policy_id, actor, action, resource, effect, reason, created_at
             FROM policy_audit WHERE session_id = ?1
             ORDER BY created_at DESC LIMIT ?2"
        )?;
        stmt.query_map(params![session_id, limit], audit_row_to_json)?
            .collect::<Result<Vec<_>, _>>()?
    } else {
        let mut stmt = conn.prepare(
            "SELECT id, policy_id, actor, action, resource, effect, reason, created_at
             FROM policy_audit
             ORDER BY created_at DESC LIMIT ?1"
        )?;
        stmt.query_map(params![limit], audit_row_to_json)?
            .collect::<Result<Vec<_>, _>>()?
    };

    Ok(ToolResult {
        output: serde_json::to_string_pretty(&entries)?,
        success: true,
        duration_ms: 0,
    })
}

fn audit_row_to_json(r: &rusqlite::Row) -> rusqlite::Result<serde_json::Value> {
    Ok(serde_json::json!({
        "id": r.get::<_, String>(0)?,
        "policy_id": r.get::<_, Option<String>>(1)?,
        "actor": r.get::<_, String>(2)?,
        "action": r.get::<_, String>(3)?,
        "resource": r.get::<_, String>(4)?,
        "effect": r.get::<_, String>(5)?,
        "reason": r.get::<_, Option<String>>(6)?,
        "created_at": r.get::<_, i64>(7)?,
    }))
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db, schema};

    fn setup() -> Connection {
        let conn = db::open_memory().unwrap();
        schema::init_agent_db(&conn).unwrap();
        conn
    }

    #[test]
    fn is_runtime_tool_identifies_all() {
        assert!(is_runtime_tool("memory_search"));
        assert!(is_runtime_tool("fact_add"));
        assert!(is_runtime_tool("job_create"));
        assert!(is_runtime_tool("policy_list"));
        assert!(!is_runtime_tool("file_read"));
        assert!(!is_runtime_tool("shell_exec"));
        assert!(!is_runtime_tool("unknown"));
    }

    #[test]
    fn dispatch_unknown_tool_fails() {
        let conn = setup();
        let result = dispatch(&conn, "a", "s", "nonexistent", "{}");
        assert!(result.is_err());
    }

    #[test]
    fn fact_add_and_list() {
        let conn = setup();
        let result = dispatch(
            &conn, "agent-1", "s1", "fact_add",
            r#"{"content": "The sky is blue", "summary": "sky color", "pointer": "sky: blue"}"#,
        ).unwrap();
        assert!(result.success);
        assert!(result.output.contains("created"));

        let list_result = dispatch(&conn, "agent-1", "s1", "fact_list", "{}").unwrap();
        assert!(list_result.success);
        assert!(list_result.output.contains("The sky is blue"));
    }

    #[test]
    fn fact_update_changes_content() {
        let conn = setup();
        let add_result = dispatch(
            &conn, "agent-1", "s1", "fact_add",
            r#"{"content": "original", "summary": "orig", "pointer": "orig"}"#,
        ).unwrap();

        let parsed: serde_json::Value = serde_json::from_str(&add_result.output).unwrap();
        let fact_id = parsed["id"].as_str().unwrap();

        let args = format!(r#"{{"id": "{fact_id}", "content": "updated", "summary": "upd", "pointer": "upd"}}"#);
        let update_result = dispatch(&conn, "agent-1", "s1", "fact_update", &args).unwrap();
        assert!(update_result.success);
        assert!(update_result.output.contains("updated"));
    }

    #[test]
    fn scratch_set_and_get() {
        let conn = setup();
        let set_result = dispatch(
            &conn, "a", "session-1", "scratch_set",
            r#"{"key": "plan", "content": "Step 1: do the thing"}"#,
        ).unwrap();
        assert!(set_result.success);

        let get_result = dispatch(
            &conn, "a", "session-1", "scratch_get",
            r#"{"key": "plan"}"#,
        ).unwrap();
        assert!(get_result.success);
        assert!(get_result.output.contains("Step 1: do the thing"));
    }

    #[test]
    fn scratch_get_missing_key() {
        let conn = setup();
        let result = dispatch(&conn, "a", "s1", "scratch_get", r#"{"key": "nope"}"#).unwrap();
        assert!(result.success);
        assert!(result.output.contains("null"));
    }

    #[test]
    fn knowledge_ingest_and_list() {
        let conn = setup();
        let result = dispatch(
            &conn, "a", "s1", "knowledge_ingest",
            r##"{"title": "Runbook", "content": "# Deploy\nStep 1\n# Rollback\nStep 2"}"##,
        ).unwrap();
        assert!(result.success);
        assert!(result.output.contains("ingested"));

        let list_result = dispatch(&conn, "a", "s1", "knowledge_list", "{}").unwrap();
        assert!(list_result.success);
        assert!(list_result.output.contains("Runbook"));
    }

    #[test]
    fn job_create_and_list() {
        let conn = setup();
        let result = dispatch(
            &conn, "agent-1", "s1", "job_create",
            r#"{"name": "daily-digest", "schedule": "0 9 * * *", "job_type": "prompt", "payload": "{}"}"#,
        ).unwrap();
        assert!(result.success);
        assert!(result.output.contains("daily-digest"));

        let list_result = dispatch(&conn, "agent-1", "s1", "job_list", "{}").unwrap();
        assert!(list_result.success);
        assert!(list_result.output.contains("daily-digest"));
    }

    #[test]
    fn job_pause_and_resume() {
        let conn = setup();
        let create_result = dispatch(
            &conn, "a", "s1", "job_create",
            r#"{"name": "test", "schedule": "* * * * *"}"#,
        ).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&create_result.output).unwrap();
        let job_id = parsed["id"].as_str().unwrap();

        let pause_args = format!(r#"{{"id": "{job_id}"}}"#);
        let pause_result = dispatch(&conn, "a", "s1", "job_pause", &pause_args).unwrap();
        assert!(pause_result.success);
        assert!(pause_result.output.contains("paused"));

        let resume_result = dispatch(&conn, "a", "s1", "job_resume", &pause_args).unwrap();
        assert!(resume_result.success);
        assert!(resume_result.output.contains("active"));
    }

    #[test]
    fn policy_list_returns_entries() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, created_at)
             VALUES ('p1', 'allow-all', 0, 'allow', '*', '*', '*', 1)",
            [],
        ).unwrap();

        let result = dispatch(&conn, "a", "s1", "policy_list", "{}").unwrap();
        assert!(result.success);
        assert!(result.output.contains("allow-all"));
    }

    #[test]
    fn memory_search_returns_results() {
        let conn = setup();

        let fact = crate::store::facts::NewFact {
            agent_id: "agent-1".into(),
            content: "The billing system uses Stripe webhooks".into(),
            summary: "billing uses Stripe".into(),
            pointer: "billing: Stripe webhooks".into(),
            keywords: Some("billing stripe webhooks payments".into()),
            source_message_id: None,
            confidence: 1.0,
        };
        crate::store::facts::add(&conn, &fact, None).unwrap();

        let result = dispatch(
            &conn, "agent-1", "s1", "memory_search",
            r#"{"query": "billing"}"#,
        ).unwrap();
        assert!(result.success);
        assert!(result.output.contains("billing") || result.output.contains("Stripe"));
    }

    #[test]
    fn fact_add_missing_content_fails() {
        let conn = setup();
        let result = dispatch(&conn, "a", "s1", "fact_add", r#"{"summary": "x"}"#);
        assert!(result.is_err());
    }
}
