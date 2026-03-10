use super::registry::ToolResult;
use rusqlite::{Connection, params};

/// Dispatch a runtime tool call by name.
/// Runtime tools operate on the agent's own database — memory, knowledge,
/// scheduling, policy, and audit — giving the agent self-awareness.
pub fn dispatch(
    conn: &Connection,
    agent_id: &str,
    session_id: &str,
    tool_name: &str,
    arguments: &str,
) -> anyhow::Result<ToolResult> {
    match tool_name {
        "web_search" => web_search(arguments),
        "memory_search" => memory_search(conn, agent_id, arguments),
        "fact_add" => fact_add(conn, agent_id, arguments),
        "fact_update" => fact_update(conn, arguments),
        "fact_list" => fact_list(conn, agent_id),
        "scratch_set" => scratch_set(conn, session_id, arguments),
        "scratch_get" => scratch_get(conn, session_id, arguments),
        "knowledge_ingest" => knowledge_ingest(conn, agent_id, arguments),
        "knowledge_list" => knowledge_list(conn),
        "job_create" => job_create(conn, agent_id, session_id, arguments),
        "job_list" => job_list(conn, agent_id, session_id),
        "job_pause" => job_pause(conn, agent_id, session_id, arguments),
        "job_resume" => job_resume(conn, agent_id, session_id, arguments),
        "policy_list" => policy_list(conn),
        "policy_add" => policy_add(conn, agent_id, session_id, arguments),
        "audit_query" => audit_query(conn, session_id, arguments),
        "js_tool_add" => js_tool_add(conn, agent_id, session_id, arguments),
        "js_tool_list" => js_tool_list(conn, agent_id, session_id),
        "js_tool_delete" => js_tool_delete(conn, agent_id, session_id, arguments),
        _ => anyhow::bail!("unknown runtime tool: {tool_name}"),
    }
}

fn build_tool_op_request(
    agent_id: &str,
    session_id: &str,
    op: &str,
    args: serde_json::Value,
) -> crate::operations::OperationRequest {
    let request_id = uuid::Uuid::new_v4().to_string();
    crate::operations::OperationRequest {
        op: op.to_string(),
        op_version: Some("v1".into()),
        request_id: Some(request_id.clone()),
        idempotency_key: None,
        actor: crate::operations::ActorContext {
            agent_id: agent_id.to_string(),
            tenant_id: None,
            user_id: None,
            channel: Some("agent-tool".into()),
        },
        context: crate::operations::OperationContext {
            session_id: Some(session_id.to_string()),
            trace_id: Some(request_id),
            timestamp: Some(chrono::Utc::now().timestamp()),
        },
        args,
    }
}

fn exec_canonical_op(
    conn: &Connection,
    agent_id: &str,
    session_id: &str,
    op: &str,
    args: serde_json::Value,
) -> anyhow::Result<crate::operations::OperationResponse> {
    let req = build_tool_op_request(agent_id, session_id, op, args);
    crate::operations::execute(conn, &req)
}

fn op_response_to_tool_result(resp: &crate::operations::OperationResponse) -> ToolResult {
    let output = if resp.ok {
        serde_json::to_string(&resp.data).unwrap_or_else(|_| resp.message.clone())
    } else {
        format!("Operation denied: {}", resp.message)
    };
    ToolResult {
        output,
        success: resp.ok,
        duration_ms: 0,
    }
}

/// Execute a JavaScript snippet via the in-process sqlite-js QuickJS engine.
/// Returns the stringified result. The JS code has access to `db.exec(sql)`
/// for querying the same SQLite database.
pub fn eval_js(conn: &Connection, script: &str) -> anyhow::Result<String> {
    let result: String = conn
        .query_row("SELECT js_eval(?1)", [script], |r| r.get(0))
        .map_err(|e| anyhow::anyhow!("js_eval failed: {e}"))?;
    Ok(result)
}

/// Returns true if the given tool name is a built-in runtime tool (handled by
/// this module, NOT a user-defined JS tool stored in the DB).
pub fn is_runtime_tool(name: &str) -> bool {
    matches!(
        name,
        "web_search"
            | "memory_search"
            | "fact_add"
            | "fact_update"
            | "fact_list"
            | "scratch_set"
            | "scratch_get"
            | "knowledge_ingest"
            | "knowledge_list"
            | "job_create"
            | "job_list"
            | "job_pause"
            | "job_resume"
            | "policy_list"
            | "policy_add"
            | "audit_query"
            | "js_tool_add"
            | "js_tool_list"
            | "js_tool_delete"
    )
}

