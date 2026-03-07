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

fn resolve_agent<'a>(config: &'a Config, name: Option<&str>) -> Result<&'a mp_core::config::AgentConfig> {
    match name {
        Some(n) => config.agents.iter().find(|a| a.name == n)
            .ok_or_else(|| anyhow::anyhow!("Agent '{n}' not found in config")),
        None => config.agents.first()
            .ok_or_else(|| anyhow::anyhow!("No agents configured")),
    }
}

fn open_agent_db(config: &Config, agent_name: &str) -> Result<rusqlite::Connection> {
    let db_path = config.agent_db_path(agent_name);
    let conn = mp_core::db::open(&db_path)?;
    mp_ext::init_all_extensions(&conn)?;
    Ok(conn)
}

// =========================================================================
// Init
// =========================================================================

async fn cmd_init(config_path: &str) -> Result<()> {
    let path = Path::new(config_path);
    if path.exists() {
        anyhow::bail!("{config_path} already exists. Delete it first to re-initialize.");
    }

    let config = Config::default_config();
    let toml_str = config.to_toml()?;

    std::fs::write(path, &toml_str)?;
    std::fs::create_dir_all(&config.data_dir)?;

    let meta_path = config.metadata_db_path();
    let meta_conn = mp_core::db::open(&meta_path)?;
    mp_core::schema::init_metadata_db(&meta_conn)?;

    for agent in &config.agents {
        let db_path = config.agent_db_path(&agent.name);
        let agent_conn = mp_core::db::open(&db_path)?;
        mp_ext::init_all_extensions(&agent_conn)?;
        mp_core::schema::init_agent_db(&agent_conn)?;

        // Register built-in tools and runtime skills
        mp_core::tools::registry::register_builtins(&agent_conn)?;
        mp_core::tools::registry::register_runtime_skills(&agent_conn)?;

        meta_conn.execute(
            "INSERT OR IGNORE INTO agents (id, name, persona, trust_level, llm_provider, db_path, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, strftime('%s', 'now'))",
            rusqlite::params![
                agent.name,
                agent.name,
                agent.persona,
                agent.trust_level,
                agent.llm.provider,
                db_path.to_string_lossy(),
            ],
        )?;
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

// =========================================================================
// Start / Stop
// =========================================================================

async fn cmd_start(config: &Config) -> Result<()> {
    println!();
    println!("  Moneypenny v{}", env!("CARGO_PKG_VERSION"));
    println!();
    println!("  Starting gateway on {}:{}", config.gateway.host, config.gateway.port);
    for agent in &config.agents {
        println!("  Starting agent \"{}\"...", agent.name);
    }
    println!();
    println!("  [Gateway loop not yet implemented — requires M13]");
    println!();
    Ok(())
}

async fn cmd_stop(_config: &Config) -> Result<()> {
    println!("  Sending shutdown signal...");
    println!("  [not yet implemented]");
    Ok(())
}

// =========================================================================
// Agent
// =========================================================================

async fn cmd_agent(config: &Config, cmd: cli::AgentCommand) -> Result<()> {
    match cmd {
        cli::AgentCommand::List => {
            println!();
            println!("  {:20} {:15} {:15}", "NAME", "TRUST", "LLM");
            println!("  {:20} {:15} {:15}", "----", "-----", "---");
            for agent in &config.agents {
                println!("  {:20} {:15} {:15}",
                    agent.name, agent.trust_level, agent.llm.provider);
            }
            println!();
        }
        cli::AgentCommand::Create { name } => {
            println!("  [mp agent create {name} — requires runtime agent registry (M13)]");
        }
        cli::AgentCommand::Delete { name, .. } => {
            println!("  [mp agent delete {name} — requires runtime agent registry (M13)]");
        }
        cli::AgentCommand::Status { name } => {
            let agent = resolve_agent(config, name.as_deref())?;
            let conn = open_agent_db(config, &agent.name)?;

            let fact_count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM facts WHERE superseded_at IS NULL", [], |r| r.get(0)
            ).unwrap_or(0);
            let session_count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM sessions", [], |r| r.get(0)
            ).unwrap_or(0);
            let doc_count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM documents", [], |r| r.get(0)
            ).unwrap_or(0);
            let skill_count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM skills", [], |r| r.get(0)
            ).unwrap_or(0);

            println!();
            println!("  Agent: {}", agent.name);
            println!("  Trust: {}", agent.trust_level);
            println!("  LLM:   {} ({})", agent.llm.provider, agent.llm.model.as_deref().unwrap_or("default"));
            println!();
            println!("  Facts:     {fact_count}");
            println!("  Sessions:  {session_count}");
            println!("  Documents: {doc_count}");
            println!("  Skills:    {skill_count}");
            println!();
        }
        cli::AgentCommand::Config { name, key, value } => {
            println!("  [mp agent config {name} {key}={value} — not yet implemented]");
        }
    }
    Ok(())
}

