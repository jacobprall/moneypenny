use rusqlite::{Connection, params};
use uuid::Uuid;

/// Result of a policy evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    Allow,
    Deny,
    Audit,
}

/// What happens when no policy rule matches a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyMode {
    /// Reject anything not explicitly allowed. Use for production/governed agents.
    DenyByDefault,
    /// Allow anything not explicitly denied. Use for development/exploration.
    AllowByDefault,
}

impl Default for PolicyMode {
    fn default() -> Self {
        PolicyMode::AllowByDefault
    }
}

/// Full result of evaluating a policy request.
#[derive(Debug, Clone)]
pub struct PolicyDecision {
    pub effect: Effect,
    pub policy_id: Option<String>,
    pub reason: Option<String>,
}

/// A request to the policy engine.
#[derive(Debug)]
pub struct PolicyRequest<'a> {
    pub actor: &'a str,
    pub action: &'a str,
    pub resource: &'a str,
    pub sql_content: Option<&'a str>,
    pub channel: Option<&'a str>,
    /// Free-form argument string for matching against `argument_pattern`.
    /// For URL ingest this carries the URL; for tool calls it could carry
    /// serialized arguments, etc.
    pub arguments: Option<&'a str>,
}

#[derive(Debug, Clone, Default)]
pub struct PolicyAuditContext<'a> {
    pub session_id: Option<&'a str>,
    pub correlation_id: Option<&'a str>,
    pub idempotency_key: Option<&'a str>,
    pub idempotency_state: Option<&'a str>,
}

/// Evaluate a policy request against the policies table.
///
/// When no rule matches, the fallback depends on `mode`:
/// - `DenyByDefault` — rejects the request (production/governed)
/// - `AllowByDefault` — permits the request (development/exploration)
pub fn evaluate(conn: &Connection, req: &PolicyRequest) -> anyhow::Result<PolicyDecision> {
    evaluate_with_mode_and_audit(
        conn,
        req,
        PolicyMode::default(),
        &PolicyAuditContext::default(),
    )
}

/// A fetched policy row with all columns.
struct PolicyRow {
    id: String,
    effect: String,
    actor_pattern: Option<String>,
    action_pattern: Option<String>,
    resource_pattern: Option<String>,
    sql_pattern: Option<String>,
    argument_pattern: Option<String>,
    channel_pattern: Option<String>,
    message: Option<String>,
    rule_type: Option<String>,
    rule_config: Option<String>,
}

/// Evaluate with an explicit policy mode.
pub fn evaluate_with_mode(
    conn: &Connection,
    req: &PolicyRequest,
    mode: PolicyMode,
) -> anyhow::Result<PolicyDecision> {
    evaluate_with_mode_and_audit(conn, req, mode, &PolicyAuditContext::default())
}

pub fn evaluate_with_audit(
    conn: &Connection,
    req: &PolicyRequest,
    audit: &PolicyAuditContext,
) -> anyhow::Result<PolicyDecision> {
    evaluate_with_mode_and_audit(conn, req, PolicyMode::default(), audit)
}

