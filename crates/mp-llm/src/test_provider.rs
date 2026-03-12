//! Test LLM provider for E2E and integration tests.
//!
//! Returns a configurable sequence of responses (text and/or tool calls) without
//! calling a real model. Use for exercising the agent loop, tool execution, and
//! session state without network or API keys.

use crate::provider::{LlmProvider, StreamResult};
use crate::types::*;
use async_trait::async_trait;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// A test LLM provider that returns predefined responses in sequence.
///
/// Each call to `generate` or `generate_stream` returns the next response from
/// the configured list. When exhausted, returns an empty text response (useful
/// for extraction and other follow-up calls in tests).
///
/// # Example
///
/// ```ignore
/// let provider = TestLlmProvider::builder()
///     .with_tool_call("fact_add", serde_json::json!({"content": "User prefers dark mode"}))
///     .with_text("I've remembered that.")
///     .build();
/// ```
#[derive(Clone)]
pub struct TestLlmProvider {
    responses: Arc<Vec<GenerateResponse>>,
    next: Arc<AtomicUsize>,
    supports_streaming: bool,
}

impl TestLlmProvider {
    /// Create a builder for configuring the response sequence.
    pub fn builder() -> TestLlmProviderBuilder {
        TestLlmProviderBuilder::default()
    }

    /// Create a provider that returns a single text response.
    pub fn with_text(text: impl Into<String>) -> Self {
        Self::builder().with_text(text).build()
    }

    /// Create a provider that returns a tool call, then a final text response.
    pub fn with_tool_then_text(
        tool_name: impl Into<String>,
        tool_args: serde_json::Value,
        final_text: impl Into<String>,
    ) -> Self {
        Self::builder()
            .with_tool_call(tool_name, tool_args)
            .with_text(final_text)
            .build()
    }

    fn next_response(&self) -> GenerateResponse {
        let idx = self.next.fetch_add(1, Ordering::SeqCst);
        self.responses
            .get(idx)
            .cloned()
            .unwrap_or_else(|| GenerateResponse {
                content: Some("[]".to_string()),
                tool_calls: vec![],
                usage: Usage::default(),
            })
    }
}

/// Builder for `TestLlmProvider`.
#[derive(Default)]
pub struct TestLlmProviderBuilder {
    responses: Vec<GenerateResponse>,
    supports_streaming: bool,
}

impl TestLlmProviderBuilder {
    /// Add a text-only response.
    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.responses.push(GenerateResponse {
            content: Some(text.into()),
            tool_calls: vec![],
            usage: Usage {
                prompt_tokens: 1,
                completion_tokens: 1,
                total_tokens: 2,
            },
        });
        self
    }

    /// Add a tool-call-only response (no text).
    pub fn with_tool_call(
        mut self,
        name: impl Into<String>,
        args: serde_json::Value,
    ) -> Self {
        let id = format!("call_{}", self.responses.len());
        self.responses.push(GenerateResponse {
            content: None,
            tool_calls: vec![ToolCall {
                id: id.clone(),
                name: name.into(),
                arguments: args.to_string(),
            }],
            usage: Usage {
                prompt_tokens: 1,
                completion_tokens: 10,
                total_tokens: 11,
            },
        });
        self
    }

    /// Add a response with both text and tool calls.
    pub fn with_text_and_tool_call(
        mut self,
        text: impl Into<String>,
        name: impl Into<String>,
        args: serde_json::Value,
    ) -> Self {
        let id = format!("call_{}", self.responses.len());
        self.responses.push(GenerateResponse {
            content: Some(text.into()),
            tool_calls: vec![ToolCall {
                id: id.clone(),
                name: name.into(),
                arguments: args.to_string(),
            }],
            usage: Usage {
                prompt_tokens: 1,
                completion_tokens: 15,
                total_tokens: 16,
            },
        });
        self
    }

    /// Add a raw `GenerateResponse` for full control.
    pub fn with_response(mut self, resp: GenerateResponse) -> Self {
        self.responses.push(resp);
        self
    }

    /// Enable streaming. Default is false (uses `generate` in the agent).
    pub fn streaming(mut self, enabled: bool) -> Self {
        self.supports_streaming = enabled;
        self
    }

    /// Build the provider.
    pub fn build(self) -> TestLlmProvider {
        TestLlmProvider {
            responses: Arc::new(self.responses),
            next: Arc::new(AtomicUsize::new(0)),
            supports_streaming: self.supports_streaming,
        }
    }
}