// =========================================================================
// Chat & Send
// =========================================================================

async fn cmd_chat(_config: &Config, agent: Option<String>) -> Result<()> {
    let name = agent.as_deref().unwrap_or("main");
    println!();
    println!("  Moneypenny chat — agent: {name}");
    println!("  Type /help for commands, Ctrl-C to exit.");
    println!("  [Interactive chat requires async LLM integration — M10 agent loop is ready]");
    println!();
    Ok(())
}

async fn cmd_send(config: &Config, agent_name: &str, message: &str) -> Result<()> {
    let agent = resolve_agent(config, Some(agent_name))?;
    let conn = open_agent_db(config, &agent.name)?;

    let sid = mp_core::store::log::create_session(&conn, &agent.name, Some("cli"))?;
    mp_core::store::log::append_message(&conn, &sid, "user", message)?;

    println!();
    println!("  Message stored in session {sid}");
    println!("  [LLM response requires provider connection — agent loop is ready in mp-core]");
    println!();
    Ok(())
}

// =========================================================================
// Facts
// =========================================================================

async fn cmd_facts(config: &Config, cmd: cli::FactsCommand) -> Result<()> {
    match cmd {
        cli::FactsCommand::List { agent } => {
            let ag = resolve_agent(config, agent.as_deref())?;
            let conn = open_agent_db(config, &ag.name)?;
            let facts = mp_core::store::facts::list_active(&conn, &ag.name)?;

            println!();
            if facts.is_empty() {
                println!("  No facts found for agent \"{}\".", ag.name);
            } else {
                println!("  {:36} {:6} {:50}", "ID", "CONF", "POINTER");
                println!("  {:36} {:6} {:50}", "--", "----", "-------");
                for f in &facts {
                    println!("  {:36} {:<6.1} {}", f.id, f.confidence, f.pointer);
                }
                println!();
                println!("  {} active facts", facts.len());
            }
            println!();
        }
        cli::FactsCommand::Search { query, agent } => {
            let ag = resolve_agent(config, agent.as_deref())?;
            let conn = open_agent_db(config, &ag.name)?;
            let results = mp_core::search::search(&conn, &query, &ag.name, 20, None)?;

            println!();
            if results.is_empty() {
                println!("  No results for \"{query}\".");
            } else {
                for r in &results {
                    let preview: String = r.content.chars().take(80).collect();
                    println!("  [{:?}] {:.4}  {}", r.store, r.score, preview);
                }
                println!();
                println!("  {} results", results.len());
            }
            println!();
        }
        cli::FactsCommand::Inspect { id } => {
            let ag = resolve_agent(config, None)?;
            let conn = open_agent_db(config, &ag.name)?;

            match mp_core::store::facts::get(&conn, &id)? {
                None => println!("  Fact {id} not found."),
                Some(f) => {
                    println!();
                    println!("  ID:         {}", f.id);
                    println!("  Pointer:    {}", f.pointer);
                    println!("  Summary:    {}", f.summary);
                    println!("  Confidence: {:.1}", f.confidence);
                    println!("  Version:    {}", f.version);
                    println!();
                    println!("  Content:");
                    println!("  {}", f.content);
                    println!();

                    let audit = mp_core::store::facts::get_audit(&conn, &id)?;
                    if !audit.is_empty() {
                        println!("  Audit trail:");
                        for a in &audit {
                            println!("    {} — {}", a.operation, a.reason.as_deref().unwrap_or(""));
                        }
                    }
                    println!();
                }
            }
        }
        cli::FactsCommand::Promote { id, scope } => {
            println!("  [mp facts promote {id} --scope {scope} — requires sync (M13)]");
        }
        cli::FactsCommand::Delete { id, confirm } => {
            let ag = resolve_agent(config, None)?;
            let conn = open_agent_db(config, &ag.name)?;

            if !confirm {
                println!("  Use --confirm to delete fact {id}");
            } else {
                mp_core::store::facts::delete(&conn, &id, Some("deleted via CLI"))?;
                println!("  Fact {id} deleted.");
            }
        }
    }
    Ok(())
}

