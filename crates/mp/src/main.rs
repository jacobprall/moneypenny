pub mod agent;
mod adapters;
mod cli;
mod commands;
mod context;
mod domain_tools;
mod tools;
mod docs;
pub mod helpers;
mod sidecar;
mod ui;
pub mod worker;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Command};
use mp_core::config::Config;
use std::path::Path;
use tracing_subscriber::{EnvFilter, fmt};

#[tokio::main]
async fn main() -> Result<()> {
    let _ = dotenvy::dotenv();
    let cli = Cli::parse();

    if matches!(cli.command, Command::Init) {
        return commands::init::run(&cli.config).await;
    }

    let config_path = Path::new(&cli.config);

    // Also load .env from the config file's directory so env vars are found
    // regardless of process cwd (e.g. when launched as an MCP server).
    if let Some(config_dir) = config_path.parent().and_then(|p| std::fs::canonicalize(p).ok()) {
        let _ = dotenvy::from_path(config_dir.join(".env"));
    }

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
        Command::Start => commands::start::run(&config, config_path).await,
        Command::Serve { agent } => commands::serve::run(&config, config_path, agent).await,
        Command::Stop => commands::stop::run(&config).await,
        Command::Agent(cmd) => commands::agent::run(&config, cmd).await,
        Command::Chat { agent, session_id, new } => {
            commands::chat::run(&config, agent, session_id, new).await
        }
        Command::Send {
            agent,
            message,
            session_id,
        } => commands::send::run(&config, &agent, &message, session_id).await,
        Command::Facts(cmd) => commands::facts::run(&config, cmd).await,
        Command::Ingest {
            path,
            url,
            agent,
            openclaw_file,
            replay,
            status,
            replay_run,
            replay_latest,
            replay_offset,
            status_filter,
            file_filter,
            dry_run,
            apply,
            source,
            limit,
            cortex,
            claude_code,
            cursor,
        } => {
            let args = commands::ingest::IngestArgs {
                path,
                url,
                agent,
                openclaw_file,
                replay,
                status,
                replay_run,
                replay_latest,
                replay_offset,
                status_filter,
                file_filter,
                dry_run,
                apply,
                source,
                limit,
                cortex,
                claude_code,
                cursor,
            };
            commands::ingest::run(&config, &args).await
        }
        Command::Session(cmd) => commands::session::run(&config, cmd).await,
        Command::Knowledge(cmd) => commands::knowledge::run(&config, cmd).await,
        Command::Skill(cmd) => commands::skill::run(&config, cmd).await,
        Command::Policy(cmd) => commands::policy::run(&config, cmd).await,
        Command::Job(cmd) => commands::job::run(&config, cmd).await,
        Command::Embeddings(cmd) => commands::embeddings::run(&config, cmd).await,
        Command::Audit { agent, command } => {
            commands::audit::run(&config, agent, command).await
        }
        Command::Sync(cmd) => commands::sync::run(&config, cmd).await,
        Command::Fleet(cmd) => commands::fleet::run(&config, cmd).await,
        Command::Mpq { expression, agent, dry_run } => {
            commands::mpq::run(&config, &expression, agent, dry_run).await
        }
        Command::Db(cmd) => commands::db::run(&config, cmd).await,
        Command::Health => commands::health::run(&config).await,
        Command::Doctor => commands::doctor::run(&config, config_path).await,
        Command::Worker { agent } => worker::cmd_worker(&config, &agent).await,
        Command::Sidecar { agent } => sidecar::cmd_sidecar(&config, agent).await,
        Command::Setup(cmd) => commands::setup::run(&config, config_path, cmd).await,
        Command::Hook { event, agent } => commands::hook::run(&config, &event, agent).await,
    }
}

fn init_logging(level: &str) {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level));

    fmt()
        .with_env_filter(filter)
        .with_target(true)
        .with_writer(std::io::stderr)
        .init();
}
