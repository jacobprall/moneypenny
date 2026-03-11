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

pub const TOOL_BRAIN: &str = "moneypenny_brain";
pub const TOOL_FACTS: &str = "moneypenny_facts";
pub const TOOL_KNOWLEDGE: &str = "moneypenny_knowledge";
pub const TOOL_POLICY: &str = "moneypenny_policy";
pub const TOOL_ACTIVITY: &str = "moneypenny_activity";
pub const TOOL_EXPERIENCE: &str = "moneypenny_experience";
pub const TOOL_EVENTS: &str = "moneypenny_events";
pub const TOOL_FOCUS: &str = "moneypenny_focus";
pub const TOOL_EXECUTE: &str = "moneypenny_execute";
pub const TOOL_JOBS: &str = "moneypenny_jobs";

/// Returns the MCP domain tools.
pub fn all_tools() -> Vec<ToolDef> {
    vec![
        tool_brain(),
        tool_facts(),
        tool_knowledge(),
        tool_policy(),
        tool_activity(),
        tool_experience(),
        tool_events(),
        tool_focus(),
        tool_jobs(),
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

fn tool_brain() -> ToolDef {
    ToolDef {
        name: TOOL_BRAIN,
        description: "Brain lifecycle — create, get, list, update, delete, checkpoint, restore, export.\n\nActions: create, get, list, update, delete, checkpoint, restore, export",
        actions: &[
            ("create", "brain.create"),
            ("get", "brain.get"),
            ("list", "brain.list"),
            ("update", "brain.update"),
            ("delete", "brain.delete"),
            ("checkpoint", "brain.checkpoint"),
            ("restore", "brain.restore"),
            ("export", "brain.export"),
        ],
        mutating: true,
        mcp_schema: json!({
            "name": TOOL_BRAIN,
            "description": "Brain lifecycle — create, get, list, update, delete, checkpoint, restore, export.\n\nActions: create, get, list, update, delete, checkpoint, restore, export",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["create", "get", "list", "update", "delete", "checkpoint", "restore", "export"],
                        "description": "create: provision brain\nget: fetch by id\nlist: list all\nupdate: update name/mission/config\ndelete: remove brain\ncheckpoint: snapshot to file\nrestore: restore from checkpoint\nexport: dump as JSON"
                    },
                    "input": {
                        "type": "object",
                        "description": "Parameters:\n- create: {name, mission?, config?}\n- get/list/update/delete: {brain_id}\n- checkpoint: {brain_id, name, output_path, include?}\n- restore: {checkpoint_path? checkpoint_id?, agent_db_path, mode?}\n- export: {brain_id, format?, output_path?}",
                        "default": {}
                    }
                },
                "required": ["action"],
                "additionalProperties": false
            }
        }),
    }
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
        description: "Ingest and retrieve documents — the agent's long-term reference library. Provide any URL to automatically fetch, extract, and store it as searchable knowledge.\n\nActions: ingest, search, list",
        actions: &[
            ("ingest", "knowledge.ingest"),
            ("search", "knowledge.search"),
            ("list", "knowledge.list"),
        ],
        mutating: true,
        mcp_schema: json!({
            "name": TOOL_KNOWLEDGE,
            "description": "Ingest and retrieve documents — the agent's long-term reference library. Provide any URL to automatically fetch, extract, and store it as searchable knowledge.\n\nActions: ingest, search, list",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["ingest", "search", "list"],
                        "description": "ingest: add a document — provide a URL (HTTP/HTTPS) to automatically fetch, strip HTML, chunk, and index a webpage; or provide raw content directly\nsearch: hybrid search across ingested documents\nlist: list all ingested documents"
                    },
                    "input": {
                        "type": "object",
                        "description": "Action-specific parameters:\n- ingest: {path?, content?, title?} — pass path as any HTTP/HTTPS URL to fetch and ingest a webpage automatically (HTML is cleaned and chunked), or provide content directly as text/markdown\n- search: {query, limit?}\n- list: {}",
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
        description: "Query session history, audit trail, token spend, and session briefings.\n\nActions: query, usage, briefing",
        actions: &[
            ("query", "activity.query"),
            ("usage", "usage.summary"),
            ("briefing", "briefing.compose"),
        ],
        mutating: false,
        mcp_schema: json!({
            "name": TOOL_ACTIVITY,
            "description": "Query session history, audit trail, token spend, and session briefings.\n\nActions: query, usage, briefing",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["query", "usage", "briefing"],
                        "description": "query: search session events and policy decisions\nusage: token/cost spend summary with breakdowns by model, session, or day\nbriefing: session recap — recent activity, new facts, denials, and spend"
                    },
                    "input": {
                        "type": "object",
                        "description": "Action-specific parameters:\n- query: {source?, event?, action?, resource?, agent_id?, conversation_id?, query?, limit?}\n- usage: {period?: 'today'|'week'|'month'|'all', group_by?: 'model'|'session'|'day'}\n- briefing: {} (no parameters needed)",
                        "default": {}
                    }
                },
                "required": ["action"],
                "additionalProperties": false
            }
        }),
    }
}