// =========================================================================
// Ingest
// =========================================================================

async fn cmd_ingest(
    config: &Config,
    path: Option<String>,
    url: Option<String>,
    agent: Option<String>,
) -> Result<()> {
    let ag = resolve_agent(config, agent.as_deref())?;
    let conn = open_agent_db(config, &ag.name)?;

    if let Some(p) = path {
        let content = std::fs::read_to_string(&p)?;
        let title = Path::new(&p).file_name()
            .map(|n| n.to_string_lossy().to_string());
        let (doc_id, chunks) = mp_core::store::knowledge::ingest(
            &conn, Some(&p), title.as_deref(), &content, None,
        )?;
        println!("  Ingested {p}: {chunks} chunks (doc {doc_id})");
    } else if let Some(u) = url {
        println!("  [mp ingest --url {u} — HTTP fetch not yet implemented]");
    } else {
        anyhow::bail!("Provide a path or --url to ingest.");
    }
    Ok(())
}

// =========================================================================
// Knowledge
// =========================================================================

async fn cmd_knowledge(config: &Config, cmd: cli::KnowledgeCommand) -> Result<()> {
    let ag = resolve_agent(config, None)?;
    let conn = open_agent_db(config, &ag.name)?;

    match cmd {
        cli::KnowledgeCommand::Search { query } => {
            let results = mp_core::search::fts5_search_knowledge(&conn, &query, 20)?;
            println!();
            if results.is_empty() {
                println!("  No knowledge results for \"{query}\".");
            } else {
                for (id, content, _score) in &results {
                    let preview: String = content.chars().take(80).collect();
                    println!("  {id}: {preview}");
                }
            }
            println!();
        }
        cli::KnowledgeCommand::List => {
            let docs = mp_core::store::knowledge::list_documents(&conn)?;
            println!();
            if docs.is_empty() {
                println!("  No documents ingested.");
            } else {
                println!("  {:36} {:30} {:20}", "ID", "TITLE", "PATH");
                println!("  {:36} {:30} {:20}", "--", "-----", "----");
                for d in &docs {
                    println!("  {:36} {:30} {:20}",
                        d.id,
                        d.title.as_deref().unwrap_or("-"),
                        d.path.as_deref().unwrap_or("-"),
                    );
                }
            }
            println!();
        }
    }
    Ok(())
}

// =========================================================================
// Skill
// =========================================================================

async fn cmd_skill(config: &Config, cmd: cli::SkillCommand) -> Result<()> {
    let ag = resolve_agent(config, None)?;
    let conn = open_agent_db(config, &ag.name)?;

    match cmd {
        cli::SkillCommand::Add { path, .. } => {
            let content = std::fs::read_to_string(&path)?;
            let name = Path::new(&path).file_stem()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "unnamed".into());
            let id = mp_core::store::knowledge::add_skill(
                &conn, &name, &format!("Skill from {path}"), &content, None,
            )?;
            println!("  Added skill \"{name}\" ({id})");
        }
        cli::SkillCommand::List { .. } => {
            let mut stmt = conn.prepare(
                "SELECT id, name, usage_count, success_rate, promoted FROM skills ORDER BY usage_count DESC"
            )?;
            let skills: Vec<(String, String, i64, Option<f64>, bool)> = stmt.query_map([], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get::<_, i64>(4)? != 0))
            })?.collect::<Result<Vec<_>, _>>()?;

            println!();
            if skills.is_empty() {
                println!("  No skills registered.");
            } else {
                println!("  {:36} {:20} {:6} {:8} {:8}", "ID", "NAME", "USES", "RATE", "PROMO");
                println!("  {:36} {:20} {:6} {:8} {:8}", "--", "----", "----", "----", "-----");
                for (id, name, uses, rate, promoted) in &skills {
                    let rate_str = rate.map(|r| format!("{:.0}%", r * 100.0)).unwrap_or("-".into());
                    println!("  {:36} {:20} {:6} {:8} {:8}",
                        id, name, uses, rate_str, if *promoted { "yes" } else { "" });
                }
            }
            println!();
        }
        cli::SkillCommand::Promote { id } => {
            mp_core::store::knowledge::promote_skill(&conn, &id)?;
            println!("  Skill {id} promoted.");
        }
    }
    Ok(())
}

