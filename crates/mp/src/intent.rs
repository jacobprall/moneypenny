//! Intent classification for agent messages.

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|n| haystack.contains(n))
}

/// Prefer text-only responses for explain/plan questions that don't request actions.
pub fn is_text_first(user_message: &str) -> bool {
    let s = user_message.to_lowercase();
    let asks_explain_or_plan = contains_any(
        &s,
        &[
            "explain",
            "why ",
            "what happened",
            "how does",
            "how do",
            "walk me through",
            "step by step",
            "plan",
            "summarize",
            "summary",
            "what should i do",
            "can you think of",
            "think of a good task",
            "suggest",
            "idea",
            "recommended task",
            "what would be a good task",
        ],
    );
    let asks_action = contains_any(
        &s,
        &[
            "create ",
            "add ",
            "update ",
            "delete ",
            "remove ",
            "ingest ",
            "schedule ",
            "run ",
            "execute ",
            "use tool",
            "call tool",
            "save ",
            "remember ",
            "set ",
        ],
    );
    asks_explain_or_plan && !asks_action
}

/// "Write confirmation" is treated as explicit user intent to perform mutations.
pub fn has_write_confirmation(user_message: &str) -> bool {
    let s = user_message.to_lowercase();
    contains_any(
        &s,
        &[
            "confirm",
            "approved",
            "go ahead",
            "yes do it",
            "please do it",
            "create ",
            "add ",
            "update ",
            "delete ",
            "remove ",
            "ingest ",
            "schedule ",
            "save ",
            "remember ",
            "set ",
            "run ",
            "execute ",
        ],
    )
}

/// Whether multiple tool calls are allowed for this message.
pub fn allow_multi_tool_calls(user_message: &str) -> bool {
    let s = user_message.to_lowercase();
    contains_any(
        &s,
        &[
            "use multiple tools",
            "use many tools",
            "run all tools",
            "show off all features",
            "full workflow",
        ],
    )
}

/// Tool name constants for agent dispatch.
pub mod tool_names {
    pub const WEB_SEARCH: &str = "web_search";
    pub const MEMORY_SEARCH: &str = "memory_search";
    pub const FACT_ADD: &str = "fact_add";
    pub const FACT_UPDATE: &str = "fact_update";
    pub const FACT_LIST: &str = "fact_list";
    pub const SCRATCH_SET: &str = "scratch_set";
    pub const SCRATCH_GET: &str = "scratch_get";
    pub const KNOWLEDGE_INGEST: &str = "knowledge_ingest";
    pub const KNOWLEDGE_LIST: &str = "knowledge_list";
    pub const JOB_CREATE: &str = "job_create";
    pub const JOB_PAUSE: &str = "job_pause";
    pub const JOB_RESUME: &str = "job_resume";
    pub const JOB_RUN: &str = "job_run";
    pub const JOB_LIST: &str = "job_list";
    pub const JS_TOOL_ADD: &str = "js_tool_add";
    pub const JS_TOOL_DELETE: &str = "js_tool_delete";
    pub const SHELL_EXEC: &str = "shell_exec";
    pub const FILE_READ: &str = "file_read";
    pub const POLICY_LIST: &str = "policy_list";
    pub const AUDIT_QUERY: &str = "audit_query";
    pub const DELEGATE: &str = "delegate_to_agent";
}

/// Whether a tool name represents a mutating operation.
pub fn is_mutating_tool(name: &str) -> bool {
    use tool_names::*;
    matches!(
        name,
        FACT_ADD
            | FACT_UPDATE
            | SCRATCH_SET
            | KNOWLEDGE_INGEST
            | JOB_CREATE
            | JOB_PAUSE
            | JOB_RESUME
            | JOB_RUN
            | JS_TOOL_ADD
            | JS_TOOL_DELETE
            | SHELL_EXEC
            | DELEGATE
    ) || name.starts_with("mcp:")
}

/// Whether a tool name represents a read-only operation.
pub fn is_read_only_tool(name: &str) -> bool {
    use tool_names::*;
    matches!(
        name,
        WEB_SEARCH
            | MEMORY_SEARCH
            | FACT_LIST
            | SCRATCH_GET
            | KNOWLEDGE_LIST
            | JOB_LIST
            | FILE_READ
            | POLICY_LIST
            | AUDIT_QUERY
    )
}
