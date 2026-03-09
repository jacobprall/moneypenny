use anyhow::Result;
use serde_json::{Value, json};

pub const TOOL_QUERY: &str = "moneypenny.query";
pub const TOOL_MEMORY: &str = "moneypenny.memory";
pub const TOOL_KNOWLEDGE: &str = "moneypenny.knowledge";
pub const TOOL_POLICY: &str = "moneypenny.policy";
pub const TOOL_JOBS: &str = "moneypenny.jobs";
pub const TOOL_AUDIT: &str = "moneypenny.audit";
pub const TOOL_INGEST: &str = "moneypenny.ingest";
pub const TOOL_EMBEDDING: &str = "moneypenny.embedding";
pub const TOOL_SESSION: &str = "moneypenny.session";
pub const TOOL_AGENT: &str = "moneypenny.agent";
pub const TOOL_TOOLS: &str = "moneypenny.tools";
pub const TOOL_CAPABILITIES: &str = "moneypenny.capabilities";
pub const TOOL_EXECUTE: &str = "moneypenny.execute";

#[derive(Debug, Clone)]
pub enum RoutedToolCall {
    Capabilities {
        payload: Value,
    },
    MpqQuery {
        expression: String,
        dry_run: bool,
    },
    Operation {
        domain_tool: String,
        action: String,
        op: String,
        args: Value,
        execute_fallback: bool,
    },
}

pub fn tools_list() -> Value {
    let tools = vec![
        mp_core::dsl::tool_definition(),
        domain_tool(
            TOOL_MEMORY,
            "Moneypenny: memory skill pack. Use when the user asks to remember, retrieve, update, or forget durable facts.",
            &[
                "search",
                "add",
                "update",
                "get",
                "delete",
                "reset_compaction",
            ],
        ),
        domain_tool(
            TOOL_KNOWLEDGE,
            "Moneypenny: knowledge skill pack. Use when the user asks to ingest or search documents and runbooks.",
            &["ingest", "search", "list"],
        ),
        domain_tool(
            TOOL_POLICY,
            "Moneypenny: policy skill pack. Use when the user asks to add, evaluate, explain, or lifecycle policy specs.",
            &[
                "add",
                "evaluate",
                "explain",
                "spec_plan",
                "spec_confirm",
                "spec_apply",
            ],
        ),
        domain_tool(
            TOOL_JOBS,
            "Moneypenny: jobs skill pack. Use when the user asks to create, run, pause, resume, inspect history, or manage job specs.",
            &[
                "create",
                "list",
                "run",
                "pause",
                "resume",
                "history",
                "spec_plan",
                "spec_confirm",
                "spec_apply",
            ],
        ),
        domain_tool(
            TOOL_AUDIT,
            "Moneypenny: audit skill pack. Use when the user asks to inspect or append governance/audit trail records.",
            &["query", "append"],
        ),
        domain_tool(
            TOOL_INGEST,
            "Moneypenny: ingest skill pack. Use when the user asks to ingest external events, inspect ingest runs, or replay runs.",
            &["events", "status", "replay"],
        ),
        domain_tool(
            TOOL_EMBEDDING,
            "Moneypenny: embedding recovery skill pack. Use when the user asks to inspect queue health, revive dead jobs, backfill, or process now.",
            &[
                "status",
                "retry_dead",
                "backfill_enqueue",
                "process",
                "backfill_process",
            ],
        ),
        domain_tool(
            TOOL_SESSION,
            "Moneypenny: session skill pack. Use when the user asks to resolve/create sessions or list recent sessions.",
            &["resolve", "list"],
        ),
        domain_tool(
            TOOL_AGENT,
            "Moneypenny: agent admin skill pack. Use when the user asks to create, delete, or configure agents.",
            &["create", "delete", "config"],
        ),
        domain_tool(
            TOOL_TOOLS,
            "Moneypenny: tools/skills pack. Use when the user asks to manage skills or JavaScript tools.",
            &[
                "skill_add",
                "skill_promote",
                "js_add",
                "js_list",
                "js_delete",
            ],
        ),
        json!({
            "name": TOOL_CAPABILITIES,
            "description": "Moneypenny: capability guide. Use when you need discoverability hints, domain summaries, and recommended next actions.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "domain": { "type": "string", "description": "Optional domain filter (memory|knowledge|policy|jobs|audit|ingest|embedding|session|agent|tools)" }
                },
                "additionalProperties": false
            }
        }),
        json!({
            "name": TOOL_EXECUTE,
            "description": "Moneypenny: advanced fallback. Use only for unsupported advanced operations not covered by domain tools.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "op": { "type": "string", "description": "Canonical operation name (advanced)" },
                    "args": { "type": "object", "default": {} },
                    "request_id": { "type": "string" },
                    "idempotency_key": { "type": "string" },
                    "agent_id": { "type": "string" },
                    "tenant_id": { "type": "string" },
                    "user_id": { "type": "string" },
                    "channel": { "type": "string" },
                    "session_id": { "type": "string" },
                    "trace_id": { "type": "string" }
                },
                "required": ["op"],
                "additionalProperties": true
            }
        }),
    ];
    json!({ "tools": tools })
}

