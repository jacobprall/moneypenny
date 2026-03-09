use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,

    /// Separate directory for model files (GGUF). When set, models are loaded
    /// from here instead of `{data_dir}/models/`. Useful for Docker images
    /// where models are baked into a read-only layer while data_dir points to
    /// a persistent volume.
    #[serde(default)]
    pub models_dir: Option<PathBuf>,

    #[serde(default)]
    pub gateway: GatewayConfig,

    #[serde(default)]
    pub agents: Vec<AgentConfig>,

    #[serde(default)]
    pub channels: ChannelsConfig,

    #[serde(default)]
    pub sync: SyncConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayConfig {
    #[serde(default = "default_host")]
    pub host: String,

    #[serde(default = "default_port")]
    pub port: u16,

    #[serde(default = "default_log_level")]
    pub log_level: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub name: String,

    #[serde(default)]
    pub persona: Option<String>,

    #[serde(default = "default_trust_level")]
    pub trust_level: String,

    /// "allow" or "deny" — what happens when no policy rule matches.
    #[serde(default = "default_policy_mode")]
    pub policy_mode: String,

    #[serde(default)]
    pub llm: LlmConfig,

    #[serde(default)]
    pub embedding: EmbeddingConfig,

    /// MCP servers to connect to on startup for tool discovery.
    #[serde(default)]
    pub mcp_servers: Vec<McpServerConfig>,
}

