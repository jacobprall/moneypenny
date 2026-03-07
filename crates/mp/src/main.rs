mod cli;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Command};
use mp_core::config::Config;
use std::path::Path;
use tracing_subscriber::{EnvFilter, fmt};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    if matches!(cli.command, Command::Init) {
        return cmd_init(&cli.config).await;
    }

    let config_path = Path::new(&cli.config);
    let config = Config::load(config_path).unwrap_or_else(|e| {
        eprintln!(
            "Failed to load config from {}: {e}\nRun `mp init` to create a config file.",
            cli.config
        );
        std::process::exit(1);
    });

    init_logging(&config.gateway.log_level);

    match cli.command {
        Command::Init => unreachable!(),
        Command::Start => cmd_start(&config).await,
        Command::Stop => cmd_stop(&config).await,
        Command::Agent(cmd) => cmd_agent(&config, cmd).await,
        Command::Chat { agent } => cmd_chat(&config, agent).await,
        Command::Send { agent, message } => cmd_send(&config, &agent, &message).await,
        Command::Facts(cmd) => cmd_facts(&config, cmd).await,
        Command::Ingest { path, url, agent } => cmd_ingest(&config, path, url, agent).await,
        Command::Knowledge(cmd) => cmd_knowledge(&config, cmd).await,
        Command::Skill(cmd) => cmd_skill(&config, cmd).await,
        Command::Policy(cmd) => cmd_policy(&config, cmd).await,
        Command::Job(cmd) => cmd_job(&config, cmd).await,
        Command::Audit { agent, command } => cmd_audit(&config, agent, command).await,
        Command::Sync(cmd) => cmd_sync(&config, cmd).await,
        Command::Db(cmd) => cmd_db(&config, cmd).await,
        Command::Health => cmd_health(&config).await,
    }
}

fn init_logging(level: &str) {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(level));

    fmt()
        .with_env_filter(filter)
        .with_target(true)
        .init();
}

async fn cmd_init(config_path: &str) -> Result<()> {
    let path = Path::new(config_path);
    if path.exists() {
        anyhow::bail!("{config_path} already exists. Delete it first to re-initialize.");
    }

    let config = Config::default_config();
    let toml_str = config.to_toml()?;

    // Write config file
    std::fs::write(path, &toml_str)?;

    // Create data directory
    std::fs::create_dir_all(&config.data_dir)?;

    // Initialize the metadata database
    let meta_path = config.metadata_db_path();
    let _conn = mp_core::db::open(&meta_path)?;

    // Initialize the default agent database
    for agent in &config.agents {
        let db_path = config.agent_db_path(&agent.name);
        let _conn = mp_core::db::open(&db_path)?;
    }

    println!();
    println!("  Moneypenny v{}", env!("CARGO_PKG_VERSION"));
    println!();
    println!("  Creating project in {}", config.data_dir.display());
    println!();
    println!("  \u{2713} Created {config_path}");
    println!("  \u{2713} Created data directory");
    for agent in &config.agents {
        println!("  \u{2713} Initialized agent \"{}\"", agent.name);
    }
    println!();
    println!("  Ready. Run `mp start` to begin.");
    println!();

    Ok(())
}

async fn cmd_start(config: &Config) -> Result<()> {
    println!();
    println!("  Moneypenny v{}", env!("CARGO_PKG_VERSION"));
    println!();
    println!("  Starting gateway on {}:{}", config.gateway.host, config.gateway.port);
    for agent in &config.agents {
        println!("  Starting agent \"{}\"...", agent.name);
    }
    println!();
    println!("  [not yet implemented — this is the M1 scaffold]");
    println!();
    Ok(())
}

async fn cmd_stop(_config: &Config) -> Result<()> {
    println!("  Sending shutdown signal...");
    println!("  [not yet implemented]");
    Ok(())
}

async fn cmd_agent(_config: &Config, cmd: cli::AgentCommand) -> Result<()> {
    match cmd {
        cli::AgentCommand::List => println!("  [mp agent list — not yet implemented]"),
        cli::AgentCommand::Create { name } => {
            println!("  [mp agent create {name} — not yet implemented]")
        }
        cli::AgentCommand::Delete { name, .. } => {
            println!("  [mp agent delete {name} — not yet implemented]")
        }
        cli::AgentCommand::Status { name } => {
            let label = name.as_deref().unwrap_or("all");
            println!("  [mp agent status {label} — not yet implemented]");
        }
        cli::AgentCommand::Config { name, key, value } => {
            println!("  [mp agent config {name} {key}={value} — not yet implemented]");
        }
    }
    Ok(())
}

async fn cmd_chat(_config: &Config, agent: Option<String>) -> Result<()> {
    let name = agent.as_deref().unwrap_or("main");
    println!("  [mp chat {name} — not yet implemented]");
    Ok(())
}

async fn cmd_send(_config: &Config, agent: &str, message: &str) -> Result<()> {
    println!("  [mp send {agent} \"{message}\" — not yet implemented]");
    Ok(())
}

async fn cmd_facts(_config: &Config, cmd: cli::FactsCommand) -> Result<()> {
    match cmd {
        cli::FactsCommand::List { .. } => println!("  [mp facts list — not yet implemented]"),
        cli::FactsCommand::Search { query, .. } => {
            println!("  [mp facts search \"{query}\" — not yet implemented]")
        }
        cli::FactsCommand::Inspect { id } => {
            println!("  [mp facts inspect {id} — not yet implemented]")
        }
        cli::FactsCommand::Promote { id, scope } => {
            println!("  [mp facts promote {id} --scope {scope} — not yet implemented]")
        }
        cli::FactsCommand::Delete { id, .. } => {
            println!("  [mp facts delete {id} — not yet implemented]")
        }
    }
    Ok(())
}

