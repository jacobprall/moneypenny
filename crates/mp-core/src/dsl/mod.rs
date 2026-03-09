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
INGEST "url"
CREATE POLICY allow|deny|audit <action> ON <resource> [FOR AGENT "id"] [MESSAGE "reason"]
EVALUATE POLICY ON ("actor", "action", "resource")
CREATE JOB "name" SCHEDULE "cron" [TYPE type]
RUN|PAUSE|RESUME JOB "name"
CREATE AGENT "name" [CONFIG key=value]
SEARCH audit WHERE <filters> [| TAKE n]

Stores: facts, knowledge, log, audit
Filters: field = value, field > value, field LIKE "%pattern%", AND
Durations: 7d, 24h, 30m
Pipeline: chain stages with |
Multi-statement: separate with ;

Examples:
  SEARCH facts WHERE topic = "auth" AND confidence > 0.7 SINCE 7d | SORT confidence DESC | TAKE 10
  INSERT INTO facts ("Redis is preferred for caching", topic="infrastructure", confidence=0.9)
  DELETE FROM facts WHERE confidence < 0.3 AND BEFORE 30d
  CREATE POLICY deny DELETE ON facts FOR AGENT "junior-bot"
  SEARCH knowledge WHERE "deployment" | TAKE 5
  SEARCH facts | COUNT
  CREATE JOB "digest" SCHEDULE "0 9 * * *" TYPE prompt
  SEARCH audit WHERE action = "delete" SINCE 24h | TAKE 20"#;

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

    // 4. Policy check: pass the raw expression through existing sql_content path
    match crate::policy::evaluate(
        conn,
        &crate::policy::PolicyRequest {
            actor: &ctx.agent_id,
            action: "query",
            resource: "mpq",
            sql_content: Some(expression),
            channel: ctx.channel.as_deref(),
            arguments: None,
        },
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
