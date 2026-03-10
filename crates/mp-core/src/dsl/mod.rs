pub mod ast;
pub mod execute;
pub mod lexer;
pub mod parser;
pub mod validate;

use rusqlite::Connection;
use serde_json::json;

use crate::operations::OperationResponse;
pub use ast::*;
pub use execute::ExecuteContext;
pub use parser::ParseError;

pub const TOOL_NAME: &str = "moneypenny.query";

pub const TOOL_DESCRIPTION: &str = r#"Moneypenny Query (MPQ). One tool for all Moneypenny operations.

SEARCH <store> [WHERE <filters>] [SINCE <duration>] [| SORT field ASC|DESC] [| TAKE n]
INSERT INTO facts ("content", key=value ...)
UPDATE facts SET key=value WHERE id = "id"
DELETE FROM facts WHERE <filters>
INGEST "content_or_file_uri" [AS "title"]
INGEST EVENTS "source" [FROM "file_path"]
EXEC "op.name" {json_args}
CREATE POLICY "name" allow|deny|audit <action> ON <resource> [FOR AGENT "id"] [MESSAGE "reason"]
EVALUATE POLICY ON ("actor", "action", "resource")
CREATE JOB "name" SCHEDULE "cron" [TYPE type]
RUN|PAUSE|RESUME JOB "name"
CREATE AGENT "name" [CONFIG key=value]
SEARCH audit WHERE <filters> [| TAKE n]

Stores: facts, knowledge, log, audit, activity
Filters: field = value, field > value, field LIKE "%pattern%", AND
Durations: 7d, 24h, 30m
Pipeline: chain stages with |
Multi-statement: separate with ;

EXEC calls any canonical operation by name:
  EXEC "ingest.events" {"source": "cursor"}
  EXEC "knowledge.ingest" {"content": "doc text", "title": "My Doc"}
  EXEC "ingest.status" {"limit": 5}
  EXEC "embedding.process" {}

Examples:
  SEARCH facts WHERE topic = "auth" AND confidence > 0.7 SINCE 7d | SORT confidence DESC | TAKE 10
  INSERT INTO facts ("Redis is preferred for caching", topic="infrastructure", confidence=0.9)
  DELETE FROM facts WHERE confidence < 0.3 AND BEFORE 30d
  CREATE POLICY "no-junior-deletes" deny DELETE ON facts FOR AGENT "junior-bot"
  SEARCH knowledge WHERE "deployment" | TAKE 5
  SEARCH facts | COUNT
  CREATE JOB "digest" SCHEDULE "0 9 * * *" TYPE prompt
  SEARCH audit WHERE action = "delete" SINCE 24h | TAKE 20
  SEARCH activity | TAKE 50
  INGEST EVENTS "cursor"
  INGEST EVENTS "cursor" FROM "/tmp/sessions.jsonl""#;

pub fn tool_definition() -> serde_json::Value {
    json!({
        "name": TOOL_NAME,
        "description": TOOL_DESCRIPTION,
        "inputSchema": {
            "type": "object",
            "properties": {
                "expression": {
                    "type": "string",
                    "description": "MPQ expression"
                },
                "dry_run": {
                    "type": "boolean",
                    "default": false,
                    "description": "Parse and policy-check without executing. Returns the execution plan."
                }
            },
            "required": ["expression"],
            "additionalProperties": false
        }
    })
}

