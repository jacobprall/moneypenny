//! Canonical tool definitions for MCP domain tools.

use serde_json::{json, Value};

/// Canonical tool definition — name, description, action→op mapping, and MCP schema.
#[derive(Debug, Clone)]
#[allow(dead_code)] // description, mutating reserved for future LLM/agent use
pub struct ToolDef {
    pub name: &'static str,
    pub description: &'static str,
    /// (action, op) pairs for routing tool calls to operations.
    pub actions: &'static [(&'static str, &'static str)],
    pub mutating: bool,
    pub mcp_schema: Value,
}

pub const TOOL_FACTS: &str = "moneypenny_facts";
pub const TOOL_KNOWLEDGE: &str = "moneypenny_knowledge";
pub const TOOL_POLICY: &str = "moneypenny_policy";
pub const TOOL_ACTIVITY: &str = "moneypenny_activity";
pub const TOOL_EXECUTE: &str = "moneypenny_execute";

/// Returns the five MCP domain tools (facts, knowledge, policy, activity, execute).
pub fn all_tools() -> Vec<ToolDef> {
    vec![
        tool_facts(),
        tool_knowledge(),
        tool_policy(),
        tool_activity(),
        tool_execute(),
    ]
}

/// Routes a domain tool action to its canonical operation. Returns (op, tool_name) if found.
pub fn route_domain_action(domain: &str, action: &str) -> Option<(&'static str, &'static str)> {
    for t in all_tools() {
        if domain_name(t.name) == domain {
            for (a, op) in t.actions {
                if *a == action {
                    return Some((op, t.name));
                }
            }
            return None;
        }
    }
    None
}

/// Returns allowed actions for a domain (for error messages). None if domain unknown.
pub fn domain_allowed_actions(domain: &str) -> Option<Vec<&'static str>> {
    for t in all_tools() {
        if domain_name(t.name) == domain {
            return Some(t.actions.iter().map(|(a, _)| *a).collect());
        }
    }
    None
}

fn domain_name(tool_name: &str) -> &str {
    tool_name
        .strip_prefix("moneypenny_")
        .or_else(|| tool_name.strip_prefix("moneypenny."))
        .unwrap_or(tool_name)
}

fn tool_facts() -> ToolDef {
    ToolDef {
        name: TOOL_FACTS,
        description: "Manage durable facts — persistent knowledge the agent remembers across sessions.\n\nActions: search, add, get, update, delete",
        actions: &[
            ("search", "memory.search"),
            ("add", "memory.fact.add"),
            ("get", "memory.fact.get"),
            ("update", "memory.fact.update"),
            ("delete", "fact.delete"),
            ("forget", "fact.delete"),
        ],
        mutating: true,
        mcp_schema: json!({
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
        }),
    }
}

fn tool_knowledge() -> ToolDef {
    ToolDef {
        name: TOOL_KNOWLEDGE,
        description: "Ingest and retrieve documents — the agent's long-term reference library.\n\nActions: ingest, search, list",
        actions: &[
            ("ingest", "knowledge.ingest"),
            ("search", "knowledge.search"),
            ("list", "knowledge.list"),
        ],
        mutating: true,
        mcp_schema: json!({
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
        }),
    }
}

fn tool_policy() -> ToolDef {
    ToolDef {
        name: TOOL_POLICY,
        description: "Governance policies — control what agents can and cannot do.\n\nActions: add, list, disable, evaluate",
        actions: &[
            ("add", "policy.add"),
            ("list", "policy.list"),
            ("disable", "policy.disable"),
            ("evaluate", "policy.evaluate"),
            ("explain", "policy.explain"),
        ],
        mutating: true,
        mcp_schema: json!({
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
        }),
    }
}

fn tool_activity() -> ToolDef {
    ToolDef {
        name: TOOL_ACTIVITY,
        description: "Query session history and audit trail — see what happened and why.\n\nActions: query",
        actions: &[("query", "activity.query")],
        mutating: false,
        mcp_schema: json!({
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
        }),
    }
}

fn tool_execute() -> ToolDef {
    ToolDef {
        name: TOOL_EXECUTE,
        description: "Direct operation call — escape hatch for any canonical operation not covered by the domain tools above.",
        actions: &[], // execute uses op/args directly, not action/input
        mutating: true,
        mcp_schema: json!({
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
        }),
    }
}

