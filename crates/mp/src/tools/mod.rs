//! Tool registry — single source of truth for MCP domain tool metadata.

mod registry;

pub use registry::{
    all_tools, domain_allowed_actions, route_domain_action, ToolDef, TOOL_ACTIVITY, TOOL_EXECUTE,
    TOOL_FACTS, TOOL_KNOWLEDGE, TOOL_POLICY,
};
