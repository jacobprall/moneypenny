use rusqlite::Connection;

/// Outcome of a single agent turn.
#[derive(Debug, Clone)]
pub struct TurnResult {
    pub response: String,
    pub tool_calls_made: usize,
    pub facts_extracted: usize,
    pub session_id: String,
    pub message_id: String,
}

/// Configuration for the agent loop.
#[derive(Debug, Clone)]
pub struct AgentConfig {
    pub agent_id: String,
    pub persona: Option<String>,
    pub token_budget: usize,
    pub max_tool_retries: usize,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            agent_id: "default".into(),
            persona: None,
            token_budget: 128_000,
            max_tool_retries: 3,
        }
    }
}

/// A pluggable LLM interface for the agent loop (sync, for testing).
/// In production, this would be async and use the mp-llm crate.
pub trait AgentLlm {
    fn generate(&self, messages: &[(String, String)]) -> anyhow::Result<String>;
}

/// Run a single agent turn: message → context → policy → LLM → tools → store → respond.
pub fn turn(
    conn: &Connection,
    config: &AgentConfig,
    session_id: &str,
    user_message: &str,
    llm: &dyn AgentLlm,
) -> anyhow::Result<TurnResult> {
    // 1. Store user message
    let _user_msg_id = crate::store::log::append_message(conn, session_id, "user", user_message)?;

    // 2. Assemble context
    let budget = crate::context::TokenBudget::new(config.token_budget);
    let segments = crate::context::assemble(
        conn, &config.agent_id, session_id,
        config.persona.as_deref(), user_message,
        &budget, None,
    )?;

    // 3. Build message list for LLM
    let mut llm_messages: Vec<(String, String)> = Vec::new();
    for seg in &segments {
        let role = match seg.label {
            "system_prompt" | "policies" => "system",
            "current_message" => "user",
            _ => "system",
        };
        llm_messages.push((role.to_string(), seg.content.clone()));
    }

    // 4. Policy check on the incoming message
    let msg_policy = crate::policy::PolicyRequest {
        actor: &config.agent_id,
        action: "respond",
        resource: "conversation",
        sql_content: Some(user_message),
        channel: None,
        arguments: None,
    };
    let msg_decision = crate::policy::evaluate(conn, &msg_policy)?;

    if matches!(msg_decision.effect, crate::policy::Effect::Deny) {
        let denial_msg = format!(
            "I'm unable to respond to that: {}",
            msg_decision.reason.as_deref().unwrap_or("blocked by policy")
        );
        let resp_id = crate::store::log::append_message(conn, session_id, "assistant", &denial_msg)?;
        return Ok(TurnResult {
            response: denial_msg,
            tool_calls_made: 0,
            facts_extracted: 0,
            session_id: session_id.to_string(),
            message_id: resp_id,
        });
    }

    // 5. Call LLM
    let llm_response = llm.generate(&llm_messages)?;

    // 6. Parse for tool calls (simple format: [TOOL:name](args))
    let (response_text, tool_calls) = parse_tool_calls(&llm_response);
    let mut tool_calls_made = 0;
    let mut tool_results = Vec::new();

    for (tool_name, tool_args) in &tool_calls {
        tool_calls_made += 1;
        let resp_id = crate::store::log::append_message(
            conn, session_id, "assistant",
            &format!("Calling tool: {tool_name}"),
        )?;

        let result = crate::tools::registry::execute(
            conn, &config.agent_id, session_id, &resp_id,
            tool_name, tool_args,
            &|name, args| crate::tools::builtins::dispatch(name, args),
            None,
        )?;

        tool_results.push((tool_name.clone(), result));
    }

    // 7. If tool calls were made, build final response incorporating tool results
    let final_response = if !tool_results.is_empty() {
        let mut augmented_messages = llm_messages.clone();
        augmented_messages.push(("assistant".into(), llm_response.clone()));
        for (name, result) in &tool_results {
            augmented_messages.push((
                "tool".into(),
                format!("[{name}]: {}", result.output),
            ));
        }
        llm.generate(&augmented_messages)?
    } else {
        response_text
    };

    // 8. Redact secrets from response
    let redacted = crate::store::redact::redact(&final_response);

    // 9. Store assistant response
    let resp_msg_id = crate::store::log::append_message(conn, session_id, "assistant", &redacted)?;

    Ok(TurnResult {
        response: redacted,
        tool_calls_made,
        facts_extracted: 0,
        session_id: session_id.to_string(),
        message_id: resp_msg_id,
    })
}

