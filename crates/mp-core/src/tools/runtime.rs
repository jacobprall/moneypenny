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
        "js_tool_add"       => js_tool_add(conn, arguments),
        "js_tool_list"      => js_tool_list(conn),
        "js_tool_delete"    => js_tool_delete(conn, arguments),
        _ => anyhow::bail!("unknown runtime tool: {tool_name}"),
    }
}

/// Returns true if the given tool name is a built-in runtime tool (handled by
/// this module, NOT a user-defined JS tool stored in the DB).
pub fn is_runtime_tool(name: &str) -> bool {
    matches!(name,
        "memory_search" | "fact_add" | "fact_update" | "fact_list"
        | "scratch_set" | "scratch_get"
        | "knowledge_ingest" | "knowledge_list"
        | "job_create" | "job_list" | "job_pause" | "job_resume"
        | "policy_list" | "audit_query"
        | "js_tool_add" | "js_tool_list" | "js_tool_delete"
    )
}

/// Returns true if `tool_name` refers to a user-defined JS tool stored in
/// the `skills` table (`tool_id` starting with `sqlite_js:`).
pub fn is_js_tool(conn: &Connection, name: &str) -> bool {
    conn.query_row(
        "SELECT tool_id FROM skills WHERE name = ?1",
        [name],
        |r| r.get::<_, Option<String>>(0),
    )
    .ok()
    .flatten()
    .map(|tid| tid.starts_with("sqlite_js:"))
    .unwrap_or(false)
}

/// Execute a user-defined JS tool stored in the `skills` table.
///
/// The script must define a function named `run` that accepts a single JSON
/// object argument and returns a value that will be JSON-serialised.
///
/// Requires `node` (Node.js) or `deno` to be available on `PATH`.
/// If neither is found the tool returns a helpful error.
///
/// # Script contract
/// ```js
/// function run(args) {
///     // args is the parsed JSON arguments object
///     return { result: args.x + args.y };
/// }
/// ```
pub fn dispatch_js(conn: &Connection, tool_name: &str, arguments: &str) -> anyhow::Result<ToolResult> {
    let start = std::time::Instant::now();

    let script: String = conn.query_row(
        "SELECT content FROM skills WHERE name = ?1",
        [tool_name],
        |r| r.get(0),
    ).map_err(|_| anyhow::anyhow!("JS tool '{tool_name}' not found in skills"))?;

    // Build the runner: inject args, call run(), print JSON result
    let runner = format!(
        "const args = {};\n{}\nconsole.log(JSON.stringify(run(args)));",
        arguments, script
    );

    // Try node first, then deno
    let output = run_js_engine(&runner)?;
    let duration_ms = start.elapsed().as_millis() as u64;

    let out = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let success = output.status.success() && !out.is_empty();
    let final_output = if output.status.success() {
        if out.is_empty() { "null".into() } else { out }
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        format!("JS tool error: {}", if stderr.is_empty() { "unknown error".into() } else { stderr.trim().to_string() })
    };

    Ok(ToolResult { output: final_output, success, duration_ms })
}