fn tool_experience() -> ToolDef {
    ToolDef {
        name: TOOL_EXPERIENCE,
        description: "Curated learned priors — failure patterns, command outcomes, budget signals.\n\nActions: record, match, resolve, ignore, search, stats, compact",
        actions: &[
            ("record", "brain.memories.experience.record"),
            ("match", "brain.memories.experience.match"),
            ("resolve", "brain.memories.experience.resolve"),
            ("ignore", "brain.memories.experience.ignore"),
            ("search", "brain.memories.experience.search"),
            ("stats", "brain.memories.experience.stats"),
            ("compact", "brain.memories.experience.compact"),
        ],
        mutating: true,
        mcp_schema: json!({
            "name": TOOL_EXPERIENCE,
            "description": "Curated learned priors — failure patterns, command outcomes, budget signals.\n\nActions: record, match, resolve, ignore, search, stats, compact",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["record", "match", "resolve", "ignore", "search", "stats", "compact"],
                        "description": "record: store a prior from learn phase\nmatch: hot-path pre-action lookup\nresolve: mark as solved with fix\nignore: suppress noisy prior\nsearch: free-text search\nstats: aggregate metrics\ncompact: decay/merge"
                    },
                    "input": {
                        "type": "object",
                        "description": "Action-specific parameters:\n- record: {type?, tool?, command?, error?, context, outcome, confidence?}\n- match: {type?, tool?, command?, error?, limit?}\n- resolve: {case_id, fix_text, fix_type?}\n- ignore: {case_id, reason?}\n- search: {query, limit?}\n- stats: {window_days?, type?}\n- compact: {min_confidence?, older_than_days?}",
                        "default": {}
                    }
                },
                "required": ["action"],
                "additionalProperties": false
            }
        }),
    }
}

fn tool_events() -> ToolDef {
    ToolDef {
        name: TOOL_EVENTS,
        description: "Unified event log — append and query brain events (messages, tool calls, policy decisions, etc.).\n\nActions: append, query, compact",
        actions: &[
            ("append", "brain.memories.events.append"),
            ("query", "brain.memories.events.query"),
            ("compact", "brain.memories.events.compact"),
        ],
        mutating: true,
        mcp_schema: json!({
            "name": TOOL_EVENTS,
            "description": "Unified event log — append and query brain events (messages, tool calls, policy decisions, etc.).\n\nActions: append, query, compact",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["append", "query", "compact"],
                        "description": "append: add a custom event\nquery: search events (unified with legacy activity_log + policy_audit)\ncompact: delete old events"
                    },
                    "input": {
                        "type": "object",
                        "description": "Action-specific parameters:\n- append: {event_type?, action, resource?, actor?, session_id?, correlation_id?, detail?}\n- query: {event_type?, action?, resource?, session_id?, query?, limit?}\n- compact: {older_than_days, confirm: true}",
                        "default": {}
                    }
                },
                "required": ["action"],
                "additionalProperties": false
            }
        }),
    }
}

fn tool_focus() -> ToolDef {
    ToolDef {
        name: TOOL_FOCUS,
        description: "Focus working set and context composition — scratchpad + compose under token budget.\n\nActions: set, get, list, clear, compose, composition_log, composition_last",
        actions: &[
            ("set", "brain.focus.set"),
            ("get", "brain.focus.get"),
            ("list", "brain.focus.list"),
            ("clear", "brain.focus.clear"),
            ("compose", "brain.focus.compose"),
            ("composition_log", "brain.focus.composition.log"),
            ("composition_last", "brain.focus.composition.last"),
        ],
        mutating: true,
        mcp_schema: json!({
            "name": TOOL_FOCUS,
            "description": "Focus working set and context composition — scratchpad + compose under token budget.\n\nActions: set, get, list, clear, compose, composition_log, composition_last",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["set", "get", "list", "clear", "compose", "composition_log", "composition_last"],
                        "description": "set: write working-set item\nget: read by key\nlist: list all for session\nclear: remove key or all\ncompose: build context under budget\ncomposition_log: inspect composition by id\ncomposition_last: most recent composition"
                    },
                    "input": {
                        "type": "object",
                        "description": "Parameters:\n- set: {key, content}\n- get/list/clear: {key?}\n- compose: {task_hint?, max_tokens?, session_id?, persona?, overrides?}\n- composition_log: {composition_id}\n- composition_last: {session_id?}",
                        "default": {}
                    }
                },
                "required": ["action"],
                "additionalProperties": false
            }
        }),
    }
}

fn tool_jobs() -> ToolDef {
    ToolDef {
        name: TOOL_JOBS,
        description: "Scheduled jobs — create, manage, and run recurring tasks (cron, JS scripts, pipelines).\n\nActions: create, list, run, pause, resume, history",
        actions: &[
            ("create", "job.create"),
            ("list", "job.list"),
            ("run", "job.run"),
            ("pause", "job.pause"),
            ("resume", "job.resume"),
            ("history", "job.history"),
        ],
        mutating: true,
        mcp_schema: json!({
            "name": TOOL_JOBS,
            "description": "Scheduled jobs — create, manage, and run recurring tasks (cron, JS scripts, pipelines).\n\nActions: create, list, run, pause, resume, history",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["create", "list", "run", "pause", "resume", "history"],
                        "description": "create: schedule a new job\nlist: list all jobs\nrun: trigger a job immediately\npause: pause a job\nresume: resume a paused job\nhistory: show job run history"
                    },
                    "input": {
                        "type": "object",
                        "description": "Action-specific parameters:\n- create: {name, schedule (cron), job_type?: 'prompt'|'tool'|'js'|'pipeline', payload?, description?}\n- list: {agent_id?}\n- run: {id}\n- pause: {id}\n- resume: {id}\n- history: {id?, limit?}",
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