/// Full pipeline: lex → parse → validate → execute.
pub fn run(
    conn: &Connection,
    expression: &str,
    dry_run: bool,
    ctx: &ExecuteContext,
) -> OperationResponse {
    // 1. Lex
    let tokens = match lexer::lex(expression) {
        Ok(t) => t,
        Err(e) => return lex_error_response(e),
    };

    // 2. Parse
    let program = match parser::parse(tokens, expression) {
        Ok(p) => p,
        Err(e) => return parse_error_response(e),
    };

    // 3. Validate
    let validated = match validate::validate(program) {
        Ok(v) => v,
        Err(e) => return parse_error_response(e),
    };

    // 4. Top-level policy check: coarse gate on the full expression.
    //    Per-statement checks (verb-specific action/resource) happen in execute.rs.
    let audit_ctx = crate::policy::PolicyAuditContext {
        session_id: ctx.session_id.as_deref(),
        correlation_id: ctx.trace_id.as_deref(),
        idempotency_key: None,
        idempotency_state: None,
    };
    match crate::policy::evaluate_with_audit(
        conn,
        &crate::policy::PolicyRequest {
            actor: &ctx.agent_id,
            action: "query",
            resource: crate::policy::resource::MPQ,
            sql_content: Some(expression),
            channel: ctx.channel.as_deref(),
            arguments: None,
        },
        &audit_ctx,
    ) {
        Ok(decision) => {
            if matches!(decision.effect, crate::policy::Effect::Deny) {
                return OperationResponse {
                    ok: false,
                    code: "policy_denied".into(),
                    message: decision
                        .reason
                        .unwrap_or_else(|| "policy denied this expression".into()),
                    data: json!({
                        "policy_id": decision.policy_id,
                    }),
                    policy: Some(crate::operations::PolicyMeta {
                        effect: "deny".into(),
                        policy_id: decision.policy_id,
                        reason: None,
                    }),
                    audit: crate::operations::AuditMeta { recorded: true },
                };
            }
        }
        Err(_) => {
            // Policy evaluation failure is non-fatal in allow-by-default mode
        }
    }

    // 5. Dry run: return the parsed plan without executing
    if dry_run {
        let plan: Vec<serde_json::Value> = validated
            .program
            .statements
            .iter()
            .map(|s| {
                json!({
                    "raw": s.raw,
                    "head": format!("{:?}", s.head),
                    "pipeline_stages": s.pipeline.len(),
                })
            })
            .collect();

        return OperationResponse {
            ok: true,
            code: "dry_run".into(),
            message: format!(
                "parsed {} statement(s), ready to execute",
                validated.program.statements.len()
            ),
            data: json!({
                "plan": plan,
                "defaults_applied": validated.applied_defaults,
            }),
            policy: None,
            audit: crate::operations::AuditMeta { recorded: false },
        };
    }

    // 6. Execute
    match execute::execute_program(conn, &validated.program, ctx) {
        Ok(result) => result.response,
        Err(e) => OperationResponse {
            ok: false,
            code: "execution_error".into(),
            message: format!("{e}"),
            data: json!({"error": format!("{e}")}),
            policy: None,
            audit: crate::operations::AuditMeta { recorded: true },
        },
    }
}

fn lex_error_response(e: lexer::LexError) -> OperationResponse {
    OperationResponse {
        ok: false,
        code: "parse_error".into(),
        message: format!("lex error at position {}: {}", e.pos, e.message),
        data: json!({
            "position": e.pos,
            "expected": [],
            "got": e.message,
            "hint": "check for unterminated strings or invalid characters"
        }),
        policy: None,
        audit: crate::operations::AuditMeta { recorded: false },
    }
}