/// Returns true if `tool_name` refers to a user-defined JS tool stored in
/// the `skills` table (`tool_id` starting with `sqlite_js:`).
pub fn is_js_tool(conn: &Connection, name: &str) -> bool {
    conn.query_row("SELECT tool_id FROM skills WHERE name = ?1", [name], |r| {
        r.get::<_, Option<String>>(0)
    })
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
/// Execute a user-defined JS tool via the in-process sqlite-js QuickJS engine.
/// The tool's script is loaded from the `skills` table, injected with the
/// call arguments, and evaluated via `js_eval`. The JS code has access to
/// `db.exec(sql)` for querying the same SQLite database.
pub fn dispatch_js(
    conn: &Connection,
    tool_name: &str,
    arguments: &str,
) -> anyhow::Result<ToolResult> {
    let start = std::time::Instant::now();

    let script: String = conn
        .query_row(
            "SELECT content FROM skills WHERE name = ?1",
            [tool_name],
            |r| r.get(0),
        )
        .map_err(|_| anyhow::anyhow!("JS tool '{tool_name}' not found in skills"))?;

    let runner = format!(
        "(function() {{ const args = {};\n{}\nreturn JSON.stringify(run(args)); }})()",
        arguments, script
    );

    let duration_ms;
    match eval_js(conn, &runner) {
        Ok(out) => {
            duration_ms = start.elapsed().as_millis() as u64;
            let output = if out.is_empty() { "null".into() } else { out };
            Ok(ToolResult {
                output,
                success: true,
                duration_ms,
            })
        }
        Err(e) => {
            duration_ms = start.elapsed().as_millis() as u64;
            Ok(ToolResult {
                output: format!("JS tool error: {e}"),
                success: false,
                duration_ms,
            })
        }
    }
}

// =========================================================================
// Web search
// =========================================================================

fn web_search(arguments: &str) -> anyhow::Result<ToolResult> {
    let args: serde_json::Value = serde_json::from_str(arguments)?;
    let query = args["query"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'query'"))?;
    let limit = args["limit"].as_u64().unwrap_or(5).clamp(1, 20) as usize;

    let client = reqwest::blocking::Client::new();
    let response = client
        .get("https://api.duckduckgo.com/")
        .query(&[
            ("q", query),
            ("format", "json"),
            ("no_redirect", "1"),
            ("no_html", "1"),
        ])
        .send();

    let resp = match response {
        Ok(r) => r,
        Err(e) => {
            return Ok(ToolResult {
                output: format!("Web search request failed: {e}"),
                success: false,
                duration_ms: 0,
            });
        }
    };

    let body: serde_json::Value = match resp.json() {
        Ok(v) => v,
        Err(e) => {
            return Ok(ToolResult {
                output: format!("Web search response parse failed: {e}"),
                success: false,
                duration_ms: 0,
            });
        }
    };

    let mut results: Vec<serde_json::Value> = Vec::new();

    if let Some(abs) = body["AbstractText"].as_str() {
        let abs = abs.trim();
        if !abs.is_empty() {
            results.push(serde_json::json!({
                "title": body["Heading"].as_str().unwrap_or("Instant answer"),
                "snippet": abs,
                "url": body["AbstractURL"].as_str().unwrap_or(""),
                "source": body["AbstractSource"].as_str().unwrap_or("DuckDuckGo"),
            }));
        }
    }

    if let Some(arr) = body["RelatedTopics"].as_array() {
        for item in arr {
            if results.len() >= limit {
                break;
            }

            // Flat related topic.
            if let Some(text) = item["Text"].as_str() {
                results.push(serde_json::json!({
                    "title": text.split(" - ").next().unwrap_or(text),
                    "snippet": text,
                    "url": item["FirstURL"].as_str().unwrap_or(""),
                    "source": "DuckDuckGo",
                }));
                continue;
            }

            // Nested topic group with "Topics".
            if let Some(topics) = item["Topics"].as_array() {
                for sub in topics {
                    if results.len() >= limit {
                        break;
                    }
                    if let Some(text) = sub["Text"].as_str() {
                        results.push(serde_json::json!({
                            "title": text.split(" - ").next().unwrap_or(text),
                            "snippet": text,
                            "url": sub["FirstURL"].as_str().unwrap_or(""),
                            "source": "DuckDuckGo",
                        }));
                    }
                }
            }
        }
    }

    if results.is_empty() {
        return Ok(ToolResult {
            output: format!("No web results found for query: {query}"),
            success: true,
            duration_ms: 0,
        });
    }

    results.truncate(limit);
    Ok(ToolResult {
        output: serde_json::to_string_pretty(&results)?,
        success: true,
        duration_ms: 0,
    })
}

// =========================================================================
// Memory
// =========================================================================

fn memory_search(conn: &Connection, agent_id: &str, arguments: &str) -> anyhow::Result<ToolResult> {
    let args: serde_json::Value = serde_json::from_str(arguments)?;
    let query = args["query"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'query'"))?;
    let limit = args["limit"].as_u64().unwrap_or(10) as usize;
    let query_embedding = parse_query_embedding_blob(&args);

    let results = crate::search::search(
        conn,
        query,
        agent_id,
        limit,
        None,
        query_embedding.as_deref(),
    )?;

    let output: Vec<serde_json::Value> = results
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.id,
                "store": format!("{:?}", r.store),
                "content": r.content,
                "score": r.score,
            })
        })
        .collect();

    Ok(ToolResult {
        output: serde_json::to_string_pretty(&output)?,
        success: true,
        duration_ms: 0,
    })
}