fn evaluate_with_mode_and_audit(
    conn: &Connection,
    req: &PolicyRequest,
    mode: PolicyMode,
    audit: &PolicyAuditContext,
) -> anyhow::Result<PolicyDecision> {
    let mut stmt = conn.prepare(
        "SELECT id, effect, actor_pattern, action_pattern, resource_pattern,
                sql_pattern, argument_pattern, channel_pattern, message,
                rule_type, rule_config
         FROM policies
         WHERE enabled = 1
         ORDER BY priority DESC",
    )?;

    let policies = stmt
        .query_map([], |r| {
            Ok(PolicyRow {
                id: r.get(0)?,
                effect: r.get(1)?,
                actor_pattern: r.get(2)?,
                action_pattern: r.get(3)?,
                resource_pattern: r.get(4)?,
                sql_pattern: r.get(5)?,
                argument_pattern: r.get(6)?,
                channel_pattern: r.get(7)?,
                message: r.get(8)?,
                rule_type: r.get(9)?,
                rule_config: r.get(10)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    for row in &policies {
        if !matches_pattern(row.actor_pattern.as_deref(), req.actor) {
            continue;
        }
        if !matches_pattern(row.action_pattern.as_deref(), req.action) {
            continue;
        }
        if !matches_pattern(row.resource_pattern.as_deref(), req.resource) {
            continue;
        }
        if let Some(sql_re) = &row.sql_pattern {
            if let Some(sql) = req.sql_content {
                if !regex_matches(sql_re, sql) {
                    continue;
                }
            } else {
                continue;
            }
        }
        if let Some(arg_pat) = &row.argument_pattern {
            if let Some(args) = req.arguments {
                if !glob_match(arg_pat, args) {
                    continue;
                }
            } else {
                continue;
            }
        }
        if !matches_pattern(row.channel_pattern.as_deref(), req.channel.unwrap_or("")) {
            if row.channel_pattern.is_some() {
                continue;
            }
        }

        // Behavioral rule check: if this policy has a rule_type, evaluate the
        // behavioral condition. If the condition is NOT triggered, skip this
        // rule and continue to lower-priority rules.
        if let Some(rt) = &row.rule_type {
            let config_json = row.rule_config.as_deref().unwrap_or("{}");
            if !evaluate_behavioral(conn, rt, config_json, req)? {
                continue;
            }
        }

        let eff = match row.effect.as_str() {
            "allow" => Effect::Allow,
            "deny" => Effect::Deny,
            "audit" => Effect::Audit,
            _ => continue,
        };

        let decision = PolicyDecision {
            effect: eff,
            policy_id: Some(row.id.clone()),
            reason: row.message.clone(),
        };

        log_decision(conn, &decision, req, audit)?;
        return Ok(decision);
    }

    let decision = match mode {
        PolicyMode::DenyByDefault => PolicyDecision {
            effect: Effect::Deny,
            policy_id: None,
            reason: Some("No matching policy rule (deny-by-default).".into()),
        },
        PolicyMode::AllowByDefault => PolicyDecision {
            effect: Effect::Allow,
            policy_id: None,
            reason: Some("No matching policy rule (allow-by-default).".into()),
        },
    };
    log_decision(conn, &decision, req, audit)?;
    Ok(decision)
}

// ---------------------------------------------------------------------------
// Behavioral rule evaluators
// ---------------------------------------------------------------------------

/// Evaluate a behavioral rule condition. Returns true if the condition is
/// triggered (i.e., the rule should fire).
fn evaluate_behavioral(
    conn: &Connection,
    rule_type: &str,
    config_json: &str,
    req: &PolicyRequest,
) -> anyhow::Result<bool> {
    match rule_type {
        "rate_limit" => eval_rate_limit(conn, config_json, req),
        "retry_loop" => eval_retry_loop(conn, config_json, req),
        "token_budget" => eval_token_budget(conn, config_json),
        "time_window" => eval_time_window(config_json),
        _ => Ok(false),
    }
}

/// rate_limit: triggers when tool call count in the window exceeds max.
/// Config: {"max": N, "window_seconds": S}
fn eval_rate_limit(
    conn: &Connection,
    config_json: &str,
    req: &PolicyRequest,
) -> anyhow::Result<bool> {
    let cfg: serde_json::Value = serde_json::from_str(config_json)?;
    let max = cfg["max"].as_i64().unwrap_or(100);
    let window = cfg["window_seconds"].as_i64().unwrap_or(300);
    let since = chrono::Utc::now().timestamp() - window;

    let resource_pattern = format!(
        "%{}%",
        req.resource.split(':').last().unwrap_or(req.resource)
    );

    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM tool_calls
         WHERE tool_name LIKE ?1 AND created_at >= ?2",
            params![resource_pattern, since],
            |r| r.get(0),
        )
        .unwrap_or(0);

    Ok(count >= max)
}

/// retry_loop: triggers when the same tool+args appear N times in the window.
/// Config: {"same_tool_same_args": N, "window_seconds": S}
fn eval_retry_loop(
    conn: &Connection,
    config_json: &str,
    _req: &PolicyRequest,
) -> anyhow::Result<bool> {
    let cfg: serde_json::Value = serde_json::from_str(config_json)?;
    let threshold = cfg["same_tool_same_args"].as_i64().unwrap_or(3);
    let window = cfg["window_seconds"].as_i64().unwrap_or(60);
    let since = chrono::Utc::now().timestamp() - window;

    let max_repeat: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(cnt), 0) FROM (
            SELECT COUNT(*) as cnt FROM tool_calls
            WHERE created_at >= ?1
            GROUP BY tool_name, arguments
        )",
            params![since],
            |r| r.get(0),
        )
        .unwrap_or(0);

    Ok(max_repeat >= threshold)
}

/// token_budget: triggers when estimated session tokens exceed the budget.
/// Config: {"max_tokens_per_session": N}
/// Approximates tokens as character_count / 4.
fn eval_token_budget(conn: &Connection, config_json: &str) -> anyhow::Result<bool> {
    let cfg: serde_json::Value = serde_json::from_str(config_json)?;
    let max_tokens = cfg["max_tokens_per_session"].as_i64().unwrap_or(500_000);

    let total_chars: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(LENGTH(content)), 0) FROM messages
         WHERE session_id = (SELECT id FROM sessions ORDER BY started_at DESC LIMIT 1)",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let estimated_tokens = total_chars / 4;
    Ok(estimated_tokens >= max_tokens)
}

