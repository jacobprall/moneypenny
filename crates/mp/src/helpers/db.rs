use anyhow::Result;
use mp_core::config::Config;

pub fn resolve_agent<'a>(
    config: &'a Config,
    name: Option<&str>,
) -> Result<&'a mp_core::config::AgentConfig> {
    match name {
        Some(n) => config
            .agents
            .iter()
            .find(|a| a.name == n)
            .ok_or_else(|| {
                let available: Vec<&str> = config.agents.iter().map(|a| a.name.as_str()).collect();
                if available.is_empty() {
                    anyhow::anyhow!(
                        "Agent '{n}' not found — no agents configured.\n\
                         Fix: run `mp init` to create a default configuration."
                    )
                } else {
                    anyhow::anyhow!(
                        "Agent '{n}' not found. Available agents: {}\n\
                         Fix: use one of the names above, or add '{n}' to moneypenny.toml.",
                        available.join(", ")
                    )
                }
            }),
        None => config
            .agents
            .first()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "No agents configured in moneypenny.toml.\n\
                     Fix: run `mp init` to create a default configuration with a starter agent."
                )
            }),
    }
}

pub fn open_agent_db(config: &Config, agent_name: &str) -> Result<rusqlite::Connection> {
    let db_path = config.agent_db_path(agent_name);
    let conn = mp_core::db::open(&db_path).map_err(|e| {
        if !db_path.exists() {
            anyhow::anyhow!(
                "Agent database not found at {}\n\
                 Fix: run `mp init` to initialize the project and create agent databases.",
                db_path.display()
            )
        } else {
            anyhow::anyhow!(
                "Failed to open agent database at {}: {e}\n\
                 Fix: run `mp doctor` to diagnose the issue.",
                db_path.display()
            )
        }
    })?;
    // Load extensions before init_agent_db — migrations may touch synced tables
    // whose triggers call cloudsync_is_sync
    mp_ext::init_all_extensions(&conn)?;
    mp_core::schema::init_agent_db(&conn)?;
    if let Some(agent) = config.agents.iter().find(|a| a.name == agent_name) {
        let _ = mp_core::schema::init_vector_indexes(&conn, agent.embedding.dimensions);
        if let Err(e) = mp_core::schema::init_sync_tables(&conn) {
            tracing::warn!(agent = agent_name, "sync table init warning: {e}");
        }
        if !agent.mcp_servers.is_empty() {
            match mp_core::mcp::discover_and_register(&conn, &agent.mcp_servers) {
                Ok(n) if n > 0 => {
                    tracing::info!(agent = agent_name, tools = n, "MCP tools registered")
                }
                Ok(_) => {}
                Err(e) => tracing::warn!(agent = agent_name, "MCP discovery error: {e}"),
            }
        }
    }
    Ok(conn)
}
