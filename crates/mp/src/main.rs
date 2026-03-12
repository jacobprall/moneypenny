use anyhow::Result;
use mp::{
    cli::{Cli, Command},
    commands,
    sidecar,
    worker,
};
use clap::Parser;
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

    let ctx = commands::CommandContext::new(&config, config_path)?;

    match cli.command {
        Command::Init => unreachable!(),
        Command::Start => commands::start::run(&ctx).await,
        Command::Serve { agent } => commands::serve::run(&ctx, agent).await,
        Command::Stop => commands::stop::run(&ctx).await,
        Command::Agent(cmd) => commands::agent::run(&ctx, cmd).await,
        Command::Brain(cmd) => commands::brain::run(&ctx, cmd).await,
        Command::Experience(cmd) => commands::experience::run(&ctx, cmd).await,
        Command::Focus(cmd) => commands::focus::run(&ctx, cmd).await,
        Command::Chat {
            agent,
            session_id,
            new,
            tui,
            verbose,
            quiet,
        } => commands::chat::run(&ctx, agent, session_id, new, tui, verbose, quiet).await,
        Command::Send {
            agent,
            message,
            session_id,
            verbose,
            quiet,
        } => commands::send::run(&ctx, &agent, &message, session_id, verbose, quiet).await,
        Command::Facts(cmd) => commands::facts::run(&ctx, cmd).await,
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
            commands::ingest::run(&ctx, &args).await
        }
        Command::Session(cmd) => commands::session::run(&ctx, cmd).await,
        Command::Knowledge(cmd) => commands::knowledge::run(&ctx, cmd).await,
        Command::Skill(cmd) => commands::skill::run(&ctx, cmd).await,
        Command::Policy(cmd) => commands::policy::run(&ctx, cmd).await,
        Command::Job(cmd) => commands::job::run(&ctx, cmd).await,
        Command::Embeddings(cmd) => commands::embeddings::run(&ctx, cmd).await,
        Command::Audit { agent, command } => commands::audit::run(&ctx, agent, command).await,
        Command::Sync(cmd) => commands::sync::run(&ctx, cmd).await,
        Command::Fleet(cmd) => commands::fleet::run(&ctx, cmd).await,
        Command::Mpq { expression, agent, dry_run } => {
            commands::mpq::run(&ctx, &expression, agent, dry_run).await
        }
        Command::Db(cmd) => commands::db::run(&ctx, cmd).await,
        Command::Spend { agent, period, group_by } => {
            commands::spend::run(&ctx, agent, &period, &group_by).await
        }
        Command::Briefing { agent } => commands::briefing::run(&ctx, agent).await,
        Command::Health => commands::health::run(&ctx).await,
        Command::Doctor => commands::doctor::run(&ctx).await,
        Command::Worker { agent } => worker::cmd_worker(ctx.config, &agent).await,
        Command::Sidecar { agent } => sidecar::cmd_sidecar(ctx.config, agent).await,
        Command::Setup(cmd) => commands::setup::run(&ctx, cmd).await,
        Command::Hook { event, agent } => commands::hook::run(&ctx, &event, agent).await,
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
