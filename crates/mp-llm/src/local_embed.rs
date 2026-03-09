use crate::provider::EmbeddingProvider;
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

// ---------------------------------------------------------------------------
// Shared embedding state (one SQLite connection per provider instance)
// ---------------------------------------------------------------------------

struct EmbedState {
    conn: rusqlite::Connection,
    model_loaded: bool,
}

// rusqlite::Connection is Send (unsafe impl in rusqlite 0.35). Wrapping in
// Mutex<EmbedState> gives us the Sync we need for the async trait.
unsafe impl Send for EmbedState {}
unsafe impl Sync for EmbedState {}

// ---------------------------------------------------------------------------
// LocalEmbeddingProvider
// ---------------------------------------------------------------------------

/// Local embedding provider using GGUF models via the sqlite-ai extension.
///
/// Ships with `nomic-embed-text-v1.5` by default (768-dim, ~274MB GGUF).
/// Runs entirely on-device — no network, no API keys, no data leaving the machine.
///
/// Internally keeps a single SQLite in-memory connection with sqlite-ai loaded.
/// The GGUF model is loaded lazily on the first `embed()` call and stays warm
/// for the lifetime of the provider.
pub struct LocalEmbeddingProvider {
    model_path: PathBuf,
    model_name: String,
    dims: usize,
    state: Arc<Mutex<EmbedState>>,
}

impl LocalEmbeddingProvider {
    pub fn new(model_path: PathBuf, model_name: String, dimensions: usize) -> anyhow::Result<Self> {
        let conn = rusqlite::Connection::open_in_memory()?;
        mp_ext::init_all_extensions(&conn)?;
        Ok(Self {
            model_path,
            model_name,
            dims: dimensions,
            state: Arc::new(Mutex::new(EmbedState {
                conn,
                model_loaded: false,
            })),
        })
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
    async fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>> {
        let state = self.state.clone();
        let model_path = self.model_path.clone();
        let text = text.to_string();

        tokio::task::spawn_blocking(move || {
            let mut guard = state
                .lock()
                .map_err(|_| anyhow::anyhow!("embedding state lock poisoned"))?;

            // Lazy model load — expensive but happens only once per provider.
            if !guard.model_loaded {
                let path_str = model_path
                    .to_str()
                    .ok_or_else(|| anyhow::anyhow!("model path is not valid UTF-8"))?;

                if !model_path.exists() {
                    anyhow::bail!(
                        "embedding model not found at {:?}. \
                         Download nomic-embed-text-v1.5.Q4_K_M.gguf into the models/ directory.",
                        model_path
                    );
                }

                guard
                    .conn
                    .execute("SELECT llm_model_load(?1)", rusqlite::params![path_str])?;
                guard
                    .conn
                    .execute("SELECT llm_context_create_embedding()", [])?;
                guard.model_loaded = true;
                tracing::debug!(model = path_str, "sqlite-ai embedding model loaded");
            }

            // Generate embedding — returns a raw FLOAT32 little-endian BLOB.
            let blob: Vec<u8> = guard.conn.query_row(
                "SELECT llm_embed_generate(?1)",
                rusqlite::params![text],
                |r| r.get(0),
            )?;

            parse_f32_blob(&blob)
        })
        .await?
    }

    fn dimensions(&self) -> usize {
        self.dims
    }

    fn name(&self) -> &str {
        "local"
    }
}

// ---------------------------------------------------------------------------
// HttpEmbeddingProvider (unchanged, remote OpenAI-compatible API)
// ---------------------------------------------------------------------------

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

        let mut req = self
            .client
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

        let mut req = self
            .client
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

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Parse a raw FLOAT32 little-endian BLOB into a Vec<f32>.
/// Used by both LocalEmbeddingProvider and vector search code.
pub fn parse_f32_blob(blob: &[u8]) -> anyhow::Result<Vec<f32>> {
    if blob.len() % 4 != 0 {
        anyhow::bail!(
            "invalid embedding blob length: {} bytes (must be divisible by 4)",
            blob.len()
        );
    }
    Ok(blob
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect())
}

/// Encode a Vec<f32> into a raw FLOAT32 little-endian BLOB for SQLite storage.
pub fn f32_slice_to_blob(v: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for f in v {
        out.extend_from_slice(&f.to_le_bytes());
    }
    out
}
