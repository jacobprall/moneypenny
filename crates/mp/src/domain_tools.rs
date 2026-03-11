use anyhow::Result;
use serde_json::{json, Value};

use crate::tools::{domain_allowed_actions, route_domain_action};

// Re-export registry constants for consumers (sidecar, etc.)
pub use crate::tools::{TOOL_ACTIVITY, TOOL_BRAIN, TOOL_EVENTS, TOOL_EXECUTE, TOOL_EXPERIENCE, TOOL_FACTS, TOOL_FOCUS, TOOL_KNOWLEDGE, TOOL_POLICY};

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

// ── MCP tool definitions (from registry) ─────────────────────────────

pub fn tools_list() -> Value {
    let tools: Vec<Value> = crate::tools::all_tools()
        .into_iter()
        .map(|t| t.mcp_schema)
        .collect();
    json!({ "tools": tools })
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
        "facts" | "knowledge" | "policy" | "activity" | "experience" | "events" | "focus" | "brain" => {
            if let Some((routed_op, tool_name)) = route_domain_action(normalized.as_str(), action) {
                (routed_op.to_string(), input, tool_name)
            } else {
                let allowed = domain_allowed_actions(normalized.as_str())
                    .map(|a| a.join(", "))
                    .unwrap_or_else(|| "?".to_string());
                anyhow::bail!(
                    "invalid action '{action}' for domain '{normalized}'. allowed actions: {allowed}"
                );
            }
        }
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
        "brain.memories.experience.record",
        "brain.memories.experience.match",
        "brain.memories.experience.resolve",
        "brain.memories.experience.ignore",
        "brain.memories.experience.search",
        "brain.memories.experience.stats",
        "brain.memories.experience.compact",
        "brain.memories.events.append",
        "brain.memories.events.query",
        "brain.memories.events.compact",
        "brain.focus.set",
        "brain.focus.get",
        "brain.focus.list",
        "brain.focus.clear",
        "brain.focus.compose",
        "brain.focus.composition.log",
        "brain.focus.composition.last",
        "brain.checkpoint",
        "brain.restore",
        "brain.export",
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
        card("brain", TOOL_BRAIN, &["create", "get", "list", "update", "delete", "checkpoint", "restore", "export"], "Brain lifecycle — snapshot, restore, export."),
        card("facts", TOOL_FACTS, &["search", "add", "get", "update", "delete"], "Durable facts — persistent knowledge across sessions."),
        card("knowledge", TOOL_KNOWLEDGE, &["ingest", "search", "list"], "Document ingestion and retrieval."),
        card("policy", TOOL_POLICY, &["add", "list", "disable", "evaluate"], "Governance policy management."),
        card("activity", TOOL_ACTIVITY, &["query"], "Session history and audit trail."),
        card("experience", TOOL_EXPERIENCE, &["record", "match", "resolve", "ignore", "search", "stats", "compact"], "Curated learned priors — failure patterns, command outcomes."),
        card("events", TOOL_EVENTS, &["append", "query", "compact"], "Unified event log — append and query brain events."),
        card("focus", TOOL_FOCUS, &["set", "get", "list", "clear", "compose", "composition_log", "composition_last"], "Focus working set + context composition."),
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
        assert_eq!(tools.len(), 9, "MCP surface: brain + facts + knowledge + policy + activity + experience + events + focus + execute");
        assert!(tools.iter().any(|t| t["name"] == TOOL_BRAIN));
        assert!(tools.iter().any(|t| t["name"] == TOOL_FACTS));
        assert!(tools.iter().any(|t| t["name"] == TOOL_KNOWLEDGE));
        assert!(tools.iter().any(|t| t["name"] == TOOL_POLICY));
        assert!(tools.iter().any(|t| t["name"] == TOOL_ACTIVITY));
        assert!(tools.iter().any(|t| t["name"] == TOOL_EXPERIENCE));
        assert!(tools.iter().any(|t| t["name"] == TOOL_EVENTS));
        assert!(tools.iter().any(|t| t["name"] == TOOL_FOCUS));
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
