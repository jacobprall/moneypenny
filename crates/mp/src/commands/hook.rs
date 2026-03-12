//! Hook command — Cursor hooks for audit + policy enforcement.

use anyhow::Result;

use crate::helpers::{open_agent_db, resolve_agent, truncate};

pub async fn run(ctx: &crate::context::CommandContext<'_>, event: &str, agent: Option<String>) -> Result<()> {
    let config = ctx.config;
    let ag = resolve_agent(config, agent.as_deref())?;
    let conn = open_agent_db(config, &ag.name)?;

    let input: serde_json::Value = {
        let mut buf = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut buf)?;
        serde_json::from_str(&buf).unwrap_or_else(|_| serde_json::json!({}))
    };

    let conversation_id = input["conversation_id"].as_str().unwrap_or("unknown");
    let generation_id = input["generation_id"].as_str().unwrap_or("unknown");

    match event {
        "sessionStart" => {
            let model = input["model"].as_str().unwrap_or("unknown");
            record_activity(
                &conn, &ag.name, event, "session_start", "session",
                &format!("model={model}"),
                conversation_id, generation_id, None,
            )?;

            let briefing_req = crate::helpers::op_request(
                &ag.name,
                "briefing.compose",
                serde_json::json!({}),
            );
            if let Ok(resp) = mp_core::operations::execute(&conn, &briefing_req) {
                if resp.ok {
                    if let Some(text) = resp.data["text"].as_str() {
                        if !text.is_empty() {
                            println!("{}", serde_json::json!({
                                "permission": "allow",
                                "agent_message": text,
                            }));
                            return Ok(());
                        }
                    }
                }
            }

            emit_hook_allow();
        }
        "sessionEnd" | "stop" => {
            let status = input["status"].as_str().unwrap_or("completed");
            record_activity(
                &conn, &ag.name, event, "session_end", "session",
                &format!("status={status}"),
                conversation_id, generation_id, None,
            )?;
            emit_hook_allow();
        }
        "postToolUse" => {
            let tool = input["tool_name"].as_str().unwrap_or("unknown");
            let duration = input["duration"].as_u64();
            record_activity(
                &conn, &ag.name, event, "tool_call", tool,
                "Tool completed",
                conversation_id, generation_id, duration,
            )?;
            emit_hook_allow();
        }
        "afterShellExecution" => {
            let command = input["command"].as_str().unwrap_or("");
            let duration = input["duration"].as_u64();
            record_activity(
                &conn, &ag.name, event, "shell_exec", "shell",
                truncate(command, 500),
                conversation_id, generation_id, duration,
            )?;
            emit_hook_allow();
        }
        "afterMCPExecution" => {
            let tool = input["tool_name"].as_str().unwrap_or("unknown");
            let duration = input["duration"].as_u64();
            record_activity(
                &conn, &ag.name, event, "mcp_call", tool,
                "MCP tool completed",
                conversation_id, generation_id, duration,
            )?;
            emit_hook_allow();
        }
        "afterFileEdit" => {
            let file_path = input["file_path"].as_str().unwrap_or("unknown");
            let edit_count = input["edits"].as_array().map(|a| a.len()).unwrap_or(0);
            record_activity(
                &conn, &ag.name, event, "file_edit", file_path,
                &format!("{edit_count} edit(s)"),
                conversation_id, generation_id, None,
            )?;
            emit_hook_allow();
        }

        "preToolUse" => {
            let tool = input["tool_name"].as_str().unwrap_or("unknown");
            let tool_res = mp_core::policy::resource::tool(tool);
            let decision = mp_core::policy::evaluate(
                &conn,
                &mp_core::policy::PolicyRequest {
                    actor: &ag.name,
                    action: "call",
                    resource: &tool_res,
                    sql_content: None,
                    channel: Some("cursor"),
                    arguments: None,
                },
            )?;
            record_policy_audit(
                &conn, &ag.name, "call", &tool_res,
                &decision, conversation_id, generation_id,
            )?;
            record_activity(
                &conn, &ag.name, event, "policy_check", &tool_res,
                &format!("{:?}", decision.effect),
                conversation_id, generation_id, None,
            )?;
            emit_hook_decision(&decision);
        }
        "beforeShellExecution" => {
            let command = input["command"].as_str().unwrap_or("");
            let decision = mp_core::policy::evaluate(
                &conn,
                &mp_core::policy::PolicyRequest {
                    actor: &ag.name,
                    action: "shell_exec",
                    resource: mp_core::policy::resource::SHELL,
                    sql_content: Some(command),
                    channel: Some("cursor"),
                    arguments: Some(command),
                },
            )?;
            record_policy_audit(
                &conn, &ag.name, "shell_exec", mp_core::policy::resource::SHELL,
                &decision, conversation_id, generation_id,
            )?;
            record_activity(
                &conn, &ag.name, event, "policy_check", mp_core::policy::resource::SHELL,
                &format!("{:?}: {}", decision.effect, truncate(command, 200)),
                conversation_id, generation_id, None,
            )?;
            emit_hook_decision(&decision);
        }
        "beforeMCPExecution" => {
            let tool = input["tool_name"].as_str().unwrap_or("unknown");
            let tool_res = mp_core::policy::resource::tool(tool);
            let decision = mp_core::policy::evaluate(
                &conn,
                &mp_core::policy::PolicyRequest {
                    actor: &ag.name,
                    action: "mcp_call",
                    resource: &tool_res,
                    sql_content: None,
                    channel: Some("cursor"),
                    arguments: None,
                },
            )?;
            record_policy_audit(
                &conn, &ag.name, "mcp_call", &tool_res,
                &decision, conversation_id, generation_id,
            )?;
            record_activity(
                &conn, &ag.name, event, "policy_check", &tool_res,
                &format!("{:?}", decision.effect),
                conversation_id, generation_id, None,
            )?;
            emit_hook_decision(&decision);
        }

        _ => {
            record_activity(
                &conn, &ag.name, event, "unknown", "unknown",
                "unhandled hook event",
                conversation_id, generation_id, None,
            )?;
            emit_hook_allow();
        }
    }

    Ok(())
}