/// time_window: triggers when the current time is within the specified hours.
/// Config: {"start_hour": H, "end_hour": H, "days": [1,2,3,4,5]} (1=Mon, 7=Sun)
/// If no config, always triggers (rule is always active).
fn eval_time_window(config_json: &str) -> anyhow::Result<bool> {
    let cfg: serde_json::Value = serde_json::from_str(config_json)?;
    let now = chrono::Utc::now();
    let hour = now.format("%H").to_string().parse::<u32>().unwrap_or(0);
    let weekday = now.format("%u").to_string().parse::<u32>().unwrap_or(1); // 1=Mon

    if let (Some(start), Some(end)) = (cfg["start_hour"].as_u64(), cfg["end_hour"].as_u64()) {
        let in_hours = hour >= start as u32 && hour < end as u32;
        if !in_hours {
            return Ok(false);
        }
    }

    if let Some(days) = cfg["days"].as_array() {
        let day_list: Vec<u32> = days
            .iter()
            .filter_map(|d| d.as_u64().map(|v| v as u32))
            .collect();
        if !day_list.is_empty() && !day_list.contains(&weekday) {
            return Ok(false);
        }
    }

    Ok(true)
}

/// Generate a SQL WHERE clause fragment from policies for data-level filtering.
/// For example, fact scope filtering based on agent identity.
pub fn generate_sql_filter(conn: &Connection, agent_id: &str) -> anyhow::Result<String> {
    // Base filter: agent sees its own private facts + all shared facts
    let mut clauses: Vec<String> = vec![format!("(agent_id = '{agent_id}')")];

    // Check for confidence threshold policies
    let threshold: Option<f64> = conn
        .query_row(
            "SELECT CAST(message AS REAL) FROM policies
         WHERE enabled = 1 AND effect = 'deny' AND resource_pattern = 'fact:low_confidence'
         ORDER BY priority DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .ok();

    if let Some(min_confidence) = threshold {
        clauses.push(format!("confidence >= {min_confidence}"));
    }

    clauses.push("superseded_at IS NULL".into());

    Ok(clauses.join(" AND "))
}

/// Log a policy decision to the audit trail.
fn log_decision(
    conn: &Connection,
    decision: &PolicyDecision,
    req: &PolicyRequest,
    audit: &PolicyAuditContext,
) -> anyhow::Result<()> {
    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp();
    let effect_str = match decision.effect {
        Effect::Allow => "allowed",
        Effect::Deny => "denied",
        Effect::Audit => "audited",
    };

    conn.execute(
        "INSERT INTO policy_audit (id, policy_id, actor, action, resource, effect, reason, correlation_id, session_id, idempotency_key, idempotency_state, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            id,
            decision.policy_id,
            req.actor,
            req.action,
            req.resource,
            effect_str,
            decision.reason,
            audit.correlation_id,
            audit.session_id,
            audit.idempotency_key,
            audit.idempotency_state,
            now
        ],
    )?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Glob-style pattern matching
// ---------------------------------------------------------------------------

/// Match a glob pattern against a value.
/// Supports `*` as wildcard (matches any sequence of chars).
/// None pattern matches everything.
fn matches_pattern(pattern: Option<&str>, value: &str) -> bool {
    match pattern {
        None => true,
        Some("*") => true,
        Some(pat) => glob_match(pat, value),
    }
}

fn glob_match(pattern: &str, value: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return pattern == value;
    }

    let mut pos = 0;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        match value[pos..].find(part) {
            Some(found) => {
                if i == 0 && found != 0 {
                    return false; // Pattern doesn't start with *, must match from beginning
                }
                pos += found + part.len();
            }
            None => return false,
        }
    }

    // If pattern doesn't end with *, remaining value must be consumed
    if !pattern.ends_with('*') {
        return pos == value.len();
    }

    true
}