pub fn capabilities(domain_filter: Option<&str>) -> Value {
    let cards = vec![
        card(
            "memory",
            TOOL_MEMORY,
            &[
                "search",
                "add",
                "update",
                "get",
                "delete",
                "reset_compaction",
            ],
            "Store and retrieve durable facts.",
        ),
        card(
            "knowledge",
            TOOL_KNOWLEDGE,
            &["ingest", "search", "list"],
            "Ingest and query documents/chunks.",
        ),
        card(
            "policy",
            TOOL_POLICY,
            &[
                "add",
                "evaluate",
                "explain",
                "spec_plan",
                "spec_confirm",
                "spec_apply",
            ],
            "Governance policy authoring and evaluation.",
        ),
        card(
            "jobs",
            TOOL_JOBS,
            &[
                "create",
                "list",
                "run",
                "pause",
                "resume",
                "history",
                "spec_plan",
                "spec_confirm",
                "spec_apply",
            ],
            "Scheduled automation and job specs.",
        ),
        card(
            "audit",
            TOOL_AUDIT,
            &["query", "append"],
            "Queryable governance trail.",
        ),
        card(
            "ingest",
            TOOL_INGEST,
            &["events", "status", "replay"],
            "External event ingestion lifecycle.",
        ),
        card(
            "embedding",
            TOOL_EMBEDDING,
            &[
                "status",
                "retry_dead",
                "backfill_enqueue",
                "process",
                "backfill_process",
            ],
            "Embedding queue recovery and backfill.",
        ),
        card(
            "session",
            TOOL_SESSION,
            &["resolve", "list"],
            "Session lookup and routing.",
        ),
        card(
            "agent",
            TOOL_AGENT,
            &["create", "delete", "config"],
            "Agent administration.",
        ),
        card(
            "tools",
            TOOL_TOOLS,
            &[
                "skill_add",
                "skill_promote",
                "js_add",
                "js_list",
                "js_delete",
            ],
            "Skill and JS tool management.",
        ),
    ];

    let filtered = if let Some(domain) = domain_filter {
        cards
            .into_iter()
            .filter(|c| c["domain"].as_str() == Some(domain))
            .collect::<Vec<_>>()
    } else {
        cards
    };

    json!({
        "domains": filtered,
        "fallback": {
            "tool": TOOL_EXECUTE,
            "guidance": "Use only for unsupported advanced operations."
        }
    })
}