async fn cmd_ingest(
    _config: &Config,
    path: Option<String>,
    url: Option<String>,
    _agent: Option<String>,
) -> Result<()> {
    if let Some(p) = path {
        println!("  [mp ingest {p} — not yet implemented]");
    } else if let Some(u) = url {
        println!("  [mp ingest --url {u} — not yet implemented]");
    } else {
        anyhow::bail!("Provide a path or --url to ingest.");
    }
    Ok(())
}

async fn cmd_knowledge(_config: &Config, cmd: cli::KnowledgeCommand) -> Result<()> {
    match cmd {
        cli::KnowledgeCommand::Search { query } => {
            println!("  [mp knowledge search \"{query}\" — not yet implemented]")
        }
        cli::KnowledgeCommand::List => println!("  [mp knowledge list — not yet implemented]"),
    }
    Ok(())
}

async fn cmd_skill(_config: &Config, cmd: cli::SkillCommand) -> Result<()> {
    match cmd {
        cli::SkillCommand::Add { path, .. } => {
            println!("  [mp skill add {path} — not yet implemented]")
        }
        cli::SkillCommand::List { .. } => println!("  [mp skill list — not yet implemented]"),
        cli::SkillCommand::Promote { id } => {
            println!("  [mp skill promote {id} — not yet implemented]")
        }
    }
    Ok(())
}

async fn cmd_policy(_config: &Config, cmd: cli::PolicyCommand) -> Result<()> {
    match cmd {
        cli::PolicyCommand::List => println!("  [mp policy list — not yet implemented]"),
        cli::PolicyCommand::Add { name, .. } => {
            println!("  [mp policy add --name \"{name}\" — not yet implemented]")
        }
        cli::PolicyCommand::Test { input } => {
            println!("  [mp policy test \"{input}\" — not yet implemented]")
        }
        cli::PolicyCommand::Violations { last } => {
            println!("  [mp policy violations --last {last} — not yet implemented]")
        }
        cli::PolicyCommand::Load { file } => {
            println!("  [mp policy load {file} — not yet implemented]")
        }
    }
    Ok(())
}

async fn cmd_job(_config: &Config, cmd: cli::JobCommand) -> Result<()> {
    match cmd {
        cli::JobCommand::List { .. } => println!("  [mp job list — not yet implemented]"),
        cli::JobCommand::Create { name, .. } => {
            println!("  [mp job create --name \"{name}\" — not yet implemented]")
        }
        cli::JobCommand::Run { id } => println!("  [mp job run {id} — not yet implemented]"),
        cli::JobCommand::Pause { id } => println!("  [mp job pause {id} — not yet implemented]"),
        cli::JobCommand::History { id } => {
            let label = id.as_deref().unwrap_or("all");
            println!("  [mp job history {label} — not yet implemented]");
        }
    }
    Ok(())
}

async fn cmd_audit(
    _config: &Config,
    agent: Option<String>,
    command: Option<cli::AuditCommand>,
) -> Result<()> {
    match command {
        None => {
            let name = agent.as_deref().unwrap_or("all");
            println!("  [mp audit {name} — not yet implemented]");
        }
        Some(cli::AuditCommand::Search { query }) => {
            println!("  [mp audit search \"{query}\" — not yet implemented]")
        }
        Some(cli::AuditCommand::Export { format }) => {
            println!("  [mp audit export --format {format} — not yet implemented]")
        }
    }
    Ok(())
}

async fn cmd_sync(_config: &Config, cmd: cli::SyncCommand) -> Result<()> {
    match cmd {
        cli::SyncCommand::Status => println!("  [mp sync status — not yet implemented]"),
        cli::SyncCommand::Now { agent } => {
            let name = agent.as_deref().unwrap_or("all");
            println!("  [mp sync now {name} — not yet implemented]");
        }
        cli::SyncCommand::Connect { url } => {
            println!("  [mp sync connect {url} — not yet implemented]")
        }
    }
    Ok(())
}

async fn cmd_db(_config: &Config, cmd: cli::DbCommand) -> Result<()> {
    match cmd {
        cli::DbCommand::Query { sql, .. } => {
            println!("  [mp db query \"{sql}\" — not yet implemented]")
        }
        cli::DbCommand::Schema { .. } => println!("  [mp db schema — not yet implemented]"),
    }
    Ok(())
}

async fn cmd_health(config: &Config) -> Result<()> {
    println!();
    println!("  Moneypenny v{}", env!("CARGO_PKG_VERSION"));
    println!();

    let meta_path = config.metadata_db_path();
    if meta_path.exists() {
        println!("  Gateway:  data dir exists at {}", config.data_dir.display());
    } else {
        println!("  Gateway:  not initialized (run `mp init`)");
    }

    for agent in &config.agents {
        let db_path = config.agent_db_path(&agent.name);
        if db_path.exists() {
            let metadata = std::fs::metadata(&db_path)?;
            let size_kb = metadata.len() / 1024;
            println!("  Agent \"{}\": db exists ({size_kb} KB)", agent.name);
        } else {
            println!("  Agent \"{}\": not initialized", agent.name);
        }
    }

    println!();
    Ok(())
}
