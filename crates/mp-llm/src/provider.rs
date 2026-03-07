use crate::types::*;
use async_trait::async_trait;
use tokio_stream::Stream;
use std::pin::Pin;

pub type StreamResult = Pin<Box<dyn Stream<Item = anyhow::Result<StreamEvent>> + Send>>;

#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Non-streaming generation.
    async fn generate(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
        config: &GenerateConfig,
    ) -> anyhow::Result<GenerateResponse>;

    /// Streaming generation. Returns a stream of events.
    async fn generate_stream(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
        config: &GenerateConfig,
    ) -> anyhow::Result<StreamResult>;

    /// Generate an embedding vector for text.
    async fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>>;

    /// Whether this provider supports streaming.
    fn supports_streaming(&self) -> bool;

    /// Human-readable provider name (e.g. "http/openai", "local/gguf").
    fn name(&self) -> &str;
}
