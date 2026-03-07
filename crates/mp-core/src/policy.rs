use rusqlite::{Connection, params};
use uuid::Uuid;

/// Result of a policy evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    Allow,
    Deny,
    Audit,
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
}

/// Evaluate a policy request against the policies table.
/// Deny-by-default: if no rule matches, the request is denied.
pub fn evaluate(conn: &Connection, req: &PolicyRequest) -> anyhow::Result<PolicyDecision> {
    let mut stmt = conn.prepare(
        "SELECT id, effect, actor_pattern, action_pattern, resource_pattern,
                sql_pattern, channel_pattern, message
         FROM policies
         WHERE enabled = 1
         ORDER BY priority DESC"
    )?;

    let policies = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,      // id
            r.get::<_, String>(1)?,      // effect
            r.get::<_, Option<String>>(2)?, // actor_pattern
            r.get::<_, Option<String>>(3)?, // action_pattern
            r.get::<_, Option<String>>(4)?, // resource_pattern
            r.get::<_, Option<String>>(5)?, // sql_pattern
            r.get::<_, Option<String>>(6)?, // channel_pattern
            r.get::<_, Option<String>>(7)?, // message
        ))
    })?.collect::<Result<Vec<_>, _>>()?;

    for (id, effect, actor_pat, action_pat, resource_pat, sql_pat, channel_pat, message) in &policies {
        if !matches_pattern(actor_pat.as_deref(), req.actor) {
            continue;
        }
        if !matches_pattern(action_pat.as_deref(), req.action) {
            continue;
        }
        if !matches_pattern(resource_pat.as_deref(), req.resource) {
            continue;
        }
        if let Some(sql_re) = sql_pat {
            if let Some(sql) = req.sql_content {
                if !regex_matches(sql_re, sql) {
                    continue;
                }
            } else {
                continue;
            }
        }
        if !matches_pattern(channel_pat.as_deref(), req.channel.unwrap_or("")) {
            if channel_pat.is_some() {
                continue;
            }
        }

        let eff = match effect.as_str() {
            "allow" => Effect::Allow,
            "deny" => Effect::Deny,
            "audit" => Effect::Audit,
            _ => continue,
        };

        let decision = PolicyDecision {
            effect: eff,
            policy_id: Some(id.clone()),
            reason: message.clone(),
        };

        log_decision(conn, &decision, req)?;
        return Ok(decision);
    }

    // Deny-by-default
    let decision = PolicyDecision {
        effect: Effect::Deny,
        policy_id: None,
        reason: Some("No matching policy rule. Deny-by-default.".into()),
    };
    log_decision(conn, &decision, req)?;
    Ok(decision)
}

/// Generate a SQL WHERE clause fragment from policies for data-level filtering.
/// For example, fact scope filtering based on agent identity.
pub fn generate_sql_filter(conn: &Connection, agent_id: &str) -> anyhow::Result<String> {
    // Base filter: agent sees its own private facts + all shared facts
    let mut clauses: Vec<String> = vec![
        format!("(agent_id = '{agent_id}')"),
    ];

    // Check for confidence threshold policies
    let threshold: Option<f64> = conn.query_row(
        "SELECT CAST(message AS REAL) FROM policies
         WHERE enabled = 1 AND effect = 'deny' AND resource_pattern = 'fact:low_confidence'
         ORDER BY priority DESC LIMIT 1",
        [],
        |r| r.get(0),
    ).ok();

    if let Some(min_confidence) = threshold {
        clauses.push(format!("confidence >= {min_confidence}"));
    }

    clauses.push("superseded_at IS NULL".into());

    Ok(clauses.join(" AND "))
}

/// Log a policy decision to the audit trail.
fn log_decision(conn: &Connection, decision: &PolicyDecision, req: &PolicyRequest) -> anyhow::Result<()> {
    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp();
    let effect_str = match decision.effect {
        Effect::Allow => "allowed",
        Effect::Deny => "denied",
        Effect::Audit => "audited",
    };

    conn.execute(
        "INSERT INTO policy_audit (id, policy_id, actor, action, resource, effect, reason, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![id, decision.policy_id, req.actor, req.action, req.resource, effect_str, decision.reason, now],
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
    // Deny-by-default
    // ========================================================================

    #[test]
    fn deny_by_default_when_no_policies() {
        let conn = setup();
        let decision = evaluate(&conn, &PolicyRequest {
            actor: "agent:main",
            action: "call",
            resource: "tool:shell_exec",
            sql_content: None,
            channel: None,
        }).unwrap();

        assert_eq!(decision.effect, Effect::Deny);
        assert!(decision.policy_id.is_none());
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

        let decision = evaluate(&conn, &PolicyRequest {
            actor: "agent:main",
            action: "call",
            resource: "tool:http_get",
            sql_content: None,
            channel: None,
        }).unwrap();

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

        let decision = evaluate(&conn, &PolicyRequest {
            actor: "agent:untrusted-bob",
            action: "call",
            resource: "tool:shell_exec",
            sql_content: None,
            channel: None,
        }).unwrap();

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

        let decision = evaluate(&conn, &PolicyRequest {
            actor: "agent:trusted-alice",
            action: "call",
            resource: "tool:shell_exec",
            sql_content: None,
            channel: None,
        }).unwrap();

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

        let decision = evaluate(&conn, &PolicyRequest {
            actor: "agent:main",
            action: "execute",
            resource: "sql:ddl",
            sql_content: Some("DROP TABLE users"),
            channel: None,
        }).unwrap();

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

        let decision = evaluate(&conn, &PolicyRequest {
            actor: "agent:main",
            action: "execute",
            resource: "sql:query",
            sql_content: Some("SELECT * FROM orders WHERE id = 1"),
            channel: None,
        }).unwrap();

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

        let decision = evaluate(&conn, &PolicyRequest {
            actor: "agent:main",
            action: "call",
            resource: "tool:search",
            sql_content: None,
            channel: Some("slack:general"),
        }).unwrap();

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

        let decision = evaluate(&conn, &PolicyRequest {
            actor: "agent:main",
            action: "call",
            resource: "tool:shell_exec",
            sql_content: None,
            channel: None,
        }).unwrap();

        assert_eq!(decision.effect, Effect::Deny);
        assert_eq!(decision.policy_id.as_deref(), Some("high"));
    }

    // ========================================================================
    // Audit trail
    // ========================================================================

    #[test]
    fn decisions_are_logged() {
        let conn = setup();

        evaluate(&conn, &PolicyRequest {
            actor: "agent:main",
            action: "call",
            resource: "tool:shell_exec",
            sql_content: None,
            channel: None,
        }).unwrap();

        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM policy_audit",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(count, 1);

        let (actor, effect): (String, String) = conn.query_row(
            "SELECT actor, effect FROM policy_audit LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        ).unwrap();
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
            evaluate(&conn, &PolicyRequest {
                actor: "agent:main",
                action: "call",
                resource: "tool:test",
                sql_content: None,
                channel: None,
            }).unwrap();
        }

        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM policy_audit", [], |r| r.get(0),
        ).unwrap();
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

        let decision = evaluate(&conn, &PolicyRequest {
            actor: "agent:main",
            action: "call",
            resource: "tool:test",
            sql_content: None,
            channel: None,
        }).unwrap();

        assert_eq!(decision.effect, Effect::Deny, "disabled rule should not match, fall through to deny-by-default");
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
}