/// Parse simple tool call markers from LLM response.
/// Format: [TOOL:name](args_json)
fn parse_tool_calls(response: &str) -> (String, Vec<(String, String)>) {
    let mut calls = Vec::new();
    let mut clean = response.to_string();

    let tool_re = regex::Regex::new(r"\[TOOL:([^\]]+)\]\(([^)]*)\)").unwrap();
    for cap in tool_re.captures_iter(response) {
        let name = cap[1].to_string();
        let args = cap[2].to_string();
        calls.push((name, args));
    }

    clean = tool_re.replace_all(&clean, "").to_string();
    (clean.trim().to_string(), calls)
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db, schema, store};

    fn setup() -> Connection {
        let conn = db::open_memory().unwrap();
        schema::init_agent_db(&conn).unwrap();
        conn
    }

    fn allow_all(conn: &Connection) {
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, created_at)
             VALUES ('allow-all', 'allow-all', 0, 'allow', '*', '*', '*', 1)",
            [],
        ).unwrap();
    }

    struct MockLlm {
        response: String,
    }

    impl AgentLlm for MockLlm {
        fn generate(&self, _messages: &[(String, String)]) -> anyhow::Result<String> {
            Ok(self.response.clone())
        }
    }

    struct CountingLlm {
        responses: std::cell::RefCell<Vec<String>>,
    }

    impl AgentLlm for CountingLlm {
        fn generate(&self, _messages: &[(String, String)]) -> anyhow::Result<String> {
            let mut responses = self.responses.borrow_mut();
            if responses.is_empty() {
                Ok("no more responses".into())
            } else {
                Ok(responses.remove(0))
            }
        }
    }

    // ========================================================================
    // Basic turn
    // ========================================================================

    #[test]
    fn turn_stores_user_and_assistant_messages() {
        let conn = setup();
        allow_all(&conn);
        let sid = store::log::create_session(&conn, "a", None).unwrap();

        let llm = MockLlm { response: "Hello! How can I help?".into() };
        let config = AgentConfig { agent_id: "a".into(), ..Default::default() };

        let result = turn(&conn, &config, &sid, "Hello", &llm).unwrap();
        assert_eq!(result.response, "Hello! How can I help?");

        let msgs = store::log::get_messages(&conn, &sid).unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].role, "user");
        assert_eq!(msgs[0].content, "Hello");
        assert_eq!(msgs[1].role, "assistant");
        assert_eq!(msgs[1].content, "Hello! How can I help?");
    }

    #[test]
    fn turn_uses_persona_in_context() {
        let conn = setup();
        allow_all(&conn);
        let sid = store::log::create_session(&conn, "a", None).unwrap();

        struct CaptureLlm(std::cell::RefCell<Vec<(String, String)>>);
        impl AgentLlm for CaptureLlm {
            fn generate(&self, messages: &[(String, String)]) -> anyhow::Result<String> {
                *self.0.borrow_mut() = messages.to_vec();
                Ok("ok".into())
            }
        }

        let llm = CaptureLlm(std::cell::RefCell::new(vec![]));
        let config = AgentConfig {
            agent_id: "a".into(),
            persona: Some("You are a SQL expert.".into()),
            ..Default::default()
        };

        turn(&conn, &config, &sid, "Help me", &llm).unwrap();
        let captured = llm.0.borrow();
        let system_msgs: Vec<&String> = captured.iter()
            .filter(|(role, _)| role == "system")
            .map(|(_, content)| content)
            .collect();
        assert!(system_msgs.iter().any(|m| m.contains("SQL expert")));
    }

    // ========================================================================
    // Policy enforcement
    // ========================================================================

    #[test]
    fn turn_denies_by_policy() {
        let conn = setup();
        conn.execute(
            "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, message, created_at)
             VALUES ('deny-all', 'deny-all', 100, 'deny', '*', '*', '*', 'blocked by policy', 1)",
            [],
        ).unwrap();
        let sid = store::log::create_session(&conn, "a", None).unwrap();

        let llm = MockLlm { response: "shouldn't see this".into() };
        let config = AgentConfig { agent_id: "a".into(), ..Default::default() };

        let result = turn(&conn, &config, &sid, "do something bad", &llm).unwrap();
        assert!(result.response.contains("unable to respond"));
        assert_eq!(result.tool_calls_made, 0);
    }

    // ========================================================================
    // Tool calls
    // ========================================================================

    #[test]
    fn turn_handles_tool_calls() {
        let conn = setup();
        allow_all(&conn);
        crate::tools::registry::register_builtins(&conn).unwrap();
        let sid = store::log::create_session(&conn, "a", None).unwrap();

        let llm = CountingLlm {
            responses: std::cell::RefCell::new(vec![
                r#"Let me check. [TOOL:shell_exec]({"command":"echo hello"})"#.into(),
                "The command returned: hello".into(),
            ]),
        };
        let config = AgentConfig { agent_id: "a".into(), ..Default::default() };

        let result = turn(&conn, &config, &sid, "run echo hello", &llm).unwrap();
        assert_eq!(result.tool_calls_made, 1);
        assert!(result.response.contains("hello"));
    }

    #[test]
    fn turn_redacts_secrets_from_response() {
        let conn = setup();
        allow_all(&conn);
        let sid = store::log::create_session(&conn, "a", None).unwrap();

        let llm = MockLlm {
            response: "Here's the key: sk-abc123longapikey456789012345678901234567890".into(),
        };
        let config = AgentConfig { agent_id: "a".into(), ..Default::default() };

        let result = turn(&conn, &config, &sid, "what's the api key?", &llm).unwrap();
        assert!(result.response.contains("[REDACTED]"));
        assert!(!result.response.contains("sk-abc123"));
    }

    // ========================================================================
    // parse_tool_calls
    // ========================================================================

    #[test]
    fn parse_tool_calls_extracts_calls() {
        let (text, calls) = parse_tool_calls(
            r#"Let me check. [TOOL:shell_exec]({"command":"ls"}) Done."#,
        );
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "shell_exec");
        assert!(calls[0].1.contains("ls"));
        assert!(text.contains("Let me check"));
        assert!(text.contains("Done"));
    }

    #[test]
    fn parse_tool_calls_no_calls() {
        let (text, calls) = parse_tool_calls("Just a normal response");
        assert!(calls.is_empty());
        assert_eq!(text, "Just a normal response");
    }

    #[test]
    fn parse_tool_calls_multiple() {
        let (_, calls) = parse_tool_calls(
            r#"[TOOL:file_read]({"path":"/tmp/a"}) and [TOOL:shell_exec]({"command":"ls"})"#,
        );
        assert_eq!(calls.len(), 2);
    }

    // ========================================================================
    // Session continuity
    // ========================================================================

    #[test]
    fn multiple_turns_accumulate_messages() {
        let conn = setup();
        allow_all(&conn);
        let sid = store::log::create_session(&conn, "a", None).unwrap();
        let config = AgentConfig { agent_id: "a".into(), ..Default::default() };

        let llm = MockLlm { response: "response 1".into() };
        turn(&conn, &config, &sid, "msg 1", &llm).unwrap();

        let llm = MockLlm { response: "response 2".into() };
        turn(&conn, &config, &sid, "msg 2", &llm).unwrap();

        let msgs = store::log::get_messages(&conn, &sid).unwrap();
        assert_eq!(msgs.len(), 4); // 2 user + 2 assistant
    }
}