// =========================================================================
// Policy
// =========================================================================

async fn cmd_policy(config: &Config, cmd: cli::PolicyCommand) -> Result<()> {
    let ag = resolve_agent(config, None)?;
    let conn = open_agent_db(config, &ag.name)?;

    match cmd {
        cli::PolicyCommand::List => {
            let mut stmt = conn.prepare(
                "SELECT id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, enabled
                 FROM policies ORDER BY priority DESC"
            )?;
            let policies: Vec<(String, String, i64, String, Option<String>, Option<String>, Option<String>, bool)> =
                stmt.query_map([], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?,
                        r.get(4)?, r.get(5)?, r.get(6)?, r.get::<_, i64>(7)? != 0))
                })?.collect::<Result<Vec<_>, _>>()?;

            println!();
            if policies.is_empty() {
                println!("  No policies configured.");
            } else {
                println!("  {:36} {:20} {:4} {:6} {:10} {:10} {:15}",
                    "ID", "NAME", "PRI", "EFFECT", "ACTOR", "ACTION", "RESOURCE");
                for (id, name, pri, effect, actor, action, resource, _) in &policies {
                    println!("  {:36} {:20} {:4} {:6} {:10} {:10} {:15}",
                        id, name, pri, effect,
                        actor.as_deref().unwrap_or("*"),
                        action.as_deref().unwrap_or("*"),
                        resource.as_deref().unwrap_or("*"),
                    );
                }
            }
            println!();
        }
        cli::PolicyCommand::Add { name, effect, actor, action, resource, message } => {
            let id = uuid::Uuid::new_v4().to_string();
            let now = chrono::Utc::now().timestamp();
            conn.execute(
                "INSERT INTO policies (id, name, priority, effect, actor_pattern, action_pattern, resource_pattern, message, created_at)
                 VALUES (?1, ?2, 0, ?3, ?4, ?5, ?6, ?7, ?8)",
                rusqlite::params![id, name, effect, actor, action, resource, message, now],
            )?;
            println!("  Policy \"{name}\" added ({id})");
        }
        cli::PolicyCommand::Test { input } => {
            let req = mp_core::policy::PolicyRequest {
                actor: &ag.name,
                action: "execute",
                resource: "sql",
                sql_content: Some(&input),
                channel: None,
            };
            let decision = mp_core::policy::evaluate(&conn, &req)?;
            println!("  Effect: {:?}", decision.effect);
            if let Some(reason) = &decision.reason {
                println!("  Reason: {reason}");
            }
        }
        cli::PolicyCommand::Violations { last } => {
            let hours = parse_duration_hours(&last);
            let since = chrono::Utc::now().timestamp() - (hours * 3600);
            let mut stmt = conn.prepare(
                "SELECT actor, action, resource, effect, reason, created_at
                 FROM policy_audit WHERE effect = 'deny' AND created_at >= ?1
                 ORDER BY created_at DESC LIMIT 50"
            )?;
            let violations: Vec<(String, String, String, String, Option<String>, i64)> =
                stmt.query_map([since], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?))
                })?.collect::<Result<Vec<_>, _>>()?;

            println!();
            if violations.is_empty() {
                println!("  No policy violations in the last {last}.");
            } else {
                for (actor, action, resource, effect, reason, _ts) in &violations {
                    println!("  [{effect}] {actor} → {action} on {resource}: {}",
                        reason.as_deref().unwrap_or(""));
                }
            }
            println!();
        }
        cli::PolicyCommand::Load { file } => {
            println!("  [mp policy load {file} — Polar file loading not yet implemented]");
        }
    }
    Ok(())
}