fn regex_matches(pattern: &str, text: &str) -> bool {
    regex::Regex::new(pattern)
        .map(|re| re.is_match(text))
        .unwrap_or(false)
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

    // ========================================================================
    // Glob matching
    // ========================================================================

    #[test]
    fn glob_exact_match() {
        assert!(glob_match("agent:alpha", "agent:alpha"));
        assert!(!glob_match("agent:alpha", "agent:beta"));
    }

    #[test]
    fn glob_wildcard_suffix() {
        assert!(glob_match("agent:*", "agent:alpha"));
        assert!(glob_match("agent:*", "agent:beta"));
        assert!(!glob_match("agent:*", "channel:slack"));
    }

    #[test]
    fn glob_wildcard_prefix() {
        assert!(glob_match("*:alpha", "agent:alpha"));
        assert!(!glob_match("*:alpha", "agent:beta"));
    }

    #[test]
    fn glob_wildcard_middle() {
        assert!(glob_match("tool:shell_*_exec", "tool:shell_safe_exec"));
        assert!(!glob_match("tool:shell_*_exec", "tool:shell_safe_run"));
    }

    #[test]
    fn glob_star_matches_all() {
        assert!(matches_pattern(Some("*"), "anything"));
    }

    #[test]
    fn glob_none_matches_all() {
        assert!(matches_pattern(None, "anything"));
    }

    // ========================================================================
    // Default fallthrough behavior
    // ========================================================================

    #[test]
    fn deny_by_default_when_no_policies() {
        let conn = setup();
        let decision = evaluate_with_mode(
            &conn,
            &PolicyRequest {
                actor: "agent:main",
                action: "call",
                resource: "tool:shell_exec",
                sql_content: None,
                channel: None,
                arguments: None,
            },
            PolicyMode::DenyByDefault,
        )
        .unwrap();

        assert_eq!(decision.effect, Effect::Deny);
        assert!(decision.policy_id.is_none());
    }

    #[test]
    fn allow_by_default_when_no_policies() {
        let conn = setup();
        let decision = evaluate_with_mode(
            &conn,
            &PolicyRequest {
                actor: "agent:main",
                action: "call",
                resource: "tool:shell_exec",
                sql_content: None,
                channel: None,
                arguments: None,
            },
            PolicyMode::AllowByDefault,
        )
        .unwrap();

        assert_eq!(decision.effect, Effect::Allow);
        assert!(decision.policy_id.is_none());
    }

    #[test]
    fn default_mode_is_allow() {
        let conn = setup();
        let decision = evaluate(
            &conn,
            &PolicyRequest {
                actor: "agent:main",
                action: "call",
                resource: "tool:shell_exec",
                sql_content: None,
                channel: None,
                arguments: None,
            },
        )
        .unwrap();

        assert_eq!(decision.effect, Effect::Allow);
    }

    // ========================================================================
    // Allow rules
    // ========================================================================

    #[test]
    fn allow_rule_matches() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, created_at)
             VALUES ('p1', 'allow all tools', 10, 'allow', 'agent:*', 'call', 'tool:*', 1)",
            [],
        ).unwrap();

        let decision = evaluate(
            &conn,
            &PolicyRequest {
                actor: "agent:main",
                action: "call",
                resource: "tool:http_get",
                sql_content: None,
                channel: None,
                arguments: None,
            },
        )
        .unwrap();

        assert_eq!(decision.effect, Effect::Allow);
        assert_eq!(decision.policy_id.as_deref(), Some("p1"));
    }

    // ========================================================================
    // Deny rules
    // ========================================================================

    #[test]
    fn deny_rule_blocks_specific_tool() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, resource_pattern, message, created_at)
             VALUES ('p-deny', 'no shell', 100, 'deny', 'agent:untrusted-*', 'tool:shell_*', 'Shell blocked', 1)",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, created_at)
             VALUES ('p-allow', 'allow tools', 10, 'allow', 'agent:*', 'call', 'tool:*', 1)",
            [],
        ).unwrap();

        let decision = evaluate(
            &conn,
            &PolicyRequest {
                actor: "agent:untrusted-bob",
                action: "call",
                resource: "tool:shell_exec",
                sql_content: None,
                channel: None,
                arguments: None,
            },
        )
        .unwrap();

        assert_eq!(decision.effect, Effect::Deny);
        assert_eq!(decision.reason.as_deref(), Some("Shell blocked"));
    }

    #[test]
    fn deny_rule_doesnt_block_other_agents() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, resource_pattern, message, created_at)
             VALUES ('p-deny', 'no shell', 100, 'deny', 'agent:untrusted-*', 'tool:shell_*', 'Shell blocked', 1)",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, created_at)
             VALUES ('p-allow', 'allow tools', 10, 'allow', 'agent:*', 'call', 'tool:*', 1)",
            [],
        ).unwrap();

        let decision = evaluate(
            &conn,
            &PolicyRequest {
                actor: "agent:trusted-alice",
                action: "call",
                resource: "tool:shell_exec",
                sql_content: None,
                channel: None,
                arguments: None,
            },
        )
        .unwrap();

        assert_eq!(decision.effect, Effect::Allow);
    }

    // ========================================================================
    // SQL pattern matching
    // ========================================================================

    #[test]
    fn deny_destructive_sql() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, action_pattern, resource_pattern, sql_pattern, message, created_at)
             VALUES ('block-drop', 'Block DROP', 100, 'deny', 'execute', 'sql:*', 'DROP|TRUNCATE', 'Destructive SQL blocked', 1)",
            [],
        ).unwrap();

        let decision = evaluate(
            &conn,
            &PolicyRequest {
                actor: "agent:main",
                action: "execute",
                resource: "sql:ddl",
                sql_content: Some("DROP TABLE users"),
                channel: None,
                arguments: None,
            },
        )
        .unwrap();

        assert_eq!(decision.effect, Effect::Deny);
        assert_eq!(decision.reason.as_deref(), Some("Destructive SQL blocked"));
    }

    #[test]
    fn safe_sql_not_blocked() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, action_pattern, resource_pattern, sql_pattern, message, created_at)
             VALUES ('block-drop', 'Block DROP', 100, 'deny', 'execute', 'sql:*', 'DROP|TRUNCATE', 'Blocked', 1)",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, action_pattern, resource_pattern, created_at)
             VALUES ('allow-sql', 'Allow SQL', 10, 'allow', 'execute', 'sql:*', 1)",
            [],
        ).unwrap();

        let decision = evaluate(
            &conn,
            &PolicyRequest {
                actor: "agent:main",
                action: "execute",
                resource: "sql:query",
                sql_content: Some("SELECT * FROM orders WHERE id = 1"),
                channel: None,
                arguments: None,
            },
        )
        .unwrap();

        assert_eq!(decision.effect, Effect::Allow);
    }

    // ========================================================================
    // Audit effect
    // ========================================================================

    #[test]
    fn audit_rule_logs_but_allows() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, action_pattern, channel_pattern, created_at)
             VALUES ('audit-pub', 'Audit public', 50, 'audit', 'call', 'slack:*', 1)",
            [],
        ).unwrap();

        let decision = evaluate(
            &conn,
            &PolicyRequest {
                actor: "agent:main",
                action: "call",
                resource: "tool:search",
                sql_content: None,
                channel: Some("slack:general"),
                arguments: None,
            },
        )
        .unwrap();

        assert_eq!(decision.effect, Effect::Audit);
    }

    // ========================================================================
    // Priority ordering
    // ========================================================================

    #[test]
    fn higher_priority_wins() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, resource_pattern, created_at)
             VALUES ('low', 'allow all', 10, 'allow', 'agent:*', 'tool:*', 1)",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, resource_pattern, message, created_at)
             VALUES ('high', 'deny shell', 100, 'deny', 'agent:*', 'tool:shell_*', 'Denied', 1)",
            [],
        ).unwrap();

        let decision = evaluate(
            &conn,
            &PolicyRequest {
                actor: "agent:main",
                action: "call",
                resource: "tool:shell_exec",
                sql_content: None,
                channel: None,
                arguments: None,
            },
        )
        .unwrap();

        assert_eq!(decision.effect, Effect::Deny);
        assert_eq!(decision.policy_id.as_deref(), Some("high"));
    }

    // ========================================================================
    // Audit trail
    // ========================================================================

    #[test]
    fn decisions_are_logged() {
        let conn = setup();

        evaluate_with_mode(
            &conn,
            &PolicyRequest {
                actor: "agent:main",
                action: "call",
                resource: "tool:shell_exec",
                sql_content: None,
                channel: None,
                arguments: None,
            },
            PolicyMode::DenyByDefault,
        )
        .unwrap();

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM policy_audit", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);

        let (actor, effect): (String, String) = conn
            .query_row("SELECT actor, effect FROM policy_audit LIMIT 1", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(actor, "agent:main");
        assert_eq!(effect, "denied");
    }

    #[test]
    fn multiple_evaluations_accumulate_audit() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, created_at)
             VALUES ('p1', 'allow all', 10, 'allow', 'agent:*', '*', '*', 1)",
            [],
        ).unwrap();

        for _ in 0..5 {
            evaluate(
                &conn,
                &PolicyRequest {
                    actor: "agent:main",
                    action: "call",
                    resource: "tool:test",
                    sql_content: None,
                    channel: None,
                    arguments: None,
                },
            )
            .unwrap();
        }

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM policy_audit", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 5);
    }

    // ========================================================================
    // Disabled policies
    // ========================================================================

    #[test]
    fn disabled_policy_is_skipped() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, resource_pattern, enabled, created_at)
             VALUES ('p1', 'allow all', 10, 'allow', 'agent:*', 'tool:*', 0, 1)",
            [],
        ).unwrap();

        let decision = evaluate_with_mode(
            &conn,
            &PolicyRequest {
                actor: "agent:main",
                action: "call",
                resource: "tool:test",
                sql_content: None,
                channel: None,
                arguments: None,
            },
            PolicyMode::DenyByDefault,
        )
        .unwrap();

        assert_eq!(
            decision.effect,
            Effect::Deny,
            "disabled rule should not match, fall through to deny-by-default"
        );
    }

    // ========================================================================
    // SQL filter generation
    // ========================================================================

    #[test]
    fn sql_filter_basic() {
        let conn = setup();
        let filter = generate_sql_filter(&conn, "agent-main").unwrap();
        assert!(filter.contains("agent_id = 'agent-main'"));
        assert!(filter.contains("superseded_at IS NULL"));
    }

    #[test]
    fn sql_filter_with_confidence_policy() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, resource_pattern, message, created_at)
             VALUES ('low-conf', 'Hide low confidence', 50, 'deny', 'fact:low_confidence', '0.3', 1)",
            [],
        ).unwrap();

        let filter = generate_sql_filter(&conn, "agent-main").unwrap();
        assert!(filter.contains("confidence >= 0.3"));
    }

    // ========================================================================
    // Behavioral rules
    // ========================================================================

    fn seed_tool_calls(conn: &Connection, tool: &str, args: &str, count: usize) {
        let sid = crate::store::log::create_session(conn, "a", None).unwrap();
        let mid = crate::store::log::append_message(conn, &sid, "assistant", "call").unwrap();
        for _ in 0..count {
            crate::store::log::record_tool_call(
                conn,
                &mid,
                &sid,
                tool,
                Some(args),
                Some("ok"),
                Some("success"),
                Some("allowed"),
                Some(10),
            )
            .unwrap();
        }
    }

    #[test]
    fn rate_limit_triggers_when_exceeded() {
        let conn = setup();
        seed_tool_calls(&conn, "shell_exec", "{}", 12);

        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, action_pattern, resource_pattern,
                                   rule_type, rule_config, message, created_at)
             VALUES ('rl', 'rate limit shells', 90, 'deny', 'call', 'tool:shell_*',
                     'rate_limit', '{\"max\": 10, \"window_seconds\": 300}',
                     'Rate limited', 1)",
            [],
        )
        .unwrap();

        let decision = evaluate(
            &conn,
            &PolicyRequest {
                actor: "a",
                action: "call",
                resource: "tool:shell_exec",
                sql_content: None,
                channel: None,
                arguments: None,
            },
        )
        .unwrap();

        assert_eq!(decision.effect, Effect::Deny);
        assert_eq!(decision.reason.as_deref(), Some("Rate limited"));
    }

    #[test]
    fn rate_limit_does_not_trigger_below_threshold() {
        let conn = setup();
        seed_tool_calls(&conn, "shell_exec", "{}", 5);

        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, action_pattern, resource_pattern,
                                   rule_type, rule_config, message, created_at)
             VALUES ('rl', 'rate limit shells', 90, 'deny', 'call', 'tool:shell_*',
                     'rate_limit', '{\"max\": 10, \"window_seconds\": 300}',
                     'Rate limited', 1)",
            [],
        )
        .unwrap();

        let decision = evaluate(
            &conn,
            &PolicyRequest {
                actor: "a",
                action: "call",
                resource: "tool:shell_exec",
                sql_content: None,
                channel: None,
                arguments: None,
            },
        )
        .unwrap();

        assert_ne!(
            decision.effect,
            Effect::Deny,
            "should not deny when under rate limit"
        );
    }

    #[test]
    fn retry_loop_triggers_on_repeated_calls() {
        let conn = setup();
        seed_tool_calls(&conn, "file_read", r#"{"path":"/tmp/x"}"#, 4);

        conn.execute(
            "INSERT INTO policies (id, name, priority, effect,
                                   rule_type, rule_config, message, created_at)
             VALUES ('retry', 'retry detect', 85, 'deny',
                     'retry_loop', '{\"same_tool_same_args\": 3, \"window_seconds\": 60}',
                     'Retry loop', 1)",
            [],
        )
        .unwrap();

        let decision = evaluate(
            &conn,
            &PolicyRequest {
                actor: "a",
                action: "call",
                resource: "tool:file_read",
                sql_content: None,
                channel: None,
                arguments: None,
            },
        )
        .unwrap();

        assert_eq!(decision.effect, Effect::Deny);
        assert_eq!(decision.reason.as_deref(), Some("Retry loop"));
    }

    #[test]
    fn retry_loop_does_not_trigger_with_varied_calls() {
        let conn = setup();
        let sid = crate::store::log::create_session(&conn, "a", None).unwrap();
        let mid = crate::store::log::append_message(&conn, &sid, "assistant", "call").unwrap();
        for i in 0..5 {
            crate::store::log::record_tool_call(
                &conn,
                &mid,
                &sid,
                "file_read",
                Some(&format!(r#"{{"path":"/tmp/{i}"}}"#)),
                Some("ok"),
                Some("success"),
                Some("allowed"),
                Some(10),
            )
            .unwrap();
        }

        conn.execute(
            "INSERT INTO policies (id, name, priority, effect,
                                   rule_type, rule_config, message, created_at)
             VALUES ('retry', 'retry detect', 85, 'deny',
                     'retry_loop', '{\"same_tool_same_args\": 3, \"window_seconds\": 60}',
                     'Retry loop', 1)",
            [],
        )
        .unwrap();

        let decision = evaluate(
            &conn,
            &PolicyRequest {
                actor: "a",
                action: "call",
                resource: "tool:file_read",
                sql_content: None,
                channel: None,
                arguments: None,
            },
        )
        .unwrap();

        assert_ne!(
            decision.effect,
            Effect::Deny,
            "varied args should not trigger retry loop"
        );
    }

    #[test]
    fn token_budget_triggers_when_exceeded() {
        let conn = setup();
        let sid = crate::store::log::create_session(&conn, "a", None).unwrap();
        let big_msg = "x".repeat(200_000); // 200K chars ≈ 50K tokens
        crate::store::log::append_message(&conn, &sid, "user", &big_msg).unwrap();

        conn.execute(
            "INSERT INTO policies (id, name, priority, effect,
                                   rule_type, rule_config, message, created_at)
             VALUES ('tb', 'token budget', 70, 'deny',
                     'token_budget', '{\"max_tokens_per_session\": 10000}',
                     'Budget exceeded', 1)",
            [],
        )
        .unwrap();

        let decision = evaluate(
            &conn,
            &PolicyRequest {
                actor: "a",
                action: "respond",
                resource: "conversation",
                sql_content: None,
                channel: None,
                arguments: None,
            },
        )
        .unwrap();

        assert_eq!(decision.effect, Effect::Deny);
        assert_eq!(decision.reason.as_deref(), Some("Budget exceeded"));
    }

    #[test]
    fn token_budget_does_not_trigger_under_limit() {
        let conn = setup();
        let sid = crate::store::log::create_session(&conn, "a", None).unwrap();
        crate::store::log::append_message(&conn, &sid, "user", "short message").unwrap();

        conn.execute(
            "INSERT INTO policies (id, name, priority, effect,
                                   rule_type, rule_config, message, created_at)
             VALUES ('tb', 'token budget', 70, 'deny',
                     'token_budget', '{\"max_tokens_per_session\": 500000}',
                     'Budget exceeded', 1)",
            [],
        )
        .unwrap();

        let decision = evaluate(
            &conn,
            &PolicyRequest {
                actor: "a",
                action: "respond",
                resource: "conversation",
                sql_content: None,
                channel: None,
                arguments: None,
            },
        )
        .unwrap();

        assert_ne!(
            decision.effect,
            Effect::Deny,
            "should not deny under token budget"
        );
    }

    #[test]
    fn time_window_triggers_during_matching_hours() {
        let conn = setup();
        let now_hour = chrono::Utc::now()
            .format("%H")
            .to_string()
            .parse::<u32>()
            .unwrap();

        conn.execute(
            &format!(
                "INSERT INTO policies (id, name, priority, effect,
                                       rule_type, rule_config, message, created_at)
                 VALUES ('tw', 'time window', 80, 'deny',
                         'time_window', '{{\"start_hour\": {}, \"end_hour\": {}}}',
                         'Not now', 1)",
                now_hour,
                now_hour + 1
            ),
            [],
        )
        .unwrap();

        let decision = evaluate(
            &conn,
            &PolicyRequest {
                actor: "a",
                action: "call",
                resource: "tool:test",
                sql_content: None,
                channel: None,
                arguments: None,
            },
        )
        .unwrap();

        assert_eq!(
            decision.effect,
            Effect::Deny,
            "should deny during matching window"
        );
    }

    #[test]
    fn time_window_does_not_trigger_outside_hours() {
        let conn = setup();
        let now_hour = chrono::Utc::now()
            .format("%H")
            .to_string()
            .parse::<u32>()
            .unwrap();
        let outside = (now_hour + 12) % 24;

        conn.execute(
            &format!(
                "INSERT INTO policies (id, name, priority, effect,
                                       rule_type, rule_config, message, created_at)
                 VALUES ('tw', 'time window', 80, 'deny',
                         'time_window', '{{\"start_hour\": {}, \"end_hour\": {}}}',
                         'Not now', 1)",
                outside,
                (outside + 1) % 24
            ),
            [],
        )
        .unwrap();

        let decision = evaluate(
            &conn,
            &PolicyRequest {
                actor: "a",
                action: "call",
                resource: "tool:test",
                sql_content: None,
                channel: None,
                arguments: None,
            },
        )
        .unwrap();

        assert_ne!(
            decision.effect,
            Effect::Deny,
            "should not deny outside the time window"
        );
    }

    #[test]
    fn behavioral_rule_skipped_when_condition_not_met() {
        let conn = setup();
        // Rate limit rule that won't trigger (no tool calls yet),
        // plus a lower-priority allow-all rule.
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, action_pattern, resource_pattern,
                                   rule_type, rule_config, message, created_at)
             VALUES ('rl', 'rate limit', 90, 'deny', 'call', 'tool:*',
                     'rate_limit', '{\"max\": 100, \"window_seconds\": 300}',
                     'Rate limited', 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, created_at)
             VALUES ('allow', 'allow all', 10, 'allow', '*', '*', '*', 1)",
            [],
        ).unwrap();

        let decision = evaluate_with_mode(
            &conn,
            &PolicyRequest {
                actor: "a",
                action: "call",
                resource: "tool:test",
                sql_content: None,
                channel: None,
                arguments: None,
            },
            PolicyMode::DenyByDefault,
        )
        .unwrap();

        assert_eq!(
            decision.effect,
            Effect::Allow,
            "behavioral rule should be skipped, falling through to allow-all"
        );
        assert_eq!(decision.policy_id.as_deref(), Some("allow"));
    }

    // ========================================================================
    // Argument pattern matching
    // ========================================================================

    #[test]
    fn argument_pattern_matches_url() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, action_pattern, resource_pattern, argument_pattern, message, created_at)
             VALUES ('url-allow', 'allow docs domain', 100, 'allow', 'ingest', 'knowledge:url', 'https://docs.example.com/*', 'Whitelisted', 1)",
            [],
        ).unwrap();

        let decision = evaluate(
            &conn,
            &PolicyRequest {
                actor: "agent:main",
                action: "ingest",
                resource: "knowledge:url",
                sql_content: None,
                channel: None,
                arguments: Some("https://docs.example.com/guide/intro.html"),
            },
        )
        .unwrap();

        assert_eq!(decision.effect, Effect::Allow);
        assert_eq!(decision.policy_id.as_deref(), Some("url-allow"));
    }

    #[test]
    fn argument_pattern_rejects_non_matching_url() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, action_pattern, resource_pattern, argument_pattern, message, created_at)
             VALUES ('url-allow', 'allow docs domain', 100, 'allow', 'ingest', 'knowledge:url', 'https://docs.example.com/*', 'Whitelisted', 1)",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, action_pattern, resource_pattern, message, created_at)
             VALUES ('url-deny', 'deny all URL ingest', 50, 'deny', 'ingest', 'knowledge:url', 'URL not whitelisted', 1)",
            [],
        ).unwrap();

        let decision = evaluate(
            &conn,
            &PolicyRequest {
                actor: "agent:main",
                action: "ingest",
                resource: "knowledge:url",
                sql_content: None,
                channel: None,
                arguments: Some("https://evil.example.org/payload"),
            },
        )
        .unwrap();

        assert_eq!(decision.effect, Effect::Deny);
        assert_eq!(decision.policy_id.as_deref(), Some("url-deny"));
        assert_eq!(decision.reason.as_deref(), Some("URL not whitelisted"));
    }

    #[test]
    fn argument_pattern_none_skips_policy_with_pattern() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, action_pattern, resource_pattern, argument_pattern, created_at)
             VALUES ('url-only', 'url specific', 100, 'deny', 'ingest', 'knowledge:*', 'https://blocked.com/*', 1)",
            [],
        ).unwrap();

        let decision = evaluate(
            &conn,
            &PolicyRequest {
                actor: "agent:main",
                action: "ingest",
                resource: "knowledge",
                sql_content: None,
                channel: None,
                arguments: None,
            },
        )
        .unwrap();

        assert_ne!(
            decision.effect,
            Effect::Deny,
            "policy with argument_pattern should be skipped when request has no arguments"
        );
    }

    #[test]
    fn url_whitelist_does_not_affect_file_ingest() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, action_pattern, resource_pattern, message, created_at)
             VALUES ('url-deny', 'deny all URL ingest', 100, 'deny', 'ingest', 'knowledge:url', 'URL blocked', 1)",
            [],
        ).unwrap();

        let decision = evaluate(
            &conn,
            &PolicyRequest {
                actor: "agent:main",
                action: "ingest",
                resource: "knowledge",
                sql_content: None,
                channel: None,
                arguments: None,
            },
        )
        .unwrap();

        assert_eq!(
            decision.effect,
            Effect::Allow,
            "file ingest (resource=knowledge) should not be affected by knowledge:url deny rule"
        );
    }

    #[test]
    fn multiple_url_whitelists_first_match_wins() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, action_pattern, resource_pattern, argument_pattern, created_at)
             VALUES ('allow-docs', 'allow docs', 100, 'allow', 'ingest', 'knowledge:url', 'https://docs.*', 1)",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, action_pattern, resource_pattern, argument_pattern, created_at)
             VALUES ('allow-wiki', 'allow wiki', 90, 'allow', 'ingest', 'knowledge:url', 'https://wiki.*', 1)",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, action_pattern, resource_pattern, message, created_at)
             VALUES ('deny-rest', 'deny rest', 10, 'deny', 'ingest', 'knowledge:url', 'Not whitelisted', 1)",
            [],
        ).unwrap();

        let d1 = evaluate(
            &conn,
            &PolicyRequest {
                actor: "a",
                action: "ingest",
                resource: "knowledge:url",
                sql_content: None,
                channel: None,
                arguments: Some("https://docs.rust-lang.org/book/"),
            },
        )
        .unwrap();
        assert_eq!(d1.effect, Effect::Allow);

        let d2 = evaluate(
            &conn,
            &PolicyRequest {
                actor: "a",
                action: "ingest",
                resource: "knowledge:url",
                sql_content: None,
                channel: None,
                arguments: Some("https://wiki.internal.co/page"),
            },
        )
        .unwrap();
        assert_eq!(d2.effect, Effect::Allow);

        let d3 = evaluate(
            &conn,
            &PolicyRequest {
                actor: "a",
                action: "ingest",
                resource: "knowledge:url",
                sql_content: None,
                channel: None,
                arguments: Some("https://malware.bad/exploit"),
            },
        )
        .unwrap();
        assert_eq!(d3.effect, Effect::Deny);
    }
}
