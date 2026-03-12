//! E2E tests: agent loop with TestLlmProvider.
//!
//! Exercises the full agent turn flow (context assembly, tool execution, session
//! state) without a real LLM. Uses a mock provider that returns predefined
//! tool calls and text responses.

mod common;

use common::init_project;
use mp_core::config::Config;
use mp_core::store::log;
use mp_llm::test_provider::TestLlmProvider;
use std::path::Path;

fn load_config(config_path: &Path) -> Config {
    Config::load(config_path).expect("load config")
}

#[tokio::test]
async fn agent_turn_text_only() {
    let (_temp, config_path) = init_project().unwrap();
    let config = load_config(&config_path);
    let conn = mp::helpers::open_agent_db(&config, "main").unwrap();
    let session_id = log::create_session(&conn, "main", Some("e2e")).unwrap();

    let provider = TestLlmProvider::with_text("Hello! I'm the test assistant.");
    let req_ctx = mp::context::RequestContext {
        agent_id: "main",
        conn: &conn,
        session_id: &session_id,
        embed_provider: None,
        policy_mode: config.agents[0].policy_mode(),
        persona: config.agents[0].persona.as_deref(),
        worker_bus: None,
    };

    let response = mp::agent::agent_turn(&req_ctx, &provider, "Say hello")
        .await
        .unwrap();

    assert_eq!(response, "Hello! I'm the test assistant.");
    let messages = log::get_messages(&conn, &session_id).unwrap();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].role, "user");
    assert_eq!(messages[0].content, "Say hello");
    assert_eq!(messages[1].role, "assistant");
    assert!(messages[1].content.contains("test assistant"));
}

#[tokio::test]
async fn agent_turn_tool_call_fact_add_then_text() {
    let (_temp, config_path) = init_project().unwrap();
    let config = load_config(&config_path);
    let conn = mp::helpers::open_agent_db(&config, "main").unwrap();
    let session_id = log::create_session(&conn, "main", Some("e2e")).unwrap();

    let provider = TestLlmProvider::with_tool_then_text(
        "fact_add",
        serde_json::json!({
            "content": "User prefers dark mode for their IDE",
            "summary": "Dark mode preference",
            "pointer": "IDE dark mode"
        }),
        "I've remembered that you prefer dark mode.",
    );

    let req_ctx = mp::context::RequestContext {
        agent_id: "main",
        conn: &conn,
        session_id: &session_id,
        embed_provider: None,
        policy_mode: config.agents[0].policy_mode(),
        persona: config.agents[0].persona.as_deref(),
        worker_bus: None,
    };

    let response = mp::agent::agent_turn(
        &req_ctx,
        &provider,
        "Remember that I prefer dark mode for my IDE",
    )
    .await
    .unwrap();

    assert!(response.contains("dark mode"));
    let facts = mp_core::store::facts::list_active(&conn, "main").unwrap();
    assert!(!facts.is_empty());
    let found = facts
        .iter()
        .find(|f| f.content.contains("dark mode"))
        .expect("fact should be stored");
    assert!(found.pointer.contains("dark mode") || found.content.contains("dark mode"));
}

#[tokio::test]
async fn agent_turn_memory_search_then_text() {
    let (_temp, config_path) = init_project().unwrap();
    let config = load_config(&config_path);
    let conn = mp::helpers::open_agent_db(&config, "main").unwrap();
    let session_id = log::create_session(&conn, "main", Some("e2e")).unwrap();

    let provider = TestLlmProvider::with_tool_then_text(
        "memory_search",
        serde_json::json!({ "query": "Redis", "limit": 5 }),
        "I searched memory for 'Redis' but found nothing relevant.",
    );

    let req_ctx = mp::context::RequestContext {
        agent_id: "main",
        conn: &conn,
        session_id: &session_id,
        embed_provider: None,
        policy_mode: config.agents[0].policy_mode(),
        persona: config.agents[0].persona.as_deref(),
        worker_bus: None,
    };

    let response = mp::agent::agent_turn(&req_ctx, &provider, "What do you know about Redis?")
        .await
        .unwrap();

    assert!(response.contains("Redis") || response.contains("nothing"));
}
