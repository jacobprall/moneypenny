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

/// Dedicated trait for embedding providers.
///
/// Separate from LlmProvider because embeddings should run locally by default,
/// independent of which cloud LLM handles generation.
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Generate an embedding vector for a single text input.
    async fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>>;

    /// Generate embedding vectors for a batch of texts.
    /// Default implementation calls `embed()` sequentially.
    async fn embed_batch(&self, texts: &[&str]) -> anyhow::Result<Vec<Vec<f32>>> {
        let mut results = Vec::with_capacity(texts.len());
        for text in texts {
            results.push(self.embed(text).await?);
        }
        Ok(results)
    }

    /// The dimensionality of the embedding vectors this provider produces.
    fn dimensions(&self) -> usize;

    /// Human-readable provider name.
    fn name(&self) -> &str;
}
