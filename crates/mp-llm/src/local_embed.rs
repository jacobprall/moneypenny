use crate::provider::EmbeddingProvider;
use async_trait::async_trait;
use std::path::PathBuf;

/// Local embedding provider using GGUF models via sqlite-ai.
///
/// Ships with `nomic-embed-text-v1.5` by default (768-dim, ~274MB GGUF).
/// Runs entirely on-device — no network, no API keys, no data leaving the machine.
///
/// Model is loaded on first use and kept in memory for subsequent calls.
/// The GGUF file is expected at `model_path` (typically `<data_dir>/models/<model>.gguf`).
pub struct LocalEmbeddingProvider {
    model_path: PathBuf,
    model_name: String,
    dims: usize,
}

impl LocalEmbeddingProvider {
    pub fn new(model_path: PathBuf, model_name: String, dimensions: usize) -> Self {
        Self {
            model_path,
            model_name,
            dims: dimensions,
        }
    }

    pub fn model_path(&self) -> &PathBuf {
        &self.model_path
    }

    pub fn model_name(&self) -> &str {
        &self.model_name
    }
}

#[async_trait]
impl EmbeddingProvider for LocalEmbeddingProvider {
    async fn embed(&self, _text: &str) -> anyhow::Result<Vec<f32>> {
        // Implementation path: sqlite-ai extension provides `ai_embed()` which
        // loads a GGUF embedding model and returns a float vector.
        //
        // When sqlite-ai compiles (see TASKS.md cross-cutting section):
        //   1. Open an in-memory SQLite connection with sqlite-ai loaded
        //   2. Call: SELECT ai_embed(:model_path, :text)
        //   3. Parse the result blob into Vec<f32>
        //
        // For now, blocked on sqlite-ai extension compilation.
        anyhow::bail!(
            "Local embedding not yet available — requires sqlite-ai extension. \
             Model: {} at {:?}. See TASKS.md for status.",
            self.model_name,
            self.model_path,
        )
    }

    fn dimensions(&self) -> usize {
        self.dims
    }

    fn name(&self) -> &str {
        "local"
    }
}

/// HTTP-based embedding provider for remote APIs (OpenAI-compatible).
///
/// Use when you want cloud embeddings instead of local. Not the default.
pub struct HttpEmbeddingProvider {
    client: reqwest::Client,
    api_base: String,
    api_key: Option<String>,
    model: String,
    dims: usize,
}

impl HttpEmbeddingProvider {
    pub fn new(
        api_base: impl Into<String>,
        api_key: Option<String>,
        model: impl Into<String>,
        dimensions: usize,
    ) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_base: api_base.into(),
            api_key,
            model: model.into(),
            dims: dimensions,
        }
    }
}

#[derive(serde::Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
}

#[derive(serde::Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
}

#[async_trait]
impl EmbeddingProvider for HttpEmbeddingProvider {
    async fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>> {
        let body = serde_json::json!({
            "model": self.model,
            "input": text,
        });

        let mut req = self.client
            .post(format!("{}/embeddings", self.api_base))
            .json(&body);

        if let Some(key) = &self.api_key {
            req = req.bearer_auth(key);
        }

        let resp = req.send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Embedding API error {status}: {body}");
        }

        let api_resp: EmbeddingResponse = resp.json().await?;
        api_resp
            .data
            .into_iter()
            .next()
            .map(|d| d.embedding)
            .ok_or_else(|| anyhow::anyhow!("No embedding returned"))
    }

    async fn embed_batch(&self, texts: &[&str]) -> anyhow::Result<Vec<Vec<f32>>> {
        let body = serde_json::json!({
            "model": self.model,
            "input": texts,
        });

        let mut req = self.client
            .post(format!("{}/embeddings", self.api_base))
            .json(&body);

        if let Some(key) = &self.api_key {
            req = req.bearer_auth(key);
        }

        let resp = req.send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Embedding API error {status}: {body}");
        }

        let api_resp: EmbeddingResponse = resp.json().await?;
        Ok(api_resp.data.into_iter().map(|d| d.embedding).collect())
    }

    fn dimensions(&self) -> usize {
        self.dims
    }

    fn name(&self) -> &str {
        "http"
    }
}