fn record_activity(
    conn: &rusqlite::Connection,
    agent_id: &str,
    event: &str,
    action: &str,
    resource: &str,
    detail: &str,
    conversation_id: &str,
    generation_id: &str,
    duration_ms: Option<u64>,
) -> Result<()> {
    conn.execute(
        "INSERT INTO activity_log (id, agent_id, event, action, resource, detail, conversation_id, generation_id, duration_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        rusqlite::params![
            uuid::Uuid::new_v4().to_string(),
            agent_id,
            event,
            action,
            resource,
            detail,
            conversation_id,
            generation_id,
            duration_ms.map(|d| d as i64),
        ],
    )?;
    Ok(())
}

fn record_policy_audit(
    conn: &rusqlite::Connection,
    agent_id: &str,
    action: &str,
    resource: &str,
    decision: &mp_core::policy::PolicyDecision,
    conversation_id: &str,
    generation_id: &str,
) -> Result<()> {
    let now = chrono::Utc::now().timestamp();
    let effect_str = match decision.effect {
        mp_core::policy::Effect::Allow => "allow",
        mp_core::policy::Effect::Deny => "deny",
        mp_core::policy::Effect::Audit => "audit",
    };
    conn.execute(
        "INSERT INTO policy_audit (id, policy_id, actor, action, resource, effect, reason, correlation_id, session_id, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        rusqlite::params![
            uuid::Uuid::new_v4().to_string(),
            decision.policy_id,
            agent_id,
            action,
            resource,
            effect_str,
            decision.reason,
            generation_id,
            conversation_id,
            now,
        ],
    )?;
    Ok(())
}

fn emit_hook_allow() {
    println!("{}", serde_json::json!({ "permission": "allow" }));
}

fn emit_hook_decision(decision: &mp_core::policy::PolicyDecision) {
    match decision.effect {
        mp_core::policy::Effect::Deny => {
            let msg = decision.reason.as_deref().unwrap_or("Blocked by Moneypenny policy");
            println!("{}", serde_json::json!({
                "permission": "deny",
                "user_message": msg,
                "agent_message": format!("Policy denied this action: {msg}")
            }));
        }
        _ => {
            emit_hook_allow();
        }
    }
}
