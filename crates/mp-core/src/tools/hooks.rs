/// Pre- and post-execution hooks for tools.
///
/// Hooks run synchronously inside [`super::registry::execute`] at two points:
///
/// 1. **Pre-hooks** — after the policy check, before the tool runs.
///    A pre-hook can:
///    - Pass through unchanged (`Continue`)
///    - Substitute the argument string (`OverrideArgs`) — e.g. to sanitize input
///    - Block execution entirely (`Abort`) — e.g. to enforce a secondary check
///
/// 2. **Post-hooks** — after the tool runs, before secret redaction.
///    A post-hook can:
///    - Pass through unchanged (`Keep`)
///    - Replace the output text (`OverrideOutput`) — e.g. to truncate, enrich, or reformat
///
/// Hook handlers are Rust closures registered programmatically. Tool patterns use
/// the same simple glob syntax as policies: `*` matches anything, so `shell_*`
/// matches `shell_exec`, `shell_write`, etc.
///
/// # Example
///
/// ```rust
/// # use mp_core::tools::hooks::{ToolHooks, HookContext, PreOutcome, PostOutcome};
/// # use mp_core::tools::registry::ToolResult;
/// let mut hooks = ToolHooks::new();
///
/// // Block shell commands containing "rm -rf"
/// hooks.add_pre("no-rm-rf", "shell_exec", |_ctx, args| {
///     if args.contains("rm -rf") {
///         PreOutcome::Abort("Destructive shell commands are not permitted.".into())
///     } else {
///         PreOutcome::Continue { args: None }
///     }
/// });
///
/// // Truncate very long HTTP responses
/// hooks.add_post("http-truncate", "http_request", |_ctx, result| {
///     if result.output.len() > 10_000 {
///         PostOutcome::OverrideOutput(format!("{}…[truncated]", &result.output[..10_000]))
///     } else {
///         PostOutcome::Keep
///     }
/// });
/// ```
use super::registry::ToolResult;

// =========================================================================
// Context
// =========================================================================

/// Immutable context passed to every hook handler.
#[derive(Debug, Clone)]
pub struct HookContext {
    pub tool_name: String,
    pub agent_id: String,
    pub session_id: String,
}

// =========================================================================
// Outcomes
// =========================================================================

/// Decision returned by a pre-execution hook.
pub enum PreOutcome {
    /// Let execution proceed.  If `args` is `Some`, use it instead of the
    /// original arguments for this tool call and all subsequent pre-hooks.
    Continue { args: Option<String> },
    /// Block execution and return this string as the tool output.
    Abort(String),
}

/// Decision returned by a post-execution hook.
pub enum PostOutcome {
    /// Keep the existing output unchanged.
    Keep,
    /// Replace the output with this string.
    OverrideOutput(String),
}

// =========================================================================
// Hook storage
// =========================================================================

type PreFn = Box<dyn Fn(&HookContext, &str) -> PreOutcome + Send + Sync>;
type PostFn = Box<dyn Fn(&HookContext, &ToolResult) -> PostOutcome + Send + Sync>;

struct PreEntry {
    name: String,
    pattern: String,
    f: PreFn,
}
struct PostEntry {
    name: String,
    pattern: String,
    f: PostFn,
}

// =========================================================================
// Registry
// =========================================================================

/// Container for pre- and post-execution hooks.
///
/// Build one instance per gateway/session and pass it to
/// [`super::registry::execute`] as `Option<&ToolHooks>`.
pub struct ToolHooks {
    pre: Vec<PreEntry>,
    post: Vec<PostEntry>,
}

impl Default for ToolHooks {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolHooks {
    pub fn new() -> Self {
        Self {
            pre: Vec::new(),
            post: Vec::new(),
        }
    }

    /// Register a pre-execution hook for tools whose name matches `pattern`.
    ///
    /// Hooks are evaluated in registration order.  The first `Abort` wins and
    /// prevents any remaining pre-hooks and the tool itself from running.
    pub fn add_pre<F>(&mut self, name: &str, pattern: &str, f: F)
    where
        F: Fn(&HookContext, &str) -> PreOutcome + Send + Sync + 'static,
    {
        self.pre.push(PreEntry {
            name: name.into(),
            pattern: pattern.into(),
            f: Box::new(f),
        });
    }

    /// Register a post-execution hook for tools whose name matches `pattern`.
    ///
    /// Post-hooks run in registration order.  Each hook sees the output produced
    /// by the previous hook, so they chain naturally.
    pub fn add_post<F>(&mut self, name: &str, pattern: &str, f: F)
    where
        F: Fn(&HookContext, &ToolResult) -> PostOutcome + Send + Sync + 'static,
    {
        self.post.push(PostEntry {
            name: name.into(),
            pattern: pattern.into(),
            f: Box::new(f),
        });
    }