pub fn route_tool_call(tool_name: &str, arguments: &Value) -> Result<RoutedToolCall> {
    let normalized = normalize_tool_name(tool_name);
    if normalized == "query" {
        let expression = arguments
            .get("expression")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow::anyhow!("moneypenny.query requires 'expression'"))?;
        let dry_run = arguments
            .get("dry_run")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        return Ok(RoutedToolCall::MpqQuery {
            expression: expression.to_string(),
            dry_run,
        });
    }

    if normalized == "capabilities" {
        let domain = arguments.get("domain").and_then(Value::as_str);
        return Ok(RoutedToolCall::Capabilities {
            payload: capabilities(domain),
        });
    }

    if normalized == "execute" {
        let op = arguments
            .get("op")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow::anyhow!("moneypenny.execute requires 'op'"))?;
        let args = arguments.get("args").cloned().unwrap_or_else(|| json!({}));
        if !args.is_object() {
            anyhow::bail!("moneypenny.execute requires object 'args'");
        }
        return Ok(RoutedToolCall::Operation {
            domain_tool: TOOL_EXECUTE.to_string(),
            action: "execute".to_string(),
            op: op.to_string(),
            args,
            execute_fallback: true,
        });
    }

    let input = arguments.get("input").cloned().unwrap_or_else(|| json!({}));
    if !input.is_object() {
        anyhow::bail!("tool input must be an object");
    }
    let action = arguments
        .get("action")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("tool call requires string field 'action'"))?;

    let (op, args, domain_name) = match normalized.as_str() {
        "memory" => route_memory(action, input)?,
        "knowledge" => route_knowledge(action, input)?,
        "policy" => route_policy(action, input)?,
        "jobs" => route_jobs(action, input)?,
        "audit" => route_audit(action, input)?,
        "ingest" => route_ingest(action, input)?,
        "embedding" => route_embedding(action, input)?,
        "session" => route_session(action, input)?,
        "agent" => route_agent(action, input)?,
        "tools" => route_tools(action, input)?,
        _ => anyhow::bail!("unknown moneypenny tool '{tool_name}'"),
    };

    Ok(RoutedToolCall::Operation {
        domain_tool: domain_name.to_string(),
        action: action.to_string(),
        op,
        args,
        execute_fallback: false,
    })
}

pub fn covered_ops() -> &'static [&'static str] {
    &[
        "memory.search",
        "memory.fact.add",
        "memory.fact.update",
        "memory.fact.get",
        "memory.fact.compaction.reset",
        "fact.delete",
        "knowledge.ingest",
        "knowledge.search",
        "knowledge.list",
        "policy.add",
        "policy.evaluate",
        "policy.explain",
        "policy.spec.plan",
        "policy.spec.confirm",
        "policy.spec.apply",
        "job.create",
        "job.list",
        "job.run",
        "job.pause",
        "job.resume",
        "job.history",
        "job.spec.plan",
        "job.spec.confirm",
        "job.spec.apply",
        "audit.query",
        "audit.append",
        "ingest.events",
        "ingest.status",
        "ingest.replay",
        "embedding.status",
        "embedding.retry_dead",
        "embedding.backfill.enqueue",
        "embedding.process",
        "embedding.backfill.process",
        "session.resolve",
        "session.list",
        "agent.create",
        "agent.delete",
        "agent.config",
        "skill.add",
        "skill.promote",
        "js.tool.add",
        "js.tool.list",
        "js.tool.delete",
    ]
}

pub fn next_actions(domain_tool: &str, action: &str) -> Vec<Value> {
    match (normalize_tool_name(domain_tool).as_str(), action) {
        ("embedding", "status") => vec![
            json!({"tool": TOOL_EMBEDDING, "action": "retry_dead"}),
            json!({"tool": TOOL_EMBEDDING, "action": "process"}),
        ],
        ("embedding", "retry_dead") => vec![
            json!({"tool": TOOL_EMBEDDING, "action": "process"}),
            json!({"tool": TOOL_EMBEDDING, "action": "status"}),
        ],
        ("knowledge", "ingest") => vec![
            json!({"tool": TOOL_KNOWLEDGE, "action": "search"}),
            json!({"tool": TOOL_EMBEDDING, "action": "status"}),
        ],
        ("jobs", "create") => vec![
            json!({"tool": TOOL_JOBS, "action": "list"}),
            json!({"tool": TOOL_JOBS, "action": "run"}),
        ],
        ("policy", "add") => vec![
            json!({"tool": TOOL_POLICY, "action": "evaluate"}),
            json!({"tool": TOOL_AUDIT, "action": "query"}),
        ],
        _ => vec![],
    }
}