fn parse_error_response(e: ParseError) -> OperationResponse {
    OperationResponse {
        ok: false,
        code: "parse_error".into(),
        message: format!(
            "parse error at position {}: expected {}, got {}",
            e.position,
            e.expected.join(" or "),
            e.got
        ),
        data: json!({
            "position": e.position,
            "expected": e.expected,
            "got": e.got,
            "hint": e.hint,
        }),
        policy: None,
        audit: crate::operations::AuditMeta { recorded: false },
    }
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

    fn ctx(agent: &str) -> ExecuteContext {
        ExecuteContext {
            agent_id: agent.to_string(),
            channel: None,
            session_id: Some("test-session".into()),
            trace_id: Some("test-trace".into()),
        }
    }

    // ── Top-level MPQ policy: sql_pattern on full expression ──

    #[test]
    fn top_level_deny_via_sql_pattern() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, action_pattern, resource_pattern, sql_pattern, message, created_at)
             VALUES ('block-delete', 'Block DELETE via MPQ', 100, 'deny', 'query', 'mpq', 'DELETE', 'DELETE operations blocked', 1)",
            [],
        ).unwrap();

        let resp = run(&conn, "DELETE FROM facts WHERE id = \"abc\"", false, &ctx("agent:junior"));
        assert!(!resp.ok);
        assert_eq!(resp.code, "policy_denied");
        assert!(resp.message.contains("DELETE operations blocked"));
    }

    #[test]
    fn top_level_allows_when_no_deny_pattern() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, action_pattern, resource_pattern, sql_pattern, message, created_at)
             VALUES ('block-delete', 'Block DELETE via MPQ', 100, 'deny', 'query', 'mpq', 'DELETE', 'No deletes', 1)",
            [],
        ).unwrap();

        let resp = run(&conn, "SEARCH facts | TAKE 5", false, &ctx("agent:junior"));
        assert!(resp.ok, "SEARCH should not be blocked by DELETE sql_pattern");
    }

    // ── Per-statement policy: verb-level action/resource matching ──

    #[test]
    fn per_statement_deny_delete_on_fact() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, action_pattern, resource_pattern, actor_pattern, message, created_at)
             VALUES ('no-delete', 'No fact deletion for junior', 100, 'deny', 'delete', 'fact', 'agent:junior', 'Junior agents cannot delete facts', 1)",
            [],
        ).unwrap();

        let resp = run(&conn, "DELETE FROM facts WHERE id = \"abc\"", false, &ctx("agent:junior"));
        assert!(!resp.ok);
        assert!(resp.message.contains("Junior agents cannot delete"));
    }

    #[test]
    fn per_statement_deny_does_not_affect_other_agents() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, action_pattern, resource_pattern, actor_pattern, message, created_at)
             VALUES ('no-delete', 'No fact deletion for junior', 100, 'deny', 'delete', 'fact', 'agent:junior', 'Juniors cannot delete', 1)",
            [],
        ).unwrap();

        // Senior agent should not be blocked
        let resp = run(&conn, "DELETE FROM facts WHERE id = \"abc\"", false, &ctx("agent:senior"));
        // This will fail at execution (no fact with that id), but should NOT be policy_denied
        assert_ne!(resp.code, "policy_denied");
    }

    #[test]
    fn per_statement_deny_search_for_specific_agent() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, action_pattern, resource_pattern, actor_pattern, message, created_at)
             VALUES ('no-audit-search', 'Block audit search', 100, 'deny', 'search', 'audit', 'agent:restricted', 'Cannot search audit log', 1)",
            [],
        ).unwrap();

        let resp = run(&conn, "SEARCH audit SINCE 7d", false, &ctx("agent:restricted"));
        assert!(!resp.ok);
        assert!(resp.message.contains("Cannot search audit"));
    }

    #[test]
    fn per_statement_deny_policy_creation() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, action_pattern, resource_pattern, message, created_at)
             VALUES ('no-policy-create', 'Block policy creation', 100, 'deny', 'create', 'policy', 'Policy creation blocked', 1)",
            [],
        ).unwrap();

        let resp = run(
            &conn,
            r#"CREATE POLICY "allow-search" allow search ON facts"#,
            false,
            &ctx("agent:any"),
        );
        assert!(!resp.ok);
        assert!(resp.message.contains("Policy creation blocked"));
    }

    // ── Multi-statement: first statement blocked, second never runs ──

    #[test]
    fn multi_statement_stops_on_policy_deny() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, action_pattern, resource_pattern, actor_pattern, message, created_at)
             VALUES ('no-delete', 'No deletes', 100, 'deny', 'delete', 'fact', '*', 'All deletes blocked', 1)",
            [],
        ).unwrap();

        let resp = run(
            &conn,
            "DELETE FROM facts WHERE id = \"x\"; SEARCH facts | TAKE 5",
            false,
            &ctx("agent:any"),
        );
        assert!(!resp.ok);
        assert!(resp.message.contains("All deletes blocked"));
    }

    // ── Per-statement sql_content carries individual statement text ──

    #[test]
    fn per_statement_sql_pattern_on_individual_statement() {
        let conn = setup();
        // sql_pattern that only matches "confidence" — should block updates touching confidence
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, action_pattern, resource_pattern, sql_pattern, message, created_at)
             VALUES ('no-conf-update', 'Block confidence tampering', 100, 'deny', 'update', 'fact', 'confidence', 'Cannot modify confidence directly', 1)",
            [],
        ).unwrap();

        let resp = run(
            &conn,
            r#"UPDATE facts SET confidence = 1.0 WHERE id = "abc""#,
            false,
            &ctx("agent:any"),
        );
        assert!(!resp.ok);
        assert!(resp.message.contains("Cannot modify confidence"));
    }

    // ── Audit trail records policy decisions for MPQ ──

    #[test]
    fn policy_decisions_are_audited() {
        let conn = setup();

        run(&conn, "SEARCH facts | TAKE 5", false, &ctx("agent:test"));

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM policy_audit WHERE action = 'query' AND resource = 'mpq'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(count >= 1, "top-level MPQ policy decision should be audited");

        let per_stmt_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM policy_audit WHERE action = 'search' AND resource = 'memory'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(per_stmt_count >= 1, "per-statement policy decision should be audited");
    }

    #[test]
    fn exec_calls_canonical_operation() {
        let conn = setup();
        let resp = run(
            &conn,
            r#"EXEC "job.list" {}"#,
            false,
            &ctx("agent:test"),
        );
        assert!(resp.ok, "EXEC job.list should succeed: {}", resp.message);
    }

    #[test]
    fn exec_unknown_op_fails_gracefully() {
        let conn = setup();
        let resp = run(
            &conn,
            r#"EXEC "nonexistent.op" {}"#,
            false,
            &ctx("agent:test"),
        );
        assert!(!resp.ok);
        assert!(resp.message.contains("unknown operation") || resp.data.to_string().contains("unknown"));
    }

    #[test]
    fn exec_knowledge_ingest_with_content() {
        let conn = setup();
        let resp = run(
            &conn,
            r##"EXEC "knowledge.ingest" {"content": "Test doc content.", "title": "Test"}"##,
            false,
            &ctx("agent:test"),
        );
        assert!(resp.ok, "EXEC knowledge.ingest should succeed: {}", resp.message);
    }

    #[test]
    fn exec_dry_run_returns_plan() {
        let conn = setup();
        let resp = run(
            &conn,
            r#"EXEC "job.list" {}"#,
            true,
            &ctx("agent:test"),
        );
        assert!(resp.ok);
        assert_eq!(resp.code, "dry_run");
    }

    #[test]
    fn exec_policy_deny() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, action_pattern, resource_pattern, message, created_at)
             VALUES ('block-exec', 'Block all EXEC', 100, 'deny', 'exec', 'operation', 'EXEC blocked', 1)",
            [],
        ).unwrap();

        let resp = run(
            &conn,
            r#"EXEC "job.list" {}"#,
            false,
            &ctx("agent:test"),
        );
        assert!(!resp.ok);
        assert_eq!(resp.code, "policy_denied");
    }

    #[test]
    fn ingest_inline_content() {
        let conn = setup();
        let resp = run(
            &conn,
            r##"INGEST "Hello World, this is inline content." AS "inline-doc""##,
            false,
            &ctx("agent:test"),
        );
        assert!(resp.ok, "INGEST inline should succeed: {}", resp.message);
    }

    #[test]
    fn ingest_http_url_routes_to_core_ingest() {
        let conn = setup();
        let resp = run(
            &conn,
            r#"INGEST "http://127.0.0.1:9/doc""#,
            false,
            &ctx("agent:test"),
        );
        assert!(!resp.ok);
        assert_eq!(resp.code, "execution_error");
        assert!(!resp.message.contains("not supported"));
    }

    #[test]
    fn audit_includes_session_and_trace() {
        let conn = setup();

        let c = ExecuteContext {
            agent_id: "agent:traced".into(),
            channel: Some("slack:general".into()),
            session_id: Some("sess-42".into()),
            trace_id: Some("trace-99".into()),
        };
        run(&conn, "SEARCH facts | TAKE 1", false, &c);

        let (session, correlation): (Option<String>, Option<String>) = conn
            .query_row(
                "SELECT session_id, correlation_id FROM policy_audit WHERE actor = 'agent:traced' LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();

        assert_eq!(session.as_deref(), Some("sess-42"));
        assert_eq!(correlation.as_deref(), Some("trace-99"));
    }

    // ── Dry run still checks top-level policy ──

    #[test]
    fn dry_run_enforces_top_level_policy() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, action_pattern, resource_pattern, sql_pattern, message, created_at)
             VALUES ('block-all', 'Block everything', 100, 'deny', 'query', 'mpq', '.*', 'All queries blocked', 1)",
            [],
        ).unwrap();

        let resp = run(&conn, "SEARCH facts | TAKE 5", true, &ctx("agent:any"));
        assert!(!resp.ok);
        assert_eq!(resp.code, "policy_denied");
    }
}
