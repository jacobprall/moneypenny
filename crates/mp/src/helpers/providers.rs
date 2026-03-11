use anyhow::Result;
use mp_core::config::Config;

pub fn build_provider(
    agent: &mp_core::config::AgentConfig,
) -> Result<Box<dyn mp_llm::provider::LlmProvider>> {
    mp_llm::build_provider(
        &agent.llm.provider,
        agent.llm.api_base.as_deref(),
        agent.llm.api_key.as_deref(),
        agent.llm.model.as_deref(),
    )
    .map_err(|e| {
        let provider = &agent.llm.provider;
        let hint = match provider.as_str() {
            "anthropic" => "Fix: set ANTHROPIC_API_KEY in your environment or .env file.",
            "openai" => "Fix: set OPENAI_API_KEY in your environment or .env file.",
            _ => "Fix: check the LLM provider configuration in moneypenny.toml.",
        };
        anyhow::anyhow!("LLM provider '{provider}' failed to initialize: {e}\n{hint}")
    })
}

pub fn build_embedding_provider(
    config: &Config,
    agent: &mp_core::config::AgentConfig,
) -> Result<Box<dyn mp_llm::provider::EmbeddingProvider>> {
    let model_path = agent.embedding.resolve_model_path(&config.models_dir());
    mp_llm::build_embedding_provider(
        &agent.embedding.provider,
        &agent.embedding.model,
        &model_path,
        agent.embedding.dimensions,
        agent.embedding.api_base.as_deref(),
        agent.embedding.api_key.as_deref(),
    )
}

pub fn build_embedding_provider_with_override(
    config: &Config,
    agent: &mp_core::config::AgentConfig,
    model_override: Option<&str>,
) -> Result<Box<dyn mp_llm::provider::EmbeddingProvider>> {
    let mut embed_cfg = agent.embedding.clone();
    if let Some(model) = model_override {
        embed_cfg.model = model.to_string();
        embed_cfg.model_path = None;
    }
    let model_path = embed_cfg.resolve_model_path(&config.models_dir());
    mp_llm::build_embedding_provider(
        &embed_cfg.provider,
        &embed_cfg.model,
        &model_path,
        embed_cfg.dimensions,
        embed_cfg.api_base.as_deref(),
        embed_cfg.api_key.as_deref(),
    )
}

pub fn embedding_model_id(agent: &mp_core::config::AgentConfig) -> String {
    mp_core::store::embedding::model_identity(
        &agent.embedding.provider,
        &agent.embedding.model,
        agent.embedding.dimensions,
    )
}
