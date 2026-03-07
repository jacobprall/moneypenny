pub mod types;
pub mod provider;
pub mod anthropic;
pub mod http;
pub mod local_embed;
pub mod sqlite_ai;

use provider::{LlmProvider, EmbeddingProvider};

/// Build an LlmProvider for generation from configuration values.
pub fn build_provider(
    provider_type: &str,
    api_base: Option<&str>,
    api_key: Option<&str>,
    model: Option<&str>,
) -> anyhow::Result<Box<dyn LlmProvider>> {
    match provider_type {
        "anthropic" => Ok(Box::new(anthropic::AnthropicProvider::from_config(
            api_base, api_key, model,
        ))),
        "http" => Ok(Box::new(http::HttpProvider::from_config(
            api_base, api_key, model,
        ))),
        "local" => {
            let model_path = model
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| std::path::PathBuf::from("models/default.gguf"));
            Ok(Box::new(sqlite_ai::SqliteAiProvider::new(model_path, None)))
        }
        other => anyhow::bail!("Unknown LLM provider: {other}. Use 'anthropic', 'http', or 'local'."),
    }
}

/// Build an EmbeddingProvider from configuration values.
///
/// Defaults to local GGUF inference with nomic-embed-text-v1.5.
/// Falls back to HTTP for remote embedding APIs (OpenAI-compatible).
pub fn build_embedding_provider(
    provider_type: &str,
    model_name: &str,
    model_path: &std::path::Path,
    dimensions: usize,
    api_base: Option<&str>,
    api_key: Option<&str>,
) -> anyhow::Result<Box<dyn EmbeddingProvider>> {
    match provider_type {
        "local" => Ok(Box::new(local_embed::LocalEmbeddingProvider::new(
            model_path.to_path_buf(),
            model_name.to_string(),
            dimensions,
        ))),
        "http" => Ok(Box::new(local_embed::HttpEmbeddingProvider::new(
            api_base.unwrap_or("https://api.openai.com/v1"),
            api_key.map(String::from),
            model_name,
            dimensions,
        ))),
        other => anyhow::bail!(
            "Unknown embedding provider: {other}. Use 'local' or 'http'."
        ),
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
    fn anthropic_provider_metadata() {
        let p = anthropic::AnthropicProvider::from_config(None, None, None);
        assert_eq!(p.name(), "anthropic");
        assert!(p.supports_streaming());
    }

    #[test]
    fn http_provider_metadata() {
        let p = http::HttpProvider::from_config(None, None, None);
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
    fn build_anthropic_provider() {
        let p = build_provider("anthropic", None, None, None).unwrap();
        assert_eq!(p.name(), "anthropic");
        assert!(p.supports_streaming());
    }

    #[test]
    fn build_anthropic_with_custom_config() {
        let p = build_provider(
            "anthropic",
            Some("https://api.anthropic.com"),
            Some("sk-ant-test"),
            Some("claude-sonnet-4-20250514"),
        ).unwrap();
        assert_eq!(p.name(), "anthropic");
    }

    #[test]
    fn build_http_provider() {
        let p = build_provider("http", None, None, None).unwrap();
        assert_eq!(p.name(), "http");
        assert!(p.supports_streaming());
    }

    #[test]
    fn build_local_provider() {
        let p = build_provider("local", None, None, None).unwrap();
        assert_eq!(p.name(), "sqlite-ai");
        assert!(!p.supports_streaming());
    }

    #[test]
    fn build_unknown_provider_fails() {
        let result = build_provider("magic", None, None, None);
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
        ).unwrap();
        assert_eq!(p.name(), "http");
    }

    // ========================================================================
    // Embedding provider factory tests
    // ========================================================================

    #[test]
    fn embedding_trait_is_object_safe() {
        fn _accepts_boxed(_p: Box<dyn provider::EmbeddingProvider>) {}
    }

    #[test]
    fn build_local_embedding_provider() {
        let p = build_embedding_provider(
            "local", "nomic-embed-text-v1.5",
            std::path::Path::new("/tmp/models/nomic.gguf"),
            768, None, None,
        ).unwrap();
        assert_eq!(p.name(), "local");
        assert_eq!(p.dimensions(), 768);
    }

    #[test]
    fn build_http_embedding_provider() {
        let p = build_embedding_provider(
            "http", "text-embedding-3-small",
            std::path::Path::new(""),
            1536, Some("https://api.openai.com/v1"), Some("sk-test"),
        ).unwrap();
        assert_eq!(p.name(), "http");
        assert_eq!(p.dimensions(), 1536);
    }

    #[test]
    fn build_unknown_embedding_provider_fails() {
        let result = build_embedding_provider(
            "magic", "model", std::path::Path::new(""), 768, None, None,
        );
        match result {
            Ok(_) => panic!("expected error for unknown embedding provider"),
            Err(e) => assert!(e.to_string().contains("Unknown embedding provider")),
        }
    }

    #[tokio::test]
    async fn local_embedding_returns_stub_error() {
        let p = local_embed::LocalEmbeddingProvider::new(
            "/tmp/model.gguf".into(), "nomic-embed-text-v1.5".into(), 768,
        );
        let result = p.embed("hello").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("sqlite-ai extension"));
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

    // ========================================================================
    // Anthropic provider: request building
    // ========================================================================

    #[test]
    fn anthropic_request_basic() {
        let body = anthropic::tests::build_request_test(
            "claude-sonnet-4-20250514",
            &[Message::system("You are helpful"), Message::user("Hi")],
            &[],
            &GenerateConfig { max_tokens: Some(1024), temperature: Some(0.5), stop: vec![] },
            false,
        );
        assert_eq!(body["model"], "claude-sonnet-4-20250514");
        assert_eq!(body["max_tokens"], 1024);
        assert_eq!(body["system"], "You are helpful");
        assert_eq!(body["temperature"], 0.5);
        assert!(body.get("stream").is_none());

        let msgs = body["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0]["role"], "user");
        assert_eq!(msgs[0]["content"], "Hi");
    }

    #[test]
    fn anthropic_request_system_not_in_messages() {
        let body = anthropic::tests::build_request_test(
            "claude-sonnet-4-20250514",
            &[Message::system("Be concise"), Message::system("No emojis"), Message::user("Hello")],
            &[],
            &GenerateConfig::default(),
            false,
        );
        assert_eq!(body["system"], "Be concise\n\nNo emojis");
        let msgs = body["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 1);
        assert!(msgs.iter().all(|m| m["role"] != "system"));
    }

    #[test]
    fn anthropic_request_max_tokens_default() {
        let body = anthropic::tests::build_request_test(
            "claude-sonnet-4-20250514",
            &[Message::user("Hi")],
            &[],
            &GenerateConfig::default(),
            false,
        );
        assert_eq!(body["max_tokens"], 8192);
    }

    #[test]
    fn anthropic_request_with_tools() {
        let tools = vec![ToolDef {
            name: "get_weather".into(),
            description: "Get weather".into(),
            parameters: serde_json::json!({"type": "object", "properties": {"city": {"type": "string"}}}),
        }];
        let body = anthropic::tests::build_request_test(
            "claude-sonnet-4-20250514",
            &[Message::user("weather?")],
            &tools,
            &GenerateConfig::default(),
            false,
        );
        let api_tools = body["tools"].as_array().unwrap();
        assert_eq!(api_tools.len(), 1);
        assert_eq!(api_tools[0]["name"], "get_weather");
        assert_eq!(api_tools[0]["input_schema"]["type"], "object");
        assert!(api_tools[0].get("type").is_none());
    }

    #[test]
    fn anthropic_request_streaming() {
        let body = anthropic::tests::build_request_test(
            "claude-sonnet-4-20250514",
            &[Message::user("Hi")],
            &[],
            &GenerateConfig::default(),
            true,
        );
        assert_eq!(body["stream"], true);
    }

    #[test]
    fn anthropic_request_tool_results_as_user_message() {
        let body = anthropic::tests::build_request_test(
            "claude-sonnet-4-20250514",
            &[
                Message::user("What's the weather?"),
                Message::assistant_with_tool_calls(None, vec![ToolCall {
                    id: "toolu_1".into(),
                    name: "get_weather".into(),
                    arguments: r#"{"city":"SF"}"#.into(),
                }]),
                Message::tool("72F and sunny", "toolu_1"),
            ],
            &[],
            &GenerateConfig::default(),
            false,
        );
        let msgs = body["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 3);

        // Assistant message has tool_use content block
        let asst = &msgs[1];
        assert_eq!(asst["role"], "assistant");
        let content = asst["content"].as_array().unwrap();
        assert_eq!(content[0]["type"], "tool_use");
        assert_eq!(content[0]["name"], "get_weather");
        assert_eq!(content[0]["input"]["city"], "SF");

        // Tool result is in a user message
        let tool_msg = &msgs[2];
        assert_eq!(tool_msg["role"], "user");
        let tool_content = tool_msg["content"].as_array().unwrap();
        assert_eq!(tool_content[0]["type"], "tool_result");
        assert_eq!(tool_content[0]["tool_use_id"], "toolu_1");
        assert_eq!(tool_content[0]["content"], "72F and sunny");
    }

    #[test]
    fn anthropic_request_multiple_tool_results_merged() {
        let body = anthropic::tests::build_request_test(
            "claude-sonnet-4-20250514",
            &[
                Message::user("weather in SF and NYC?"),
                Message::assistant_with_tool_calls(None, vec![
                    ToolCall { id: "t1".into(), name: "get_weather".into(), arguments: r#"{"city":"SF"}"#.into() },
                    ToolCall { id: "t2".into(), name: "get_weather".into(), arguments: r#"{"city":"NYC"}"#.into() },
                ]),
                Message::tool("72F", "t1"),
                Message::tool("55F", "t2"),
            ],
            &[],
            &GenerateConfig::default(),
            false,
        );
        let msgs = body["messages"].as_array().unwrap();
        // user, assistant, user (merged tool results) = 3 messages, not 4
        assert_eq!(msgs.len(), 3);
        let tool_msg = &msgs[2];
        let tool_content = tool_msg["content"].as_array().unwrap();
        assert_eq!(tool_content.len(), 2);
        assert_eq!(tool_content[0]["tool_use_id"], "t1");
        assert_eq!(tool_content[1]["tool_use_id"], "t2");
    }

    // ========================================================================
    // Anthropic provider: response parsing
    // ========================================================================

    #[test]
    fn anthropic_parse_text_response() {
        let raw = serde_json::json!({
            "content": [{"type": "text", "text": "Hello! How can I help?"}],
            "stop_reason": "end_turn",
            "usage": {"input_tokens": 10, "output_tokens": 8}
        });
        let resp = anthropic::tests::parse_response_test(raw);
        assert_eq!(resp.content.as_deref(), Some("Hello! How can I help?"));
        assert!(resp.tool_calls.is_empty());
        assert_eq!(resp.usage.prompt_tokens, 10);
        assert_eq!(resp.usage.completion_tokens, 8);
        assert_eq!(resp.usage.total_tokens, 18);
    }

    #[test]
    fn anthropic_parse_tool_use_response() {
        let raw = serde_json::json!({
            "content": [
                {"type": "tool_use", "id": "toolu_abc", "name": "get_weather", "input": {"city": "SF"}}
            ],
            "stop_reason": "tool_use",
            "usage": {"input_tokens": 15, "output_tokens": 20}
        });
        let resp = anthropic::tests::parse_response_test(raw);
        assert!(resp.content.is_none());
        assert_eq!(resp.tool_calls.len(), 1);
        assert_eq!(resp.tool_calls[0].id, "toolu_abc");
        assert_eq!(resp.tool_calls[0].name, "get_weather");
        let args: serde_json::Value = serde_json::from_str(&resp.tool_calls[0].arguments).unwrap();
        assert_eq!(args["city"], "SF");
    }

    #[test]
    fn anthropic_parse_mixed_response() {
        let raw = serde_json::json!({
            "content": [
                {"type": "text", "text": "Let me check the weather."},
                {"type": "tool_use", "id": "toolu_1", "name": "get_weather", "input": {"city": "SF"}}
            ],
            "stop_reason": "tool_use",
            "usage": {"input_tokens": 20, "output_tokens": 30}
        });
        let resp = anthropic::tests::parse_response_test(raw);
        assert_eq!(resp.content.as_deref(), Some("Let me check the weather."));
        assert_eq!(resp.tool_calls.len(), 1);
        assert_eq!(resp.tool_calls[0].name, "get_weather");
    }

    #[test]
    fn anthropic_parse_no_usage() {
        let raw = serde_json::json!({
            "content": [{"type": "text", "text": "ok"}],
            "stop_reason": "end_turn"
        });
        let resp = anthropic::tests::parse_response_test(raw);
        assert_eq!(resp.content.as_deref(), Some("ok"));
        assert_eq!(resp.usage.total_tokens, 0);
    }

    #[test]
    fn anthropic_parse_empty_content() {
        let raw = serde_json::json!({
            "content": [],
            "stop_reason": "end_turn"
        });
        let result = anthropic::tests::try_parse_response_test(raw);
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert!(resp.content.is_none());
        assert!(resp.tool_calls.is_empty());
    }

    #[tokio::test]
    async fn anthropic_embed_returns_unsupported_error() {
        let p = anthropic::AnthropicProvider::from_config(None, None, None);
        let result = p.embed("hello").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("does not provide an embeddings API"));
    }
}