    /// Run all matching pre-hooks in order.
    ///
    /// Returns the effective (possibly overridden) argument string, or `None`
    /// if no hook modified the args, or `Err(message)` if a hook aborted.
    pub(super) fn run_pre(
        &self,
        ctx: &HookContext,
        original_args: &str,
    ) -> Result<Option<String>, String> {
        let mut current: Option<String> = None;

        for entry in &self.pre {
            if !glob_match(&entry.pattern, &ctx.tool_name) {
                continue;
            }
            let args = current.as_deref().unwrap_or(original_args);
            match (entry.f)(ctx, args) {
                PreOutcome::Abort(msg) => {
                    tracing::debug!(hook = %entry.name, tool = %ctx.tool_name, "pre-hook aborted");
                    return Err(msg);
                }
                PreOutcome::Continue {
                    args: Some(new_args),
                } => {
                    tracing::trace!(hook = %entry.name, tool = %ctx.tool_name, "pre-hook overrode args");
                    current = Some(new_args);
                }
                PreOutcome::Continue { args: None } => {}
            }
        }

        Ok(current)
    }

    /// Run all matching post-hooks in order.
    ///
    /// Returns the (possibly modified) output string, or `None` if no hook
    /// changed anything.
    pub(super) fn run_post(&self, ctx: &HookContext, result: &ToolResult) -> Option<String> {
        let mut current: Option<String> = None;

        for entry in &self.post {
            if !glob_match(&entry.pattern, &ctx.tool_name) {
                continue;
            }

            let effective = if let Some(ref s) = current {
                ToolResult {
                    output: s.clone(),
                    ..*result
                }
            } else {
                result.clone()
            };

            match (entry.f)(ctx, &effective) {
                PostOutcome::OverrideOutput(new_out) => {
                    tracing::trace!(hook = %entry.name, tool = %ctx.tool_name, "post-hook overrode output");
                    current = Some(new_out);
                }
                PostOutcome::Keep => {}
            }
        }

        current
    }
}

// =========================================================================
// Glob matching (same simple semantics as policy.rs)
// =========================================================================

fn glob_match(pattern: &str, value: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if let Some(prefix) = pattern.strip_suffix('*') {
        return value.starts_with(prefix);
    }
    if let Some(suffix) = pattern.strip_prefix('*') {
        return value.ends_with(suffix);
    }
    pattern == value
}

// =========================================================================
// Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::registry::ToolResult;

    fn ctx(tool: &str) -> HookContext {
        HookContext {
            tool_name: tool.into(),
            agent_id: "agent".into(),
            session_id: "session".into(),
        }
    }

    fn ok_result(output: &str) -> ToolResult {
        ToolResult {
            output: output.into(),
            success: true,
            duration_ms: 0,
        }
    }

    // =========================================================================
    // glob_match
    // =========================================================================

    #[test]
    fn glob_star_matches_anything() {
        assert!(glob_match("*", "shell_exec"));
        assert!(glob_match("*", ""));
    }

    #[test]
    fn glob_prefix_star_matches_suffix() {
        assert!(glob_match("shell_*", "shell_exec"));
        assert!(glob_match("shell_*", "shell_write"));
        assert!(!glob_match("shell_*", "file_read"));
    }

    #[test]
    fn glob_suffix_star_matches_prefix() {
        assert!(glob_match("*_exec", "shell_exec"));
        assert!(!glob_match("*_exec", "shell_write"));
    }

    #[test]
    fn glob_exact_match() {
        assert!(glob_match("shell_exec", "shell_exec"));
        assert!(!glob_match("shell_exec", "shell_write"));
    }

    // =========================================================================
    // Pre-hooks
    // =========================================================================