fn route_memory(action: &str, input: Value) -> Result<(String, Value, &'static str)> {
    let op = match action {
        "search" => "memory.search",
        "add" => "memory.fact.add",
        "update" => "memory.fact.update",
        "get" => "memory.fact.get",
        "delete" | "forget" => "fact.delete",
        "reset_compaction" => "memory.fact.compaction.reset",
        _ => invalid_action(
            "memory",
            action,
            &[
                "search",
                "add",
                "update",
                "get",
                "delete",
                "reset_compaction",
            ],
        )?,
    };
    Ok((op.to_string(), input, TOOL_MEMORY))
}

fn route_knowledge(action: &str, input: Value) -> Result<(String, Value, &'static str)> {
    let op = match action {
        "ingest" => "knowledge.ingest",
        "search" => "knowledge.search",
        "list" => "knowledge.list",
        _ => invalid_action("knowledge", action, &["ingest", "search", "list"])?,
    };
    Ok((op.to_string(), input, TOOL_KNOWLEDGE))
}

fn route_policy(action: &str, input: Value) -> Result<(String, Value, &'static str)> {
    let op = match action {
        "add" => "policy.add",
        "evaluate" => "policy.evaluate",
        "explain" => "policy.explain",
        "spec_plan" => "policy.spec.plan",
        "spec_confirm" => "policy.spec.confirm",
        "spec_apply" => "policy.spec.apply",
        _ => invalid_action(
            "policy",
            action,
            &[
                "add",
                "evaluate",
                "explain",
                "spec_plan",
                "spec_confirm",
                "spec_apply",
            ],
        )?,
    };
    Ok((op.to_string(), input, TOOL_POLICY))
}

fn route_jobs(action: &str, input: Value) -> Result<(String, Value, &'static str)> {
    let op = match action {
        "create" => "job.create",
        "list" => "job.list",
        "run" => "job.run",
        "pause" => "job.pause",
        "resume" => "job.resume",
        "history" => "job.history",
        "spec_plan" => "job.spec.plan",
        "spec_confirm" => "job.spec.confirm",
        "spec_apply" => "job.spec.apply",
        _ => invalid_action(
            "jobs",
            action,
            &[
                "create",
                "list",
                "run",
                "pause",
                "resume",
                "history",
                "spec_plan",
                "spec_confirm",
                "spec_apply",
            ],
        )?,
    };
    Ok((op.to_string(), input, TOOL_JOBS))
}

fn route_audit(action: &str, input: Value) -> Result<(String, Value, &'static str)> {
    let op = match action {
        "query" => "audit.query",
        "append" => "audit.append",
        _ => invalid_action("audit", action, &["query", "append"])?,
    };
    Ok((op.to_string(), input, TOOL_AUDIT))
}

fn route_ingest(action: &str, input: Value) -> Result<(String, Value, &'static str)> {
    let op = match action {
        "events" => "ingest.events",
        "status" => "ingest.status",
        "replay" => "ingest.replay",
        _ => invalid_action("ingest", action, &["events", "status", "replay"])?,
    };
    Ok((op.to_string(), input, TOOL_INGEST))
}

fn route_embedding(action: &str, input: Value) -> Result<(String, Value, &'static str)> {
    let op = match action {
        "status" => "embedding.status",
        "retry_dead" => "embedding.retry_dead",
        "backfill_enqueue" => "embedding.backfill.enqueue",
        "process" => "embedding.process",
        "backfill_process" => "embedding.backfill.process",
        _ => invalid_action(
            "embedding",
            action,
            &[
                "status",
                "retry_dead",
                "backfill_enqueue",
                "process",
                "backfill_process",
            ],
        )?,
    };
    Ok((op.to_string(), input, TOOL_EMBEDDING))
}

