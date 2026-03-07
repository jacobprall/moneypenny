use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,

    #[serde(default)]
    pub gateway: GatewayConfig,

    #[serde(default)]
    pub agents: Vec<AgentConfig>,

    #[serde(default)]
    pub channels: ChannelsConfig,
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

    #[serde(default)]
    pub llm: LlmConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    #[serde(default = "default_provider")]
    pub provider: String,

    #[serde(default)]
    pub model: Option<String>,

    #[serde(default)]
    pub embedding_model: Option<String>,

    #[serde(default)]
    pub api_base: Option<String>,

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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpChannelConfig {
    #[serde(default = "default_http_port")]
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackChannelConfig {
    pub bot_token: String,
    pub app_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordChannelConfig {
    pub bot_token: String,
}

impl Config {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }

    pub fn default_config() -> Self {
        Config {
            data_dir: default_data_dir(),
            gateway: GatewayConfig::default(),
            agents: vec![AgentConfig {
                name: "main".to_string(),
                persona: None,
                trust_level: "standard".to_string(),
                llm: LlmConfig::default(),
            }],
            channels: ChannelsConfig::default(),
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
            embedding_model: None,
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
    "local".to_string()
}

fn default_true() -> bool {
    true
}