fn parse_duration_hours(s: &str) -> i64 {
    if let Some(d) = s.strip_suffix('d') {
        d.parse::<i64>().unwrap_or(7) * 24
    } else if let Some(h) = s.strip_suffix('h') {
        h.parse::<i64>().unwrap_or(24)
    } else {
        168 // default: 7 days
    }
}

// =========================================================================
// Job
// =========================================================================

async fn cmd_job(config: &Config, cmd: cli::JobCommand) -> Result<()> {
    let ag = resolve_agent(config, None)?;
    let conn = open_agent_db(config, &ag.name)?;

    match cmd {
        cli::JobCommand::List { agent } => {
            let jobs = mp_core::scheduler::list_jobs(&conn, agent.as_deref())?;
            println!();
            if jobs.is_empty() {
                println!("  No jobs scheduled.");
            } else {
                println!("  {:36} {:20} {:8} {:10} {:8}", "ID", "NAME", "TYPE", "STATUS", "SCHED");
                for j in &jobs {
                    println!("  {:36} {:20} {:8} {:10} {:8}",
                        j.id, j.name, j.job_type, j.status, j.schedule);
                }
            }
            println!();
        }
        cli::JobCommand::Create { name, schedule, job_type, payload, agent } => {
            let agent_id = agent.unwrap_or_else(|| ag.name.clone());
            let now = chrono::Utc::now().timestamp();
            let id = mp_core::scheduler::create_job(&conn, &mp_core::scheduler::NewJob {
                agent_id,
                name: name.clone(),
                description: None,
                schedule,
                next_run_at: now + 60,
                job_type,
                payload,
                max_retries: None,
                retry_delay_ms: None,
                timeout_ms: None,
                overlap_policy: None,
            })?;
            println!("  Job \"{name}\" created ({id})");
        }
        cli::JobCommand::Run { id } => {
            match mp_core::scheduler::get_job(&conn, &id)? {
                None => println!("  Job {id} not found."),
                Some(job) => {
                    println!("  Triggering job \"{}\"...", job.name);
                    let run = mp_core::scheduler::dispatch_job(
                        &conn, &job,
                        &|j| Ok(format!("Manual trigger of {}", j.name)),
                    )?;
                    println!("  Run {}: {}", run.id, run.status);
                    if let Some(r) = &run.result {
                        println!("  Result: {r}");
                    }
                }
            }
        }
        cli::JobCommand::Pause { id } => {
            mp_core::scheduler::pause_job(&conn, &id)?;
            println!("  Job {id} paused.");
        }
        cli::JobCommand::History { id } => {
            let job_id = id.unwrap_or_else(|| "%".into());
            let mut stmt = conn.prepare(
                "SELECT id, job_id, status, result, started_at FROM job_runs
                 WHERE job_id LIKE ?1 ORDER BY created_at DESC LIMIT 20"
            )?;
            let runs: Vec<(String, String, String, Option<String>, i64)> =
                stmt.query_map([&job_id], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
                })?.collect::<Result<Vec<_>, _>>()?;

            println!();
            if runs.is_empty() {
                println!("  No job runs found.");
            } else {
                for (rid, jid, status, result, _ts) in &runs {
                    let preview = result.as_deref().unwrap_or("-");
                    println!("  {rid}  job:{jid}  {status}  {preview}");
                }
            }
            println!();
        }
    }
    Ok(())
}

// =========================================================================
// Audit
// =========================================================================

