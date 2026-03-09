use crate::provider::{LlmProvider, StreamResult};
use crate::types::*;
use async_trait::async_trait;
use std::path::PathBuf;

/// Local GGUF inference via the sqlite-ai extension.
///
/// Uses `sqlite-ai` for both generation and embeddings, running entirely
/// on-device with no network required.
pub struct SqliteAiProvider {
    model_path: PathBuf,
    embedding_model_path: Option<PathBuf>,
}

impl SqliteAiProvider {
    pub fn new(model_path: PathBuf, embedding_model_path: Option<PathBuf>) -> Self {
        Self {
            model_path,
            embedding_model_path,
        }
    }
}

#[async_trait]
impl LlmProvider for SqliteAiProvider {
    async fn generate(
        &self,
        _messages: &[Message],
        _tools: &[ToolDef],
        _config: &GenerateConfig,
    ) -> anyhow::Result<GenerateResponse> {
        // M3 scope: the trait and types are the deliverable.
        // sqlite-ai integration requires loading the native extension and calling
        // ai_complete() — wired up when the extension loading pipeline is ready.
        anyhow::bail!(
            "SqliteAiProvider::generate not yet implemented. \
             Requires sqlite-ai extension at {:?}",
            self.model_path
        )
    }

    async fn generate_stream(
        &self,
        _messages: &[Message],
        _tools: &[ToolDef],
        _config: &GenerateConfig,
    ) -> anyhow::Result<StreamResult> {
        anyhow::bail!("SqliteAiProvider does not support streaming yet")
    }

    async fn embed(&self, _text: &str) -> anyhow::Result<Vec<f32>> {
        anyhow::bail!(
            "SqliteAiProvider::embed not yet implemented. \
             Requires sqlite-ai extension at {:?}",
            self.embedding_model_path
        )
    }

    fn supports_streaming(&self) -> bool {
        false
    }

    fn name(&self) -> &str {
        "sqlite-ai"
    }
}
