use anyhow::Result;
use serde_json::{Value, json};

pub const TOOL_FACTS: &str = "moneypenny_facts";
pub const TOOL_KNOWLEDGE: &str = "moneypenny_knowledge";
pub const TOOL_POLICY: &str = "moneypenny_policy";
pub const TOOL_ACTIVITY: &str = "moneypenny_activity";
pub const TOOL_EXECUTE: &str = "moneypenny_execute";

// Legacy constants — kept so routing still resolves old tool calls gracefully.
// Both dot and underscore prefixes are accepted by normalize_tool_name().
pub const TOOL_QUERY: &str = "moneypenny_query";
pub const TOOL_CAPABILITIES: &str = "moneypenny_capabilities";
const TOOL_MEMORY: &str = "moneypenny_memory";
const TOOL_JOBS: &str = "moneypenny_jobs";
const TOOL_AUDIT: &str = "moneypenny_audit";
const TOOL_INGEST: &str = "moneypenny_ingest";
const TOOL_EMBEDDING: &str = "moneypenny_embedding";
const TOOL_SESSION: &str = "moneypenny_session";
const TOOL_AGENT: &str = "moneypenny_agent";
const TOOL_TOOLS: &str = "moneypenny_tools";

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

// ── MCP tool definitions ─────────────────────────────────────────────

pub fn tools_list() -> Value {
    let tools = vec![
        tool_facts(),
        tool_knowledge(),
        tool_policy(),
        tool_activity(),
        tool_execute(),
    ];
    json!({ "tools": tools })
}

fn tool_facts() -> Value {
    json!({
        "name": TOOL_FACTS,
        "description": "Manage durable facts — persistent knowledge the agent remembers across sessions.\n\nActions: search, add, get, update, delete",
        "inputSchema": {
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["search", "add", "get", "update", "delete"],
                    "description": "search: hybrid search across facts (and optionally knowledge/logs)\nadd: store a new fact\nget: retrieve a fact by ID\nupdate: update an existing fact\ndelete: remove a fact"
                },
                "input": {
                    "type": "object",
                    "description": "Action-specific parameters:\n- search: {query, limit?}\n- add: {content, summary?, pointer?, keywords?, confidence?}\n- get: {id}\n- update: {id, content, summary?, pointer?}\n- delete: {id, reason?}",
                    "default": {}
                }
            },
            "required": ["action"],
            "additionalProperties": false
        }
    })
}

fn tool_knowledge() -> Value {
    json!({
        "name": TOOL_KNOWLEDGE,
        "description": "Ingest and retrieve documents — the agent's long-term reference library.\n\nActions: ingest, search, list",
        "inputSchema": {
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["ingest", "search", "list"],
                    "description": "ingest: add a document (provide content directly, or pass a URL or file:// path to fetch automatically)\nsearch: hybrid search across ingested documents\nlist: list all ingested documents"
                },
                "input": {
                    "type": "object",
                    "description": "Action-specific parameters:\n- ingest: {path?, content?, title?} — pass path as an HTTP/HTTPS URL to fetch and ingest a webpage, or provide content directly\n- search: {query, limit?}\n- list: {}",
                    "default": {}
                }
            },
            "required": ["action"],
            "additionalProperties": false
        }
    })
}

fn tool_policy() -> Value {
    json!({
        "name": TOOL_POLICY,
        "description": "Governance policies — control what agents can and cannot do.\n\nActions: add, list, disable, evaluate",
        "inputSchema": {
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["add", "list", "disable", "evaluate"],
                    "description": "add: create a new policy rule\nlist: list all policies (with optional filters)\ndisable: disable a policy by ID\nevaluate: test whether an action would be allowed"
                },
                "input": {
                    "type": "object",
                    "description": "Action-specific parameters:\n- add: {name, effect?, priority?, actor_pattern?, action_pattern?, resource_pattern?, sql_pattern?, argument_pattern?, message?}\n- list: {enabled?, effect?, limit?}\n- disable: {id}\n- evaluate: {actor, action, resource}",
                    "default": {}
                }
            },
            "required": ["action"],
            "additionalProperties": false
        }
    })
}