    #[test]
    fn pre_hook_continue_passes_through() {
        let hooks = ToolHooks::new();
        let result = hooks.run_pre(&ctx("shell_exec"), r#"{"command":"ls"}"#);
        assert!(result.is_ok());
        assert!(result.unwrap().is_none()); // no override
    }

    #[test]
    fn pre_hook_abort_blocks_execution() {
        let mut hooks = ToolHooks::new();
        hooks.add_pre("blocker", "shell_exec", |_, args| {
            if args.contains("rm -rf") {
                PreOutcome::Abort("Destructive command blocked.".into())
            } else {
                PreOutcome::Continue { args: None }
            }
        });

        let blocked = hooks.run_pre(&ctx("shell_exec"), r#"{"command":"rm -rf /"}"#);
        assert!(blocked.is_err());
        assert!(blocked.unwrap_err().contains("Destructive"));

        let allowed = hooks.run_pre(&ctx("shell_exec"), r#"{"command":"ls"}"#);
        assert!(allowed.is_ok());
    }

    #[test]
    fn pre_hook_override_args_replaces_arguments() {
        let mut hooks = ToolHooks::new();
        hooks.add_pre("sanitize", "shell_exec", |_, _args| PreOutcome::Continue {
            args: Some(r#"{"command":"echo safe"}"#.into()),
        });

        let result = hooks
            .run_pre(&ctx("shell_exec"), r#"{"command":"original"}"#)
            .unwrap();
        assert_eq!(result.as_deref(), Some(r#"{"command":"echo safe"}"#));
    }

    #[test]
    fn pre_hooks_chain_args() {
        let mut hooks = ToolHooks::new();
        // First hook appends " STEP1"
        hooks.add_pre("h1", "*", |_, args| PreOutcome::Continue {
            args: Some(format!("{args} STEP1")),
        });
        // Second hook sees modified args and appends " STEP2"
        hooks.add_pre("h2", "*", |_, args| PreOutcome::Continue {
            args: Some(format!("{args} STEP2")),
        });

        let result = hooks.run_pre(&ctx("any_tool"), "BASE").unwrap();
        assert_eq!(result.as_deref(), Some("BASE STEP1 STEP2"));
    }

    #[test]
    fn pre_hook_first_abort_wins() {
        let mut hooks = ToolHooks::new();
        hooks.add_pre("abort-first", "*", |_, _| {
            PreOutcome::Abort("first abort".into())
        });
        hooks.add_pre("abort-second", "*", |_, _| {
            PreOutcome::Abort("second abort".into())
        });

        let err = hooks.run_pre(&ctx("any_tool"), "args").unwrap_err();
        assert_eq!(err, "first abort");
    }

    #[test]
    fn pre_hook_pattern_filters_by_tool() {
        let mut hooks = ToolHooks::new();
        hooks.add_pre("shell-only", "shell_exec", |_, _| {
            PreOutcome::Abort("blocked".into())
        });

        // Matches shell_exec
        assert!(hooks.run_pre(&ctx("shell_exec"), "{}").is_err());
        // Does not match file_read
        assert!(hooks.run_pre(&ctx("file_read"), "{}").is_ok());
    }

    #[test]
    fn pre_hook_none_registered_is_noop() {
        let hooks = ToolHooks::new();
        let result = hooks.run_pre(&ctx("anything"), "{}");
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    // =========================================================================
    // Post-hooks
    // =========================================================================

    #[test]
    fn post_hook_keep_returns_none() {
        let hooks = ToolHooks::new();
        let out = hooks.run_post(&ctx("shell_exec"), &ok_result("hello"));
        assert!(out.is_none());
    }

    #[test]
    fn post_hook_override_replaces_output() {
        let mut hooks = ToolHooks::new();
        hooks.add_post("truncate", "http_request", |_, _| {
            PostOutcome::OverrideOutput("TRUNCATED".into())
        });

        let out = hooks
            .run_post(&ctx("http_request"), &ok_result("long output"))
            .unwrap();
        assert_eq!(out, "TRUNCATED");
    }

    #[test]
    fn post_hooks_chain_output() {
        let mut hooks = ToolHooks::new();
        hooks.add_post("append-a", "*", |_, r| {
            PostOutcome::OverrideOutput(format!("{} A", r.output))
        });
        hooks.add_post("append-b", "*", |_, r| {
            PostOutcome::OverrideOutput(format!("{} B", r.output))
        });

        let out = hooks
            .run_post(&ctx("any_tool"), &ok_result("base"))
            .unwrap();
        assert_eq!(out, "base A B");
    }

    #[test]
    fn post_hook_pattern_filters_by_tool() {
        let mut hooks = ToolHooks::new();
        hooks.add_post("http-only", "http_request", |_, _| {
            PostOutcome::OverrideOutput("transformed".into())
        });

        assert!(
            hooks
                .run_post(&ctx("http_request"), &ok_result("out"))
                .is_some()
        );
        assert!(
            hooks
                .run_post(&ctx("file_read"), &ok_result("out"))
                .is_none()
        );
    }

    #[test]
    fn post_hook_sees_failed_results() {
        let mut hooks = ToolHooks::new();
        hooks.add_post("error-fmt", "*", |_, r| {
            if !r.success {
                PostOutcome::OverrideOutput(format!("[ERROR] {}", r.output))
            } else {
                PostOutcome::Keep
            }
        });

        let failed = ToolResult {
            output: "timeout".into(),
            success: false,
            duration_ms: 100,
        };
        let out = hooks.run_post(&ctx("shell_exec"), &failed).unwrap();
        assert_eq!(out, "[ERROR] timeout");
    }

    #[test]
    fn no_hooks_registered_is_noop() {
        let hooks = ToolHooks::new();
        let result = ok_result("output");
        assert!(hooks.run_post(&ctx("anything"), &result).is_none());
    }
}