fn run_js_engine(script: &str) -> anyhow::Result<std::process::Output> {
    // Try node first
    if let Ok(out) = std::process::Command::new("node")
        .arg("-e").arg(script)
        .output()
    {
        return Ok(out);
    }
    // Fall back to deno
    if let Ok(out) = std::process::Command::new("deno")
        .arg("eval").arg(script)
        .output()
    {
        return Ok(out);
    }
    anyhow::bail!(
        "No JavaScript runtime found. Install Node.js (node) or Deno (deno) to use JS tools."
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

    let results = crate::search::search(conn, query, agent_id, limit, None, None)?;

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

// =========================================================================
// JS tool management
// =========================================================================

/// Store a user-defined JavaScript tool in the skills table.
///
/// The agent calls this tool to persist a new callable JS function.  The
/// script must define a `run(args)` function; `args` will be the parsed JSON
/// object passed when the tool is invoked.
///
/// Example:
/// ```json
/// {
///   "name": "add_numbers",
///   "description": "Add two numbers together",
///   "script": "function run(args) { return { result: args.a + args.b }; }"
/// }
/// ```
fn js_tool_add(conn: &Connection, arguments: &str) -> anyhow::Result<ToolResult> {
    let args: serde_json::Value = serde_json::from_str(arguments)?;
    let name = args["name"].as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'name'"))?;
    let description = args["description"].as_str().unwrap_or("User-defined JS tool");
    let script = args["script"].as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'script'"))?;

    // Validate the name is safe
    if name.chars().any(|c| !c.is_alphanumeric() && c != '_' && c != '-') {
        anyhow::bail!("tool name must contain only letters, digits, underscores, or hyphens");
    }

    let tool_id = format!("sqlite_js:{name}");
    let now = chrono::Utc::now().timestamp();
    let id = uuid::Uuid::new_v4().to_string();

    conn.execute(
        "INSERT OR REPLACE INTO skills
         (id, name, description, content, tool_id, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![id, name, description, script, tool_id, now, now],
    )?;

    Ok(ToolResult {
        output: serde_json::json!({"status": "created", "name": name}).to_string(),
        success: true,
        duration_ms: 0,
    })
}

fn js_tool_list(conn: &Connection) -> anyhow::Result<ToolResult> {
    let mut stmt = conn.prepare(
        "SELECT name, description, updated_at FROM skills
         WHERE tool_id LIKE 'sqlite_js:%'
         ORDER BY name"
    )?;
    let tools = stmt.query_map([], |r| {
        Ok(serde_json::json!({
            "name": r.get::<_, String>(0)?,
            "description": r.get::<_, String>(1)?,
            "updated_at": r.get::<_, i64>(2)?,
        }))
    })?.collect::<Result<Vec<_>, _>>()?;

    Ok(ToolResult {
        output: serde_json::to_string_pretty(&tools)?,
        success: true,
        duration_ms: 0,
    })
}

fn js_tool_delete(conn: &Connection, arguments: &str) -> anyhow::Result<ToolResult> {
    let args: serde_json::Value = serde_json::from_str(arguments)?;
    let name = args["name"].as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'name'"))?;

    let rows = conn.execute(
        "DELETE FROM skills WHERE name = ?1 AND tool_id LIKE 'sqlite_js:%'",
        [name],
    )?;

    if rows == 0 {
        Ok(ToolResult {
            output: format!("JS tool '{name}' not found"),
            success: false,
            duration_ms: 0,
        })
    } else {
        Ok(ToolResult {
            output: serde_json::json!({"status": "deleted", "name": name}).to_string(),
            success: true,
            duration_ms: 0,
        })
    }
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
        assert!(is_runtime_tool("js_tool_add"));
        assert!(is_runtime_tool("js_tool_list"));
        assert!(is_runtime_tool("js_tool_delete"));
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

    // ========================================================================
    // JS tool management
    // ========================================================================

    #[test]
    fn js_tool_add_and_list() {
        let conn = setup();
        let result = dispatch(
            &conn, "a", "s1", "js_tool_add",
            r#"{"name":"greet","description":"Say hello","script":"function run(args){return{msg:'hello '+args.name};}"}"#,
        ).unwrap();
        assert!(result.success, "js_tool_add should succeed: {}", result.output);
        assert!(result.output.contains("created"));

        let list = dispatch(&conn, "a", "s1", "js_tool_list", "{}").unwrap();
        assert!(list.success);
        assert!(list.output.contains("greet"));
    }

    #[test]
    fn js_tool_is_detectable() {
        let conn = setup();
        dispatch(
            &conn, "a", "s1", "js_tool_add",
            r#"{"name":"my_js_tool","script":"function run(args){return{}}"}"#,
        ).unwrap();
        assert!(is_js_tool(&conn, "my_js_tool"));
        assert!(!is_js_tool(&conn, "memory_search")); // runtime tool, not JS
    }

    #[test]
    fn js_tool_delete_removes_tool() {
        let conn = setup();
        dispatch(
            &conn, "a", "s1", "js_tool_add",
            r#"{"name":"temp_tool","script":"function run(a){return{}}"}"#,
        ).unwrap();
        assert!(is_js_tool(&conn, "temp_tool"));

        let del = dispatch(&conn, "a", "s1", "js_tool_delete", r#"{"name":"temp_tool"}"#).unwrap();
        assert!(del.success);
        assert!(!is_js_tool(&conn, "temp_tool"));
    }

    #[test]
    fn js_tool_delete_nonexistent_returns_failure() {
        let conn = setup();
        let result = dispatch(&conn, "a", "s1", "js_tool_delete", r#"{"name":"nope"}"#).unwrap();
        assert!(!result.success);
        assert!(result.output.contains("not found"));
    }

    #[test]
    fn js_tool_add_rejects_bad_name() {
        let conn = setup();
        let result = dispatch(
            &conn, "a", "s1", "js_tool_add",
            r#"{"name":"bad name!","script":"function run(a){return{}}"}"#,
        );
        assert!(result.is_err());
    }

    #[test]
    fn js_tool_add_missing_script_fails() {
        let conn = setup();
        let result = dispatch(&conn, "a", "s1", "js_tool_add", r#"{"name":"x"}"#);
        assert!(result.is_err());
    }
}