fn tool_activity() -> Value {
    json!({
        "name": TOOL_ACTIVITY,
        "description": "Query session history and audit trail — see what happened and why.\n\nActions: query",
        "inputSchema": {
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["query"],
                    "description": "query: search session events and policy decisions"
                },
                "input": {
                    "type": "object",
                    "description": "Parameters:\n- source?: 'events' (session history), 'decisions' (policy audit), or 'all' (default: 'all')\n- event?: filter by event type (e.g. 'beforeShellExecution')\n- action?: filter by action\n- resource?: filter by resource\n- agent_id?: filter by agent\n- conversation_id?: filter by session\n- query?: free-text search\n- limit?: max results (default 50)",
                    "default": {}
                }
            },
            "required": ["action"],
            "additionalProperties": false
        }
    })
}

fn tool_execute() -> Value {
    json!({
        "name": TOOL_EXECUTE,
        "description": "Direct operation call — escape hatch for any canonical operation not covered by the domain tools above.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "op": { "type": "string", "description": "Canonical operation name (e.g. 'job.create', 'ingest.events')" },
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
    })
}

// ── Routing ──────────────────────────────────────────────────────────

pub fn route_tool_call(tool_name: &str, arguments: &Value) -> Result<RoutedToolCall> {
    let normalized = normalize_tool_name(tool_name);

    // Legacy MPQ routing — still supported for backward compat
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
        return Ok(RoutedToolCall::Capabilities {
            payload: capabilities(None),
        });
    }

    // Direct execute passthrough
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

    // Domain tool routing
    let input = arguments.get("input").cloned().unwrap_or_else(|| json!({}));
    if !input.is_object() {
        anyhow::bail!("tool input must be an object");
    }
    let action = arguments
        .get("action")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("tool call requires string field 'action'"))?;

    let (op, args, domain_name) = match normalized.as_str() {
        "facts" => route_facts(action, input)?,
        "knowledge" => route_knowledge(action, input)?,
        "policy" => route_policy(action, input)?,
        "activity" => route_activity(action, input)?,
        // Legacy domain tools — still resolved
        "memory" => route_legacy_memory(action, input)?,
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
        // facts
        "memory.search",
        "memory.fact.add",
        "memory.fact.update",
        "memory.fact.get",
        "memory.fact.compaction.reset",
        "fact.delete",
        // knowledge
        "knowledge.ingest",
        "knowledge.search",
        "knowledge.list",
        // policy
        "policy.add",
        "policy.list",
        "policy.disable",
        "policy.evaluate",
        "policy.explain",
        "policy.spec.plan",
        "policy.spec.confirm",
        "policy.spec.apply",
        // activity
        "activity.query",
        "audit.query",
        "audit.append",
        // jobs (via execute)
        "job.create",
        "job.list",
        "job.run",
        "job.pause",
        "job.resume",
        "job.history",
        "job.spec.plan",
        "job.spec.confirm",
        "job.spec.apply",
        // other (via execute)
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

pub fn next_actions(_domain_tool: &str, _action: &str) -> Vec<Value> {
    vec![]
}

// ── Capabilities (still used by legacy routing) ──────────────────────

pub fn capabilities(domain_filter: Option<&str>) -> Value {
    let cards = vec![
        card("facts", TOOL_FACTS, &["search", "add", "get", "update", "delete"], "Durable facts — persistent knowledge across sessions."),
        card("knowledge", TOOL_KNOWLEDGE, &["ingest", "search", "list"], "Document ingestion and retrieval."),
        card("policy", TOOL_POLICY, &["add", "list", "disable", "evaluate"], "Governance policy management."),
        card("activity", TOOL_ACTIVITY, &["query"], "Session history and audit trail."),
        card("execute", TOOL_EXECUTE, &["(any canonical operation)"], "Direct operation call — escape hatch."),
    ];

    let filtered = if let Some(domain) = domain_filter {
        cards.into_iter().filter(|c| c["domain"].as_str() == Some(domain)).collect::<Vec<_>>()
    } else {
        cards
    };

    json!({
        "domains": filtered,
        "hint": "Use the domain tools (moneypenny_facts, moneypenny_knowledge, moneypenny_policy, moneypenny_activity) for common operations. Use moneypenny_execute for anything else."
    })
}

// ── MVP domain routing ───────────────────────────────────────────────

fn route_facts(action: &str, input: Value) -> Result<(String, Value, &'static str)> {
    let op = match action {
        "search" => "memory.search",
        "add" => "memory.fact.add",
        "get" => "memory.fact.get",
        "update" => "memory.fact.update",
        "delete" | "forget" => "fact.delete",
        _ => invalid_action("facts", action, &["search", "add", "get", "update", "delete"])?,
    };
    Ok((op.to_string(), input, TOOL_FACTS))
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
        "list" => "policy.list",
        "disable" => "policy.disable",
        "evaluate" => "policy.evaluate",
        "explain" => "policy.explain",
        _ => invalid_action("policy", action, &["add", "list", "disable", "evaluate", "explain"])?,
    };
    Ok((op.to_string(), input, TOOL_POLICY))
}