async fn cmd_audit(
    config: &Config,
    _agent: Option<String>,
    command: Option<cli::AuditCommand>,
) -> Result<()> {
    let ag = resolve_agent(config, None)?;
    let conn = open_agent_db(config, &ag.name)?;

    match command {
        None => {
            let mut stmt = conn.prepare(
                "SELECT actor, action, resource, effect, reason, created_at
                 FROM policy_audit ORDER BY created_at DESC LIMIT 20"
            )?;
            let entries: Vec<(String, String, String, String, Option<String>, i64)> =
                stmt.query_map([], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?))
                })?.collect::<Result<Vec<_>, _>>()?;

            println!();
            if entries.is_empty() {
                println!("  No audit entries.");
            } else {
                for (actor, action, resource, effect, reason, _ts) in &entries {
                    println!("  [{effect}] {actor} → {action} on {resource}: {}",
                        reason.as_deref().unwrap_or(""));
                }
            }
            println!();
        }
        Some(cli::AuditCommand::Search { query }) => {
            let pattern = format!("%{query}%");
            let mut stmt = conn.prepare(
                "SELECT actor, action, resource, effect, reason
                 FROM policy_audit WHERE reason LIKE ?1 OR actor LIKE ?1 OR resource LIKE ?1
                 ORDER BY created_at DESC LIMIT 20"
            )?;
            let entries: Vec<(String, String, String, String, Option<String>)> =
                stmt.query_map([&pattern], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
                })?.collect::<Result<Vec<_>, _>>()?;

            println!();
            for (actor, action, resource, effect, reason) in &entries {
                println!("  [{effect}] {actor} → {action} on {resource}: {}",
                    reason.as_deref().unwrap_or(""));
            }
            println!();
        }
        Some(cli::AuditCommand::Export { format }) => {
            println!("  [mp audit export --format {format} — not yet implemented]");
        }
    }
    Ok(())
}

// =========================================================================
// Sync
// =========================================================================

async fn cmd_sync(_config: &Config, cmd: cli::SyncCommand) -> Result<()> {
    match cmd {
        cli::SyncCommand::Status => println!("  [mp sync status — requires sqlite-sync integration (M13)]"),
        cli::SyncCommand::Now { agent } => {
            let name = agent.as_deref().unwrap_or("all");
            println!("  [mp sync now {name} — requires sqlite-sync integration (M13)]");
        }
        cli::SyncCommand::Connect { url } => {
            println!("  [mp sync connect {url} — requires sqlite-sync integration (M13)]");
        }
    }
    Ok(())
}

// =========================================================================
// Db
// =========================================================================

async fn cmd_db(config: &Config, cmd: cli::DbCommand) -> Result<()> {
    let ag = resolve_agent(config, None)?;
    let conn = open_agent_db(config, &ag.name)?;

    match cmd {
        cli::DbCommand::Query { sql, .. } => {
            let mut stmt = conn.prepare(&sql)?;
            let col_count = stmt.column_count();
            let col_names: Vec<String> = (0..col_count)
                .map(|i| stmt.column_name(i).unwrap_or("?").to_string())
                .collect();

            println!();
            println!("  {}", col_names.join(" | "));
            println!("  {}", col_names.iter().map(|n| "-".repeat(n.len())).collect::<Vec<_>>().join("-+-"));

            let mut rows = stmt.query([])?;
            while let Some(row) = rows.next()? {
                let vals: Vec<String> = (0..col_count).map(|i| {
                    row.get::<_, String>(i).unwrap_or_else(|_| "NULL".into())
                }).collect();
                println!("  {}", vals.join(" | "));
            }
            println!();
        }
        cli::DbCommand::Schema { .. } => {
            let mut stmt = conn.prepare(
                "SELECT name, sql FROM sqlite_master WHERE type='table' ORDER BY name"
            )?;
            let tables: Vec<(String, Option<String>)> = stmt.query_map([], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })?.collect::<Result<Vec<_>, _>>()?;

            println!();
            for (name, sql) in &tables {
                println!("  -- {name}");
                if let Some(s) = sql {
                    for line in s.lines() {
                        println!("  {line}");
                    }
                }
                println!();
            }
        }
    }
    Ok(())
}

// =========================================================================
// Health
// =========================================================================

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
            let conn = mp_core::db::open(&db_path)?;
            let fact_count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM facts WHERE superseded_at IS NULL", [], |r| r.get(0)
            ).unwrap_or(0);
            let session_count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM sessions", [], |r| r.get(0)
            ).unwrap_or(0);

            let metadata = std::fs::metadata(&db_path)?;
            let size_kb = metadata.len() / 1024;
            println!("  Agent \"{}\": {size_kb} KB, {fact_count} facts, {session_count} sessions",
                agent.name);
        } else {
            println!("  Agent \"{}\": not initialized", agent.name);
        }
    }

    println!();
    Ok(())
}