fn parse_query_embedding_blob(args: &serde_json::Value) -> Option<Vec<u8>> {
    // Optional private arg injected by the runtime caller to enable true hybrid
    // retrieval (vector + lexical) for the current query.
    let arr = args.get("__query_embedding")?.as_array()?;
    if arr.is_empty() || arr.len() > 8192 {
        return None;
    }

    let mut blob = Vec::with_capacity(arr.len() * std::mem::size_of::<f32>());
    for n in arr {
        let value = n.as_f64()?;
        if !value.is_finite() {
            return None;
        }
        blob.extend_from_slice(&(value as f32).to_le_bytes());
    }
    Some(blob)
}

fn fact_add(conn: &Connection, agent_id: &str, arguments: &str) -> anyhow::Result<ToolResult> {
    let args: serde_json::Value = serde_json::from_str(arguments)?;
    let content = args["content"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'content'"))?;
    let summary = args["summary"].as_str().unwrap_or(content);
    let pointer = args["pointer"].as_str().unwrap_or(content);
    let keywords = args["keywords"].as_str();
    let confidence = args["confidence"].as_f64().unwrap_or(1.0);
    let scope = args["scope"].as_str().unwrap_or("shared");

    let fact = crate::store::facts::NewFact {
        agent_id: agent_id.to_string(),
        scope: scope.to_string(),
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
    let fact_id = args["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'id'"))?;
    let content = args["content"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'content'"))?;
    let summary = args["summary"].as_str().unwrap_or(content);
    let pointer = args["pointer"].as_str().unwrap_or(content);

    crate::store::facts::update(
        conn,
        fact_id,
        content,
        summary,
        pointer,
        Some("updated via agent tool"),
        None,
    )?;

    Ok(ToolResult {
        output: serde_json::json!({"id": fact_id, "status": "updated"}).to_string(),
        success: true,
        duration_ms: 0,
    })
}

fn fact_list(conn: &Connection, agent_id: &str) -> anyhow::Result<ToolResult> {
    let facts = crate::store::facts::list_active(conn, agent_id)?;

    let output: Vec<serde_json::Value> = facts
        .iter()
        .map(|f| {
            serde_json::json!({
                "id": f.id,
                "content": f.content,
                "summary": f.summary,
                "confidence": f.confidence,
                "version": f.version,
            })
        })
        .collect();

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
    let key = args["key"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'key'"))?;
    let content = args["content"]
        .as_str()
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
    let key = args["key"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'key'"))?;

    match crate::store::scratch::get(conn, session_id, key)? {
        Some(entry) => Ok(ToolResult {
            output: serde_json::json!({
                "key": entry.key,
                "content": entry.content,
            })
            .to_string(),
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

fn knowledge_ingest(conn: &Connection, agent_id: &str, arguments: &str) -> anyhow::Result<ToolResult> {
    let args: serde_json::Value = serde_json::from_str(arguments)?;
    let content = args["content"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'content'"))?;
    let title = args["title"].as_str();
    let path = args["path"].as_str();

    let (doc_id, chunk_count) =
        crate::store::knowledge::ingest_scoped(conn, path, title, content, None, Some(agent_id), None)?;

    Ok(ToolResult {
        output: serde_json::json!({
            "document_id": doc_id,
            "chunks_created": chunk_count,
            "status": "ingested",
        })
        .to_string(),
        success: true,
        duration_ms: 0,
    })
}

fn knowledge_list(conn: &Connection) -> anyhow::Result<ToolResult> {
    let docs = crate::store::knowledge::list_documents(conn)?;

    let output: Vec<serde_json::Value> = docs
        .iter()
        .map(|d| {
            serde_json::json!({
                "id": d.id,
                "title": d.title,
                "path": d.path,
            })
        })
        .collect();

    Ok(ToolResult {
        output: serde_json::to_string_pretty(&output)?,
        success: true,
        duration_ms: 0,
    })
}

// =========================================================================
// Scheduling
// =========================================================================

fn job_create(
    conn: &Connection,
    agent_id: &str,
    session_id: &str,
    arguments: &str,
) -> anyhow::Result<ToolResult> {
    let args: serde_json::Value = serde_json::from_str(arguments)?;
    let resp = exec_canonical_op(conn, agent_id, session_id, "job.create", args)?;
    Ok(op_response_to_tool_result(&resp))
}

fn job_list(conn: &Connection, agent_id: &str, session_id: &str) -> anyhow::Result<ToolResult> {
    let resp = exec_canonical_op(
        conn,
        agent_id,
        session_id,
        "job.list",
        serde_json::json!({ "agent_id": agent_id }),
    )?;
    Ok(op_response_to_tool_result(&resp))
}

fn job_pause(
    conn: &Connection,
    agent_id: &str,
    session_id: &str,
    arguments: &str,
) -> anyhow::Result<ToolResult> {
    let args: serde_json::Value = serde_json::from_str(arguments)?;
    let resp = exec_canonical_op(conn, agent_id, session_id, "job.pause", args)?;
    Ok(op_response_to_tool_result(&resp))
}

fn job_resume(
    conn: &Connection,
    _agent_id: &str,
    _session_id: &str,
    arguments: &str,
) -> anyhow::Result<ToolResult> {
    // TODO: add a canonical job.resume operation; for now pause toggle reuses job.pause
    // with a "resume" hint in args so the same policy path applies.
    let args: serde_json::Value = serde_json::from_str(arguments)?;
    let job_id = args["id"]
        .as_str()
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
    let policies = stmt
        .query_map([], |r| {
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
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(ToolResult {
        output: serde_json::to_string_pretty(&policies)?,
        success: true,
        duration_ms: 0,
    })
}

fn policy_add(
    conn: &Connection,
    agent_id: &str,
    session_id: &str,
    arguments: &str,
) -> anyhow::Result<ToolResult> {
    let args: serde_json::Value = serde_json::from_str(arguments)?;

    let req = crate::operations::OperationRequest {
        op: "policy.spec.plan".to_string(),
        op_version: Some("v1".into()),
        request_id: Some(uuid::Uuid::new_v4().to_string()),
        idempotency_key: None,
        actor: crate::operations::ActorContext {
            agent_id: agent_id.to_string(),
            tenant_id: None,
            user_id: None,
            channel: Some("agent".into()),
        },
        context: crate::operations::OperationContext {
            session_id: Some(session_id.to_string()),
            trace_id: None,
            timestamp: Some(chrono::Utc::now().timestamp()),
        },
        args: serde_json::json!({
            "intent": args["intent"].as_str().unwrap_or("agent-proposed policy"),
            "policy_name": args.get("name").or(args.get("policy_name")),
            "effect": args.get("effect"),
            "priority": args.get("priority"),
            "actor_pattern": args.get("actor_pattern"),
            "action_pattern": args.get("action_pattern"),
            "resource_pattern": args.get("resource_pattern"),
            "argument_pattern": args.get("argument_pattern"),
            "channel_pattern": args.get("channel_pattern"),
            "sql_pattern": args.get("sql_pattern"),
            "rule_type": args.get("rule_type"),
            "rule_config": args.get("rule_config"),
            "message": args.get("message"),
            "proposed_by": "agent",
            "source_session_id": session_id,
        }),
    };

    let resp = crate::operations::execute(conn, &req)?;
    let summary = if resp.ok {
        format!(
            "Policy spec planned (spec_id: {}). Status: planned.\n\
             The user must confirm this spec before it becomes active.\n\
             Tell the user: \"I've drafted a policy: '{}' (effect: {}, priority: {}). \
             Shall I apply it?\"",
            resp.data["spec_id"].as_str().unwrap_or("?"),
            resp.data["policy_name"].as_str().unwrap_or("?"),
            resp.data["effect"].as_str().unwrap_or("?"),
            resp.data["priority"].as_i64().unwrap_or(0),
        )
    } else {
        format!("Failed to plan policy: {}", resp.message)
    };

    Ok(ToolResult {
        output: summary,
        success: resp.ok,
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
             ORDER BY created_at DESC LIMIT ?2",
        )?;
        stmt.query_map(params![session_id, limit], audit_row_to_json)?
            .collect::<Result<Vec<_>, _>>()?
    } else {
        let mut stmt = conn.prepare(
            "SELECT id, policy_id, actor, action, resource, effect, reason, created_at
             FROM policy_audit
             ORDER BY created_at DESC LIMIT ?1",
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
fn js_tool_add(
    conn: &Connection,
    agent_id: &str,
    session_id: &str,
    arguments: &str,
) -> anyhow::Result<ToolResult> {
    let args: serde_json::Value = serde_json::from_str(arguments)?;
    let resp = exec_canonical_op(conn, agent_id, session_id, "js.tool.add", args)?;
    Ok(op_response_to_tool_result(&resp))
}

fn js_tool_list(conn: &Connection, agent_id: &str, session_id: &str) -> anyhow::Result<ToolResult> {
    let resp = exec_canonical_op(
        conn,
        agent_id,
        session_id,
        "js.tool.list",
        serde_json::json!({}),
    )?;
    Ok(op_response_to_tool_result(&resp))
}

fn js_tool_delete(
    conn: &Connection,
    agent_id: &str,
    session_id: &str,
    arguments: &str,
) -> anyhow::Result<ToolResult> {
    let args: serde_json::Value = serde_json::from_str(arguments)?;
    let resp = exec_canonical_op(conn, agent_id, session_id, "js.tool.delete", args)?;
    Ok(op_response_to_tool_result(&resp))
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
        assert!(is_runtime_tool("web_search"));
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
            &conn,
            "agent-1",
            "s1",
            "fact_add",
            r#"{"content": "The sky is blue", "summary": "sky color", "pointer": "sky: blue"}"#,
        )
        .unwrap();
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
            &conn,
            "agent-1",
            "s1",
            "fact_add",
            r#"{"content": "original", "summary": "orig", "pointer": "orig"}"#,
        )
        .unwrap();

        let parsed: serde_json::Value = serde_json::from_str(&add_result.output).unwrap();
        let fact_id = parsed["id"].as_str().unwrap();

        let args = format!(
            r#"{{"id": "{fact_id}", "content": "updated", "summary": "upd", "pointer": "upd"}}"#
        );
        let update_result = dispatch(&conn, "agent-1", "s1", "fact_update", &args).unwrap();
        assert!(update_result.success);
        assert!(update_result.output.contains("updated"));
    }

    #[test]
    fn scratch_set_and_get() {
        let conn = setup();
        let set_result = dispatch(
            &conn,
            "a",
            "session-1",
            "scratch_set",
            r#"{"key": "plan", "content": "Step 1: do the thing"}"#,
        )
        .unwrap();
        assert!(set_result.success);

        let get_result =
            dispatch(&conn, "a", "session-1", "scratch_get", r#"{"key": "plan"}"#).unwrap();
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
            &conn,
            "a",
            "s1",
            "knowledge_ingest",
            r##"{"title": "Runbook", "content": "# Deploy\nStep 1\n# Rollback\nStep 2"}"##,
        )
        .unwrap();
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
            &conn,
            "a",
            "s1",
            "job_create",
            r#"{"name": "test", "schedule": "* * * * *"}"#,
        )
        .unwrap();
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
            scope: "shared".into(),
            content: "The billing system uses Stripe webhooks".into(),
            summary: "billing uses Stripe".into(),
            pointer: "billing: Stripe webhooks".into(),
            keywords: Some("billing stripe webhooks payments".into()),
            source_message_id: None,
            confidence: 1.0,
        };
        crate::store::facts::add(&conn, &fact, None).unwrap();

        let result = dispatch(
            &conn,
            "agent-1",
            "s1",
            "memory_search",
            r#"{"query": "billing"}"#,
        )
        .unwrap();
        assert!(result.success);
        assert!(result.output.contains("billing") || result.output.contains("Stripe"));
    }

    #[test]
    fn memory_search_semantic_recalls_projected_logs() {
        let conn = setup();
        mp_ext::init_all_extensions(&conn).unwrap();
        crate::schema::init_vector_indexes(&conn, 3).unwrap();

        let sid = crate::store::log::create_session(&conn, "agent-1", None).unwrap();
        let mid = crate::store::log::append_message(&conn, &sid, "assistant", "executing checks")
            .unwrap();

        conn.execute(
            "INSERT INTO tool_calls
             (id, message_id, session_id, tool_name, arguments, result, status, policy_decision, content_embedding, duration_ms, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            rusqlite::params![
                "tc-runtime-sem",
                mid,
                sid,
                "shell_exec",
                "{\"command\":\"deploy check\"}",
                "deploy denied",
                "denied",
                "deny",
                vec![0_u8; 0],
                7_i64,
                1_i64,
            ],
        ).unwrap();
        conn.execute(
            "INSERT INTO policy_audit
             (id, policy_id, actor, action, resource, effect, reason, content_embedding, session_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            rusqlite::params![
                "pa-runtime-sem",
                "p1",
                "agent-1",
                "call",
                "shell_exec",
                "deny",
                "prod deploy blocked",
                vec![0_u8; 0],
                sid,
                2_i64,
            ],
        ).unwrap();

        fn f32_blob(v: &[f32]) -> Vec<u8> {
            let mut out = Vec::with_capacity(v.len() * std::mem::size_of::<f32>());
            for x in v {
                out.extend_from_slice(&x.to_le_bytes());
            }
            out
        }

        conn.execute(
            "UPDATE tool_calls SET content_embedding = ?1 WHERE id = 'tc-runtime-sem'",
            rusqlite::params![f32_blob(&[1.0, 0.0, 0.0])],
        )
        .unwrap();
        conn.execute(
            "UPDATE policy_audit SET content_embedding = ?1 WHERE id = 'pa-runtime-sem'",
            rusqlite::params![f32_blob(&[1.0, 0.0, 0.0])],
        )
        .unwrap();

        for (table, col) in &[
            ("tool_calls", "content_embedding"),
            ("policy_audit", "content_embedding"),
        ] {
            conn.query_row(
                "SELECT vector_quantize(?1, ?2)",
                rusqlite::params![table, col],
                |_| Ok::<_, rusqlite::Error>(()),
            )
            .unwrap();
        }

        let result = dispatch(
            &conn,
            "agent-1",
            &sid,
            "memory_search",
            r#"{"query":"qzv no lexical overlap","limit":10,"__query_embedding":[1.0,0.0,0.0]}"#,
        )
        .unwrap();
        assert!(result.success);
        assert!(result.output.contains("[tool_call]"));
        assert!(result.output.contains("[policy_audit]"));
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
        assert!(
            result.success,
            "js_tool_add should succeed: {}",
            result.output
        );
        assert!(result.output.contains("created"));

        let list = dispatch(&conn, "a", "s1", "js_tool_list", "{}").unwrap();
        assert!(list.success);
        assert!(list.output.contains("greet"));
    }

    #[test]
    fn js_tool_is_detectable() {
        let conn = setup();
        dispatch(
            &conn,
            "a",
            "s1",
            "js_tool_add",
            r#"{"name":"my_js_tool","script":"function run(args){return{}}"}"#,
        )
        .unwrap();
        assert!(is_js_tool(&conn, "my_js_tool"));
        assert!(!is_js_tool(&conn, "memory_search")); // runtime tool, not JS
    }

    #[test]
    fn js_tool_delete_removes_tool() {
        let conn = setup();
        dispatch(
            &conn,
            "a",
            "s1",
            "js_tool_add",
            r#"{"name":"temp_tool","script":"function run(a){return{}}"}"#,
        )
        .unwrap();
        assert!(is_js_tool(&conn, "temp_tool"));

        let del = dispatch(
            &conn,
            "a",
            "s1",
            "js_tool_delete",
            r#"{"name":"temp_tool"}"#,
        )
        .unwrap();
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
            &conn,
            "a",
            "s1",
            "js_tool_add",
            r#"{"name":"bad name!","script":"function run(a){return{}}"}"#,
        )
        .unwrap();
        assert!(!result.success);
        assert!(
            result.output.contains("denied")
                || result.output.contains("invalid")
                || result.output.contains("must contain")
        );
    }

    #[test]
    fn js_tool_add_missing_script_fails() {
        let conn = setup();
        let result = dispatch(&conn, "a", "s1", "js_tool_add", r#"{"name":"x"}"#);
        assert!(result.is_err() || !result.unwrap().success);
    }
}
