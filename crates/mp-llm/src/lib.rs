pub mod types;
pub mod provider;
pub mod http;
pub mod sqlite_ai;

use provider::LlmProvider;

/// Build an LlmProvider from configuration values.
pub fn build_provider(
    provider_type: &str,
    api_base: Option<&str>,
    api_key: Option<&str>,
    model: Option<&str>,
    embedding_model: Option<&str>,
) -> anyhow::Result<Box<dyn LlmProvider>> {
    match provider_type {
        "http" => Ok(Box::new(http::HttpProvider::from_config(
            api_base, api_key, model, embedding_model,
        ))),
        "local" => {
            let model_path = model
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| std::path::PathBuf::from("models/default.gguf"));
            let embed_path = embedding_model.map(std::path::PathBuf::from);
            Ok(Box::new(sqlite_ai::SqliteAiProvider::new(model_path, embed_path)))
        }
        other => anyhow::bail!("Unknown LLM provider: {other}. Use 'http' or 'local'."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;

    // ========================================================================
    // Type construction tests
    // ========================================================================

    #[test]
    fn message_constructors() {
        let sys = Message::system("You are helpful.");
        assert_eq!(sys.role, Role::System);
        assert_eq!(sys.content, "You are helpful.");
        assert!(sys.tool_call_id.is_none());

        let user = Message::user("Hello");
        assert_eq!(user.role, Role::User);

        let asst = Message::assistant("Hi there");
        assert_eq!(asst.role, Role::Assistant);

        let tool = Message::tool("result", "call_123");
        assert_eq!(tool.role, Role::Tool);
        assert_eq!(tool.tool_call_id.as_deref(), Some("call_123"));
    }

    #[test]
    fn message_serializes_to_openai_format() {
        let msg = Message::user("Hello");
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["role"], "user");
        assert_eq!(json["content"], "Hello");
        assert!(json.get("tool_call_id").is_none());
    }

    #[test]
    fn tool_message_includes_tool_call_id() {
        let msg = Message::tool("42", "call_abc");
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["tool_call_id"], "call_abc");
    }

    #[test]
    fn tool_def_serialization() {
        let tool = ToolDef {
            name: "get_weather".into(),
            description: "Get current weather".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "city": { "type": "string" }
                },
                "required": ["city"]
            }),
        };
        let json = serde_json::to_value(&tool).unwrap();
        assert_eq!(json["name"], "get_weather");
        assert_eq!(json["parameters"]["properties"]["city"]["type"], "string");
    }

    #[test]
    fn generate_config_defaults() {
        let cfg = GenerateConfig::default();
        assert_eq!(cfg.temperature, Some(0.7));
        assert!(cfg.max_tokens.is_none());
        assert!(cfg.stop.is_empty());
    }

    #[test]
    fn tool_call_round_trip() {
        let tc = ToolCall {
            id: "call_1".into(),
            name: "get_weather".into(),
            arguments: r#"{"city":"SF"}"#.into(),
        };
        let json = serde_json::to_string(&tc).unwrap();
        let parsed: ToolCall = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, "call_1");
        assert_eq!(parsed.name, "get_weather");
        assert_eq!(parsed.arguments, r#"{"city":"SF"}"#);
    }

    #[test]
    fn usage_defaults_to_zero() {
        let u = Usage::default();
        assert_eq!(u.prompt_tokens, 0);
        assert_eq!(u.completion_tokens, 0);
        assert_eq!(u.total_tokens, 0);
    }

    // ========================================================================
    // Provider trait tests (object safety, dispatch)
    // ========================================================================

    #[test]
    fn trait_is_object_safe() {
        // This compiles only if LlmProvider is object-safe.
        fn _accepts_boxed(_p: Box<dyn provider::LlmProvider>) {}
    }

    #[test]
    fn http_provider_metadata() {
        let p = http::HttpProvider::from_config(None, None, None, None);
        assert_eq!(p.name(), "http");
        assert!(p.supports_streaming());
    }

    #[test]
    fn sqlite_ai_provider_metadata() {
        let p = sqlite_ai::SqliteAiProvider::new("/tmp/model.gguf".into(), None);
        assert_eq!(p.name(), "sqlite-ai");
        assert!(!p.supports_streaming());
    }

    // ========================================================================
    // Provider factory tests
    // ========================================================================

    #[test]
    fn build_http_provider() {
        let p = build_provider("http", None, None, None, None).unwrap();
        assert_eq!(p.name(), "http");
        assert!(p.supports_streaming());
    }

    #[test]
    fn build_local_provider() {
        let p = build_provider("local", None, None, None, None).unwrap();
        assert_eq!(p.name(), "sqlite-ai");
        assert!(!p.supports_streaming());
    }

    #[test]
    fn build_unknown_provider_fails() {
        let result = build_provider("magic", None, None, None, None);
        match result {
            Ok(_) => panic!("expected error for unknown provider"),
            Err(e) => assert!(e.to_string().contains("Unknown LLM provider: magic")),
        }
    }

    #[test]
    fn build_http_with_custom_config() {
        let p = build_provider(
            "http",
            Some("http://localhost:11434/v1"),
            Some("sk-test"),
            Some("llama3"),
            Some("nomic-embed"),
        ).unwrap();
        assert_eq!(p.name(), "http");
    }

    // ========================================================================
    // SqliteAi generate/embed fail gracefully (not yet implemented)
    // ========================================================================

    #[tokio::test]
    async fn sqlite_ai_generate_returns_error() {
        let p = sqlite_ai::SqliteAiProvider::new("/tmp/model.gguf".into(), None);
        let result = p.generate(
            &[Message::user("hello")],
            &[],
            &GenerateConfig::default(),
        ).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not yet implemented"));
    }

    #[tokio::test]
    async fn sqlite_ai_embed_returns_error() {
        let p = sqlite_ai::SqliteAiProvider::new("/tmp/model.gguf".into(), None);
        let result = p.embed("hello").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not yet implemented"));
    }

    // ========================================================================
    // HTTP provider: request building (unit-testable without network)
    // ========================================================================

    #[test]
    fn chat_request_structure() {
        let body = http::tests::build_chat_request_test(
            "gpt-4o",
            &[Message::system("You are helpful"), Message::user("Hi")],
            &[],
            &GenerateConfig { max_tokens: Some(100), temperature: Some(0.5), stop: vec![] },
            false,
        );
        assert_eq!(body["model"], "gpt-4o");
        assert_eq!(body["stream"], false);
        assert_eq!(body["messages"].as_array().unwrap().len(), 2);
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][1]["role"], "user");
        assert_eq!(body["temperature"], 0.5);
        assert_eq!(body["max_tokens"], 100);
        assert!(body.get("tools").is_none());
    }

    #[test]
    fn chat_request_with_tools() {
        let tools = vec![ToolDef {
            name: "get_weather".into(),
            description: "Get weather".into(),
            parameters: serde_json::json!({"type": "object"}),
        }];
        let body = http::tests::build_chat_request_test(
            "gpt-4o",
            &[Message::user("weather?")],
            &tools,
            &GenerateConfig::default(),
            false,
        );
        let api_tools = body["tools"].as_array().unwrap();
        assert_eq!(api_tools.len(), 1);
        assert_eq!(api_tools[0]["type"], "function");
        assert_eq!(api_tools[0]["function"]["name"], "get_weather");
    }

    #[test]
    fn chat_request_streaming() {
        let body = http::tests::build_chat_request_test(
            "gpt-4o",
            &[Message::user("Hi")],
            &[],
            &GenerateConfig::default(),
            true,
        );
        assert_eq!(body["stream"], true);
    }

    // ========================================================================
    // HTTP provider: response parsing (unit-testable without network)
    // ========================================================================

    #[test]
    fn parse_text_response() {
        let raw = serde_json::json!({
            "choices": [{
                "message": {
                    "content": "Hello! How can I help?",
                    "tool_calls": []
                }
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 8,
                "total_tokens": 18
            }
        });
        let resp = http::tests::parse_response_test(raw);
        assert_eq!(resp.content.as_deref(), Some("Hello! How can I help?"));
        assert!(resp.tool_calls.is_empty());
        assert_eq!(resp.usage.prompt_tokens, 10);
        assert_eq!(resp.usage.completion_tokens, 8);
        assert_eq!(resp.usage.total_tokens, 18);
    }

    #[test]
    fn parse_tool_call_response() {
        let raw = serde_json::json!({
            "choices": [{
                "message": {
                    "content": null,
                    "tool_calls": [{
                        "id": "call_abc",
                        "type": "function",
                        "function": {
                            "name": "get_weather",
                            "arguments": "{\"city\":\"SF\"}"
                        }
                    }]
                }
            }],
            "usage": { "prompt_tokens": 15, "completion_tokens": 20, "total_tokens": 35 }
        });
        let resp = http::tests::parse_response_test(raw);
        assert!(resp.content.is_none());
        assert_eq!(resp.tool_calls.len(), 1);
        assert_eq!(resp.tool_calls[0].id, "call_abc");
        assert_eq!(resp.tool_calls[0].name, "get_weather");
        assert_eq!(resp.tool_calls[0].arguments, r#"{"city":"SF"}"#);
    }

    #[test]
    fn parse_response_no_usage() {
        let raw = serde_json::json!({
            "choices": [{
                "message": { "content": "ok", "tool_calls": [] }
            }]
        });
        let resp = http::tests::parse_response_test(raw);
        assert_eq!(resp.content.as_deref(), Some("ok"));
        assert_eq!(resp.usage.total_tokens, 0);
    }

    #[test]
    fn parse_response_empty_choices_fails() {
        let raw = serde_json::json!({ "choices": [] });
        let result = http::tests::try_parse_response_test(raw);
        assert!(result.is_err());
    }
}