#[async_trait]
impl LlmProvider for TestLlmProvider {
    async fn generate(
        &self,
        _messages: &[Message],
        _tools: &[ToolDef],
        _config: &GenerateConfig,
    ) -> anyhow::Result<GenerateResponse> {
        Ok(self.next_response())
    }

    async fn generate_stream(
        &self,
        _messages: &[Message],
        _tools: &[ToolDef],
        _config: &GenerateConfig,
    ) -> anyhow::Result<StreamResult> {
        let resp = self.next_response();
        let events: Vec<anyhow::Result<StreamEvent>> = {
            let mut evs = Vec::new();
            if let Some(ref text) = resp.content {
                evs.push(Ok(StreamEvent::Delta(text.clone())));
            }
            for tc in &resp.tool_calls {
                evs.push(Ok(StreamEvent::ToolCall(tc.clone())));
            }
            evs.push(Ok(StreamEvent::Done(resp.usage)));
            evs
        };
        let stream = tokio_stream::iter(events);
        Ok(Box::pin(stream))
    }

    async fn embed(&self, _text: &str) -> anyhow::Result<Vec<f32>> {
        Ok(vec![0.0; 768])
    }

    fn supports_streaming(&self) -> bool {
        self.supports_streaming
    }

    fn name(&self) -> &str {
        "test"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_provider_returns_text() {
        let p = TestLlmProvider::with_text("Hello, world!");
        let resp = p
            .generate(&[], &[], &GenerateConfig::default())
            .await
            .unwrap();
        assert_eq!(resp.content.as_deref(), Some("Hello, world!"));
        assert!(resp.tool_calls.is_empty());
    }

    #[tokio::test]
    async fn test_provider_returns_tool_call_then_text() {
        let p = TestLlmProvider::with_tool_then_text(
            "fact_add",
            serde_json::json!({"content": "User likes Rust"}),
            "I've remembered that.",
        );
        let r1 = p.generate(&[], &[], &GenerateConfig::default()).await.unwrap();
        assert!(r1.content.is_none());
        assert_eq!(r1.tool_calls.len(), 1);
        assert_eq!(r1.tool_calls[0].name, "fact_add");
        assert!(r1.tool_calls[0].arguments.contains("Rust"));

        let r2 = p.generate(&[], &[], &GenerateConfig::default()).await.unwrap();
        assert_eq!(r2.content.as_deref(), Some("I've remembered that."));
        assert!(r2.tool_calls.is_empty());
    }

    #[tokio::test]
    async fn test_provider_exhausted_returns_empty_json() {
        let p = TestLlmProvider::with_text("Only one");
        let _ = p.generate(&[], &[], &GenerateConfig::default()).await.unwrap();
        let exhausted = p.generate(&[], &[], &GenerateConfig::default()).await.unwrap();
        assert_eq!(exhausted.content.as_deref(), Some("[]"));
        assert!(exhausted.tool_calls.is_empty());
    }

    #[tokio::test]
    async fn test_provider_embed_returns_placeholder() {
        let p = TestLlmProvider::with_text("x");
        let vec = p.embed("hello").await.unwrap();
        assert_eq!(vec.len(), 768);
        assert!(vec.iter().all(|&x| x == 0.0));
    }

    #[tokio::test]
    async fn test_provider_streaming() {
        let p = TestLlmProvider::builder()
            .with_text("Streamed")
            .streaming(true)
            .build();
        assert!(p.supports_streaming());
        let mut stream = p
            .generate_stream(&[], &[], &GenerateConfig::default())
            .await
            .unwrap();
        use tokio_stream::StreamExt;
        let mut got = Vec::new();
        while let Some(ev) = stream.next().await {
            got.push(ev.unwrap());
        }
        assert_eq!(got.len(), 2); // Delta + Done
        match &got[0] {
            StreamEvent::Delta(s) => assert_eq!(s, "Streamed"),
            _ => panic!("expected Delta first"),
        }
    }
}
