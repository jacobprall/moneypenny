//! Tool registry — single source of truth for MCP domain tool metadata.

mod registry;

pub use registry::{
    all_tools, domain_allowed_actions, route_domain_action, TOOL_ACTIVITY, TOOL_BRAIN,
    TOOL_EVENTS, TOOL_EXECUTE, TOOL_EXPERIENCE, TOOL_FACTS, TOOL_FOCUS, TOOL_JOBS, TOOL_KNOWLEDGE,
    TOOL_POLICY,
};
