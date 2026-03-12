use anyhow::Result;
use mp_core::config::Config;
use std::path::Path;

use crate::helpers::{ensure_embedding_models, op_request, seed_bootstrap_facts};
use crate::ui;

pub async fn run(config_path: &str) -> Result<()> {
    let path = Path::new(config_path);
    if path.exists() {
        anyhow::bail!("{config_path} already exists. Delete it first to re-initialize.");
    }

    let config = Config::default_config();
    let toml_str = config.to_toml()?;

    std::fs::write(path, &toml_str)?;
    std::fs::create_dir_all(&config.data_dir)?;
    std::fs::create_dir_all(config.models_dir())?;

    let meta_path = config.metadata_db_path();
    let meta_conn = mp_core::db::open(&meta_path)?;
    mp_core::schema::init_metadata_db(&meta_conn)?;

    let bootstrap_conn = {
        let conn = mp_core::db::open_memory()?;
        mp_core::schema::init_agent_db(&conn)?;
        conn.execute(
            "INSERT INTO policies (id, brain_id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, created_at)
             VALUES ('allow-bootstrap', 'bootstrap', 'allow bootstrap', 1000, 'allow', '*', '*', '*', ?1)",
            [chrono::Utc::now().timestamp()],
        )?;
        conn
    };

    for agent in &config.agents {
        let req = op_request(
            "bootstrap",
            "agent.create",
            serde_json::json!({
                "name": agent.name,
                "persona": agent.persona,
                "trust_level": agent.trust_level,
                "llm_provider": agent.llm.provider,
                "llm_model": agent.llm.model,
                "metadata_db_path": meta_path.to_string_lossy().to_string(),
                "agent_db_path": config.agent_db_path(&agent.name).to_string_lossy().to_string(),
            }),
        );
        let resp = mp_core::operations::execute(&bootstrap_conn, &req)?;
        if !resp.ok && resp.code != "already_exists" {
            anyhow::bail!(
                "failed to initialize agent '{}': {}",
                agent.name,
                resp.message
            );
        }
    }

    for agent in &config.agents {
        let agent_db_path = config.agent_db_path(&agent.name);
        if let Ok(agent_conn) = mp_core::db::open(&agent_db_path) {
            let _ = mp_core::schema::init_agent_db(&agent_conn);
            seed_bootstrap_facts(&agent_conn, &agent.name);
        }
    }

    ui::banner();
    ui::info(format!("Creating project in {}", config.data_dir.display()));
    ui::blank();
    ui::success(format!("Created {config_path}"));
    ui::success("Created data directory");
    ui::success("Created models directory");
    for agent in &config.agents {
        ui::success(format!("Initialized agent \"{}\"", agent.name));
        ui::detail(format!(
            "Embedding: {} ({}, {}D)",
            agent.embedding.provider, agent.embedding.model, agent.embedding.dimensions
        ));
    }
    ui::success("Seeded bootstrap facts");
    ui::blank();

    let spinner = ui::spinner("Downloading embedding models...");
    ensure_embedding_models(&config).await;
    spinner.finish_and_clear();

    ui::blank();
    ui::info("Ready! Next steps:");
    ui::blank();
    ui::hint("mp setup cursor --local            # register with Cursor");
    ui::hint("mp setup cortex                    # register with Cortex Code CLI");
    ui::hint("mp setup claude-code               # register with Claude Code");
    ui::blank();
    ui::info("Then ask your agent: \"What Moneypenny tools do you have?\"");
    ui::blank();
    ui::info("CLI agent:");
    ui::hint("mp chat                            # interactive terminal chat");
    ui::hint("mp send main \"remember X\"          # one-shot message");
    ui::blank();
    ui::info("Tip: Set ANTHROPIC_API_KEY in .env for LLM features (mp chat, mp send).");
    ui::blank();

    Ok(())
}