fn route_session(action: &str, input: Value) -> Result<(String, Value, &'static str)> {
    let op = match action {
        "resolve" => "session.resolve",
        "list" => "session.list",
        _ => invalid_action("session", action, &["resolve", "list"])?,
    };
    Ok((op.to_string(), input, TOOL_SESSION))
}

fn route_agent(action: &str, input: Value) -> Result<(String, Value, &'static str)> {
    let op = match action {
        "create" => "agent.create",
        "delete" => "agent.delete",
        "config" => "agent.config",
        _ => invalid_action("agent", action, &["create", "delete", "config"])?,
    };
    Ok((op.to_string(), input, TOOL_AGENT))
}

fn route_tools(action: &str, input: Value) -> Result<(String, Value, &'static str)> {
    let op = match action {
        "skill_add" => "skill.add",
        "skill_promote" => "skill.promote",
        "js_add" => "js.tool.add",
        "js_list" => "js.tool.list",
        "js_delete" => "js.tool.delete",
        _ => invalid_action(
            "tools",
            action,
            &[
                "skill_add",
                "skill_promote",
                "js_add",
                "js_list",
                "js_delete",
            ],
        )?,
    };
    Ok((op.to_string(), input, TOOL_TOOLS))
}

fn invalid_action(domain: &str, action: &str, allowed: &[&str]) -> Result<&'static str> {
    anyhow::bail!(
        "invalid action '{action}' for domain '{domain}'. allowed actions: {}",
        allowed.join(", ")
    )
}

fn domain_tool(name: &str, description: &str, actions: &[&str]) -> Value {
    json!({
        "name": name,
        "description": format!("{description} Use when the user asks to perform this domain task via Moneypenny."),
        "inputSchema": {
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": actions },
                "input": { "type": "object", "default": {} },
                "request_id": { "type": "string" },
                "idempotency_key": { "type": "string" },
                "agent_id": { "type": "string" },
                "tenant_id": { "type": "string" },
                "user_id": { "type": "string" },
                "channel": { "type": "string" },
                "session_id": { "type": "string" },
                "trace_id": { "type": "string" }
            },
            "required": ["action"],
            "additionalProperties": true
        }
    })
}

fn card(domain: &str, tool: &str, actions: &[&str], summary: &str) -> Value {
    json!({
        "domain": domain,
        "tool": tool,
        "summary": summary,
        "actions": actions
    })
}

fn normalize_tool_name(name: &str) -> String {
    name.strip_prefix("moneypenny.").unwrap_or(name).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tools_list_exposes_compact_surface() {
        let list = tools_list();
        let tools = list["tools"].as_array().cloned().unwrap_or_default();
        assert_eq!(tools.len(), 13, "expected compact MCP surface (12 domain + 1 MPQ)");
        assert!(tools.iter().any(|t| t["name"] == TOOL_QUERY));
        assert!(tools.iter().any(|t| t["name"] == TOOL_MEMORY));
        assert!(tools.iter().any(|t| t["name"] == TOOL_CAPABILITIES));
        assert!(tools.iter().any(|t| t["name"] == TOOL_EXECUTE));
    }

    #[test]
    fn route_domain_tool_to_operation() {
        let routed = route_tool_call(
            TOOL_JOBS,
            &json!({
                "action": "resume",
                "input": { "id": "job-1" }
            }),
        )
        .expect("route tool");
        match routed {
            RoutedToolCall::Operation {
                op,
                execute_fallback,
                ..
            } => {
                assert_eq!(op, "job.resume");
                assert!(!execute_fallback);
            }
            _ => panic!("expected operation"),
        }
    }

    #[test]
    fn route_execute_fallback() {
        let routed = route_tool_call(
            TOOL_EXECUTE,
            &json!({
                "op": "ingest.status",
                "args": { "limit": 3 }
            }),
        )
        .expect("route execute");
        match routed {
            RoutedToolCall::Operation {
                execute_fallback, ..
            } => assert!(execute_fallback),
            _ => panic!("expected operation"),
        }
    }
}