fn route_activity(action: &str, input: Value) -> Result<(String, Value, &'static str)> {
    let op = match action {
        "query" => "activity.query",
        _ => invalid_action("activity", action, &["query"])?,
    };
    Ok((op.to_string(), input, TOOL_ACTIVITY))
}

// ── Legacy domain routing (not advertised, still resolved) ───────────

fn route_legacy_memory(action: &str, input: Value) -> Result<(String, Value, &'static str)> {
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
            &["search", "add", "update", "get", "delete", "reset_compaction"],
        )?,
    };
    Ok((op.to_string(), input, TOOL_MEMORY))
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
            &["create", "list", "run", "pause", "resume", "history", "spec_plan", "spec_confirm", "spec_apply"],
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
            &["status", "retry_dead", "backfill_enqueue", "process", "backfill_process"],
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
            &["skill_add", "skill_promote", "js_add", "js_list", "js_delete"],
        )?,
    };
    Ok((op.to_string(), input, TOOL_TOOLS))
}

// ── Helpers ──────────────────────────────────────────────────────────

fn invalid_action(domain: &str, action: &str, allowed: &[&str]) -> Result<&'static str> {
    anyhow::bail!(
        "invalid action '{action}' for domain '{domain}'. allowed actions: {}",
        allowed.join(", ")
    )
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
    name.strip_prefix("moneypenny.")
        .or_else(|| name.strip_prefix("moneypenny_"))
        .unwrap_or(name)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tools_list_exposes_domain_surface() {
        let list = tools_list();
        let tools = list["tools"].as_array().cloned().unwrap_or_default();
        assert_eq!(tools.len(), 5, "MCP surface: facts + knowledge + policy + activity + execute");
        assert!(tools.iter().any(|t| t["name"] == TOOL_FACTS));
        assert!(tools.iter().any(|t| t["name"] == TOOL_KNOWLEDGE));
        assert!(tools.iter().any(|t| t["name"] == TOOL_POLICY));
        assert!(tools.iter().any(|t| t["name"] == TOOL_ACTIVITY));
        assert!(tools.iter().any(|t| t["name"] == TOOL_EXECUTE));
        // DSL/query tool should NOT be on the MCP surface
        assert!(!tools.iter().any(|t| t["name"] == TOOL_QUERY));
    }

    #[test]
    fn route_facts_search() {
        let routed = route_tool_call(
            TOOL_FACTS,
            &json!({"action": "search", "input": {"query": "auth"}}),
        ).unwrap();
        match routed {
            RoutedToolCall::Operation { op, domain_tool, action, .. } => {
                assert_eq!(op, "memory.search");
                assert_eq!(domain_tool, TOOL_FACTS);
                assert_eq!(action, "search");
            }
            _ => panic!("expected operation"),
        }
    }

    #[test]
    fn route_facts_add() {
        let routed = route_tool_call(
            TOOL_FACTS,
            &json!({"action": "add", "input": {"content": "test fact"}}),
        ).unwrap();
        match routed {
            RoutedToolCall::Operation { op, .. } => assert_eq!(op, "memory.fact.add"),
            _ => panic!("expected operation"),
        }
    }

    #[test]
    fn route_policy_list() {
        let routed = route_tool_call(
            TOOL_POLICY,
            &json!({"action": "list", "input": {}}),
        ).unwrap();
        match routed {
            RoutedToolCall::Operation { op, .. } => assert_eq!(op, "policy.list"),
            _ => panic!("expected operation"),
        }
    }

    #[test]
    fn route_policy_disable() {
        let routed = route_tool_call(
            TOOL_POLICY,
            &json!({"action": "disable", "input": {"id": "abc"}}),
        ).unwrap();
        match routed {
            RoutedToolCall::Operation { op, .. } => assert_eq!(op, "policy.disable"),
            _ => panic!("expected operation"),
        }
    }

    #[test]
    fn route_activity_query() {
        let routed = route_tool_call(
            TOOL_ACTIVITY,
            &json!({"action": "query", "input": {"source": "events", "limit": 10}}),
        ).unwrap();
        match routed {
            RoutedToolCall::Operation { op, domain_tool, .. } => {
                assert_eq!(op, "activity.query");
                assert_eq!(domain_tool, TOOL_ACTIVITY);
            }
            _ => panic!("expected operation"),
        }
    }

    #[test]
    fn route_execute_fallback() {
        let routed = route_tool_call(
            TOOL_EXECUTE,
            &json!({"op": "ingest.status", "args": {"limit": 3}}),
        ).unwrap();
        match routed {
            RoutedToolCall::Operation { execute_fallback, .. } => assert!(execute_fallback),
            _ => panic!("expected operation"),
        }
    }

    #[test]
    fn legacy_mpq_query_still_routes() {
        let routed = route_tool_call(
            TOOL_QUERY,
            &json!({"expression": "SEARCH facts", "dry_run": false}),
        ).unwrap();
        match routed {
            RoutedToolCall::MpqQuery { expression, dry_run } => {
                assert_eq!(expression, "SEARCH facts");
                assert!(!dry_run);
            }
            _ => panic!("expected MpqQuery"),
        }
    }

    #[test]
    fn legacy_memory_tool_still_routes() {
        let routed = route_tool_call(
            "moneypenny.memory",
            &json!({"action": "search", "input": {"query": "test"}}),
        ).unwrap();
        match routed {
            RoutedToolCall::Operation { op, .. } => assert_eq!(op, "memory.search"),
            _ => panic!("expected operation"),
        }
    }

    #[test]
    fn legacy_jobs_tool_still_routes() {
        let routed = route_tool_call(
            "moneypenny.jobs",
            &json!({"action": "resume", "input": {"id": "job-1"}}),
        ).unwrap();
        match routed {
            RoutedToolCall::Operation { op, .. } => assert_eq!(op, "job.resume"),
            _ => panic!("expected operation"),
        }
    }

    #[test]
    fn underscore_tool_names_route() {
        let routed = route_tool_call(
            "moneypenny_facts",
            &json!({"action": "search", "input": {"query": "test"}}),
        ).unwrap();
        match routed {
            RoutedToolCall::Operation { op, .. } => assert_eq!(op, "memory.search"),
            _ => panic!("expected operation"),
        }
    }

    #[test]
    fn dotted_tool_names_still_route() {
        let routed = route_tool_call(
            "moneypenny.facts",
            &json!({"action": "search", "input": {"query": "test"}}),
        ).unwrap();
        match routed {
            RoutedToolCall::Operation { op, .. } => assert_eq!(op, "memory.search"),
            _ => panic!("expected operation"),
        }
    }
}