/// Configuration for one MCP (Model Context Protocol) server.
/// The server is launched as a subprocess using the stdio transport.
///
/// Example TOML:
/// ```toml
/// [[agents.mcp_servers]]
/// name = "filesystem"
/// command = "npx"
/// args = ["-y", "@modelcontextprotocol/server-filesystem", "/home/user/projects"]
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    /// Short identifier used to namespace tool names: `{name}__{tool}`.
    pub name: String,
    /// Executable to launch (e.g. `npx`, `uvx`, `python`, `/usr/local/bin/my-server`).
    pub command: String,
    /// Arguments passed to the executable.
    #[serde(default)]
    pub args: Vec<String>,
    /// Extra environment variables injected into the server process.
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    #[serde(default = "default_provider")]
    pub provider: String,

    #[serde(default)]
    pub model: Option<String>,

    #[serde(default)]
    pub api_base: Option<String>,

    #[serde(default)]
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    #[serde(default = "default_embedding_provider")]
    pub provider: String,

    #[serde(default = "default_embedding_model")]
    pub model: String,

    /// Path to a local model file (GGUF). Resolved relative to data_dir/models/ if not absolute.
    #[serde(default)]
    pub model_path: Option<String>,

    /// Embedding vector dimensions. Used for pre-allocating storage.
    #[serde(default = "default_embedding_dimensions")]
    pub dimensions: usize,

    /// For remote embedding providers (provider = "http").
    #[serde(default)]
    pub api_base: Option<String>,

    /// For remote embedding providers.
    #[serde(default)]
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelsConfig {
    #[serde(default = "default_true")]
    pub cli: bool,

    #[serde(default)]
    pub http: Option<HttpChannelConfig>,

    #[serde(default)]
    pub slack: Option<SlackChannelConfig>,

    #[serde(default)]
    pub discord: Option<DiscordChannelConfig>,

    #[serde(default)]
    pub telegram: Option<TelegramChannelConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpChannelConfig {
    #[serde(default = "default_http_port")]
    pub port: u16,

    /// Bearer token required in `Authorization` header. If unset, no auth is enforced.
    #[serde(default)]
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackChannelConfig {
    /// Bot OAuth token (starts with `xoxb-`). Used for `chat.postMessage`.
    pub bot_token: String,
    /// Signing secret used to verify the `X-Slack-Signature` header.
    #[serde(default)]
    pub signing_secret: Option<String>,
    /// Default agent name to route Slack messages to.
    #[serde(default)]
    pub agent: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordChannelConfig {
    /// Discord application ID (used for slash-command setup).
    pub application_id: String,
    /// Ed25519 public key from the Discord developer portal (for request verification).
    pub public_key: String,
    /// Bot token for sending follow-up messages via the Discord REST API.
    pub bot_token: String,
    /// Default agent name to route Discord commands to.
    #[serde(default)]
    pub agent: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramChannelConfig {
    /// Bot token from @BotFather.
    pub bot_token: String,
    /// Default agent name to route Telegram messages to.
    #[serde(default)]
    pub agent: Option<String>,
}

/// Configuration for the sqlite-sync CRDT sync layer.
///
/// Example TOML:
/// ```toml
/// [sync]
/// # Cloud sync via SQLite Cloud (optional)
/// cloud_url = "https://sync.example.com/project-id?apikey=KEY"
///
/// # Local peer agents to sync with (agent names or absolute DB paths)
/// peers = ["other-agent"]
///
/// # Tables to replicate across peers (defaults shown)
/// tables = ["facts", "fact_links", "skills", "policies"]
///
/// # Seconds between automatic sync cycles; 0 = manual only
/// interval_secs = 300
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConfig {
    /// SQLite Cloud connection string (includes API key as query param).
    /// Enables cloud sync when set.
    #[serde(default)]
    pub cloud_url: Option<String>,

    /// Names of other agents (or absolute paths to their `.db` files)
    /// that this agent should sync with on each cycle.
    #[serde(default)]
    pub peers: Vec<String>,

    /// Tables to track and replicate. Only tables listed here get CRDT metadata.
    #[serde(default = "default_sync_tables")]
    pub tables: Vec<String>,

    /// How often (in seconds) the gateway auto-sync loop fires.
    /// `0` disables automatic sync — use `mp sync now` for manual.
    #[serde(default)]
    pub interval_secs: u64,
}

fn default_sync_tables() -> Vec<String> {
    vec![
        "facts".into(),
        "fact_links".into(),
        "skills".into(),
        "policies".into(),
    ]
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            cloud_url: None,
            peers: Vec::new(),
            tables: default_sync_tables(),
            interval_secs: 0,
        }
    }
}

impl AgentConfig {
    pub fn policy_mode(&self) -> crate::policy::PolicyMode {
        match self.policy_mode.as_str() {
            "deny" => crate::policy::PolicyMode::DenyByDefault,
            _ => crate::policy::PolicyMode::AllowByDefault,
        }
    }
}

impl Config {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let mut config: Config = toml::from_str(&content)?;
        config.apply_env_overrides();
        Ok(config)
    }

    /// Allow environment variables to override config for containerized
    /// deployments (Fly secrets, Docker env, etc.).
    fn apply_env_overrides(&mut self) {
        if let Ok(val) = std::env::var("MP_DATA_DIR") {
            self.data_dir = PathBuf::from(val);
        }
        if let Ok(val) = std::env::var("MP_MODELS_DIR") {
            self.models_dir = Some(PathBuf::from(val));
        }
        // BYOK: tenant sets their LLM key as a Fly secret / env var.
        if let Ok(val) = std::env::var("ANTHROPIC_API_KEY") {
            for agent in &mut self.agents {
                if agent.llm.provider == "anthropic" && agent.llm.api_key.is_none() {
                    agent.llm.api_key = Some(val.clone());
                }
            }
        }
        if let Ok(val) = std::env::var("OPENAI_API_KEY") {
            for agent in &mut self.agents {
                if agent.llm.provider == "http" && agent.llm.api_key.is_none() {
                    agent.llm.api_key = Some(val.clone());
                }
            }
        }
        if let Ok(val) = std::env::var("SQLITE_CLOUD_URL") {
            if self.sync.cloud_url.is_none() {
                self.sync.cloud_url = Some(val);
            }
        }
    }

    pub fn default_config() -> Self {
        Config {
            data_dir: default_data_dir(),
            models_dir: None,
            gateway: GatewayConfig::default(),
            agents: vec![AgentConfig {
                name: "main".to_string(),
                persona: None,
                trust_level: "standard".to_string(),
                policy_mode: default_policy_mode(),
                llm: LlmConfig::default(),
                embedding: EmbeddingConfig::default(),
                mcp_servers: Vec::new(),
            }],
            channels: ChannelsConfig::default(),
            sync: SyncConfig::default(),
        }
    }

    pub fn to_toml(&self) -> anyhow::Result<String> {
        Ok(toml::to_string_pretty(self)?)
    }

    pub fn agent_db_path(&self, agent_name: &str) -> PathBuf {
        self.data_dir.join(format!("{agent_name}.db"))
    }

    pub fn metadata_db_path(&self) -> PathBuf {
        self.data_dir.join("metadata.db")
    }

    pub fn models_dir(&self) -> PathBuf {
        self.models_dir
            .clone()
            .unwrap_or_else(|| self.data_dir.join("models"))
    }
}

impl EmbeddingConfig {
    /// Resolve the model file path. If `model_path` is set, use it directly.
    /// Otherwise, derive from the model name inside the given models directory.
    pub fn resolve_model_path(&self, models_dir: &Path) -> PathBuf {
        if let Some(p) = &self.model_path {
            PathBuf::from(p)
        } else {
            models_dir.join(format!("{}.gguf", self.model))
        }
    }
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            log_level: default_log_level(),
        }
    }
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            provider: default_provider(),
            model: None,
            api_base: None,
            api_key: None,
        }
    }
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            provider: default_embedding_provider(),
            model: default_embedding_model(),
            model_path: None,
            dimensions: default_embedding_dimensions(),
            api_base: None,
            api_key: None,
        }
    }
}

impl Default for ChannelsConfig {
    fn default() -> Self {
        Self {
            cli: true,
            http: None,
            slack: None,
            discord: None,
            telegram: None,
        }
    }
}

fn default_data_dir() -> PathBuf {
    PathBuf::from("./mp-data")
}

fn default_host() -> String {
    "127.0.0.1".to_string()
}

fn default_port() -> u16 {
    4820
}

fn default_http_port() -> u16 {
    4821
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_trust_level() -> String {
    "standard".to_string()
}

fn default_provider() -> String {
    "anthropic".to_string()
}

fn default_policy_mode() -> String {
    "allow".to_string()
}

fn default_embedding_provider() -> String {
    "local".to_string()
}

fn default_embedding_model() -> String {
    "nomic-embed-text-v1.5".to_string()
}

fn default_embedding_dimensions() -> usize {
    768
}

fn default_true() -> bool {
    true
}
