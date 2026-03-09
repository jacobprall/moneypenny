use crate::provider::{LlmProvider, StreamResult};
use crate::types::*;
use async_trait::async_trait;

const NOT_CONFIGURED: &str = "\
No LLM provider configured. To use conversational features (mp chat, mp ask), \
set provider = \"anthropic\" in [agents.llm] and ANTHROPIC_API_KEY in .env.";

pub struct NoopProvider;

#[async_trait]
impl LlmProvider for NoopProvider {
    async fn generate(
        &self,
        _messages: &[Message],
        _tools: &[ToolDef],
        _config: &GenerateConfig,
    ) -> anyhow::Result<GenerateResponse> {
        anyhow::bail!(NOT_CONFIGURED)
    }

    async fn generate_stream(
        &self,
        _messages: &[Message],
        _tools: &[ToolDef],
        _config: &GenerateConfig,
    ) -> anyhow::Result<StreamResult> {
        anyhow::bail!(NOT_CONFIGURED)
    }

    async fn embed(&self, _text: &str) -> anyhow::Result<Vec<f32>> {
        anyhow::bail!(NOT_CONFIGURED)
    }

    fn supports_streaming(&self) -> bool {
        false
    }

    fn name(&self) -> &str {
        "none"
    }
}
