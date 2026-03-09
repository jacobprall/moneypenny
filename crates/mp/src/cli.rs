use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "mp",
    about = "Moneypenny — the autonomous AI agent where the database is the runtime",
    version,
    propagate_version = true
)]
pub struct Cli {
    /// Path to moneypenny.toml config file
    #[arg(short, long, default_value = "moneypenny.toml", env = "MP_CONFIG")]
    pub config: String,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Create moneypenny.toml and data directory
    Init,

    /// Start gateway and all configured agents
    Start,

    /// Graceful shutdown
    Stop,

    /// Manage agents
    #[command(subcommand)]
    Agent(AgentCommand),

    /// Interactive CLI chat with an agent
    Chat {
        /// Agent name (defaults to first configured agent)
        agent: Option<String>,

        /// Resume an existing session by ID (if omitted, creates a new session)
        #[arg(long)]
        session_id: Option<String>,
    },

    /// Send a one-off message and print the response
    Send {
        /// Agent name
        agent: String,
        /// Message to send
        message: String,

        /// Resume an existing session by ID (if omitted, creates a new session)
        #[arg(long)]
        session_id: Option<String>,
    },

    /// Manage conversation sessions
    #[command(subcommand)]
    Session(SessionCommand),

    /// Manage facts (extracted knowledge)
    #[command(subcommand)]
    Facts(FactsCommand),

    /// Ingest documents into the knowledge store
    Ingest {
        /// File or directory path to ingest
        path: Option<String>,

        /// URL to ingest
        #[arg(long)]
        url: Option<String>,

        /// Agent name
        agent: Option<String>,

        /// OpenClaw JSONL file to ingest as external events
        #[arg(long)]
        openclaw_file: Option<String>,

        /// Replay from start (ignore prior ingest cursor)
        #[arg(long, default_value_t = false)]
        replay: bool,

        /// Show recent ingest runs instead of ingesting data
        #[arg(long, default_value_t = false)]
        status: bool,

        /// Replay a prior ingest run by run ID
        #[arg(long)]
        replay_run: Option<String>,

        /// Replay the latest matching run (uses --source/--status-filter/--file-filter)
        #[arg(long, default_value_t = false, conflicts_with = "replay_run")]
        replay_latest: bool,

        /// Offset when using --replay-latest (0 = newest, 1 = previous, etc.)
        #[arg(long, default_value_t = 0)]
        replay_offset: usize,

        /// Filter ingest runs by status (e.g. completed, completed_with_errors, running)
        #[arg(long)]
        status_filter: Option<String>,

        /// Filter ingest runs whose file path contains this substring
        #[arg(long)]
        file_filter: Option<String>,

        /// Force preflight replay without writing projected rows
        #[arg(long, default_value_t = false)]
        dry_run: bool,

        /// Apply replay writes (default behavior is safe preview)
        #[arg(long, default_value_t = false)]
        apply: bool,

        /// Source label for external ingest/status
        #[arg(long, default_value = "openclaw")]
        source: String,

        /// Limit for ingest status output
        #[arg(long, default_value_t = 20)]
        limit: usize,

        /// Ingest all Cortex Code CLI conversations from ~/.snowflake/cortex/conversations/
        #[arg(long, default_value_t = false)]
        cortex: bool,

        /// Ingest Claude Code conversations (optionally pass a project slug to scope)
        #[arg(long)]
        claude_code: Option<String>,

        /// Ingest Cursor agent transcripts (optionally pass a project slug to scope)
        #[arg(long)]
        cursor: Option<String>,
    },

    /// Manage the knowledge store
    #[command(subcommand)]
    Knowledge(KnowledgeCommand),

    /// Manage skills
    #[command(subcommand)]
    Skill(SkillCommand),

    /// Manage policies
    #[command(subcommand)]
    Policy(PolicyCommand),

    /// Manage scheduled jobs
    #[command(subcommand)]
    Job(JobCommand),

    /// Manage embedding queue operations
    #[command(subcommand)]
    Embeddings(EmbeddingsCommand),

    /// View the audit trail
    Audit {
        /// Agent name
        agent: Option<String>,

        #[command(subcommand)]
        command: Option<AuditCommand>,
    },

    /// Manage sync
    #[command(subcommand)]
    Sync(SyncCommand),

    /// Execute an MPQ expression (Moneypenny Query DSL)
    #[command(name = "mpq")]
    Mpq {
        /// MPQ expression (e.g. 'SEARCH facts', 'SEARCH activity')
        expression: String,

        /// Agent name (defaults to first configured agent)
        #[arg(long)]
        agent: Option<String>,

        /// Parse and policy-check without executing
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },

    /// Direct database access (read-only)
    #[command(subcommand)]
    Db(DbCommand),

    /// Show system health
    Health,

    /// Internal: run as an agent worker process (used by `mp start`)
    #[command(hide = true)]
    Worker {
        /// Agent name this worker serves
        #[arg(long)]
        agent: String,
    },

    /// Run canonical operation sidecar over stdio (JSONL)
    Sidecar {
        /// Agent name (defaults to first configured agent)
        #[arg(long)]
        agent: Option<String>,
    },

    /// Register Moneypenny with an AI coding agent
    #[command(subcommand)]
    Setup(SetupCommand),

    /// Process a Cursor hook event (audit + policy enforcement)
    #[command(hide = true)]
    Hook {
        /// Hook event name (e.g. preToolUse, sessionStart)
        #[arg(long)]
        event: String,

        /// Agent name (defaults to first configured agent)
        #[arg(long)]
        agent: Option<String>,
    },
}

// -- Agent subcommands --

#[derive(Subcommand)]
pub enum AgentCommand {
    /// List all agents
    List,

    /// Create a new agent
    Create {
        /// Agent name
        name: String,
    },

    /// Delete an agent and its database
    Delete {
        /// Agent name
        name: String,

        /// Skip confirmation prompt
        #[arg(long)]
        confirm: bool,
    },

    /// Show agent status and memory stats
    Status {
        /// Agent name (defaults to all)
        name: Option<String>,
    },

    /// Set agent configuration
    Config {
        /// Agent name
        name: String,
        /// Config key
        key: String,
        /// Config value
        value: String,
    },
}

// -- Facts subcommands --

#[derive(Subcommand)]
pub enum FactsCommand {
    /// List all facts (pointer + summary)
    List {
        /// Agent name
        agent: Option<String>,
    },

    /// Search across facts
    Search {
        /// Search query
        query: String,

        /// Agent name
        agent: Option<String>,
    },

    /// Show full fact with audit history
    Inspect {
        /// Fact ID
        id: String,
    },

    /// Expand a compacted pointer to full fact content
    Expand {
        /// Fact ID
        id: String,
    },

    /// Reset compaction state for a fact
    ResetCompaction {
        /// Fact ID (omit when using --all)
        id: Option<String>,

        /// Reset compaction for all active facts
        #[arg(long, default_value_t = false)]
        all: bool,

        /// Agent name (for --all mode)
        agent: Option<String>,

        /// Required when using --all
        #[arg(long, default_value_t = false)]
        confirm: bool,
    },

    /// Promote a fact to shared scope
    Promote {
        /// Fact ID
        id: String,

        /// Target scope
        #[arg(long, default_value = "shared")]
        scope: String,
    },

    /// Delete a fact
    Delete {
        /// Fact ID
        id: String,

        /// Skip confirmation prompt
        #[arg(long)]
        confirm: bool,
    },
}

// -- Knowledge subcommands --

#[derive(Subcommand)]
pub enum KnowledgeCommand {
    /// Search ingested knowledge
    Search {
        /// Search query
        query: String,
    },

    /// List ingested documents
    List,
}

// -- Skill subcommands --

#[derive(Subcommand)]
pub enum SkillCommand {
    /// Add a skill from a markdown file
    Add {
        /// Path to skill file
        path: String,

        /// Agent name
        agent: Option<String>,
    },

    /// List skills with usage stats
    List {
        /// Agent name
        agent: Option<String>,
    },

    /// Manually promote a skill
    Promote {
        /// Skill ID
        id: String,
    },
}

// -- Policy subcommands --

#[derive(Subcommand)]
pub enum PolicyCommand {
    /// List all active policies
    List,

    /// Add a policy rule
    Add {
        /// Policy name
        #[arg(long)]
        name: String,

        /// Effect: allow, deny, or audit
        #[arg(long, default_value = "deny")]
        effect: String,

        /// Priority (higher = evaluated first)
        #[arg(long, default_value_t = 0)]
        priority: i64,

        /// Actor pattern (glob)
        #[arg(long)]
        actor: Option<String>,

        /// Action pattern (glob)
        #[arg(long)]
        action: Option<String>,

        /// Resource pattern (glob)
        #[arg(long)]
        resource: Option<String>,

        /// Argument pattern (glob, e.g. URL whitelist)
        #[arg(long)]
        argument: Option<String>,

        /// Channel pattern (glob)
        #[arg(long)]
        channel: Option<String>,

        /// SQL pattern (regex)
        #[arg(long)]
        sql: Option<String>,

        /// Behavioral rule type (rate_limit, retry_loop, token_budget, time_window)
        #[arg(long)]
        rule_type: Option<String>,

        /// Behavioral rule config JSON
        #[arg(long)]
        rule_config: Option<String>,

        /// Denial/audit message
        #[arg(long)]
        message: Option<String>,
    },

    /// Dry-run: test if an action would be allowed
    Test {
        /// Input to test (e.g. SQL statement)
        input: String,
    },

    /// Show recent policy violations
    Violations {
        /// Time window (e.g. "7d", "24h")
        #[arg(long, default_value = "7d")]
        last: String,
    },

    /// Load policies from a JSON or Polar file
    Load {
        /// Path to policy file
        file: String,
    },
}

// -- Job subcommands --

#[derive(Subcommand)]
pub enum JobCommand {
    /// List scheduled jobs
    List {
        /// Agent name
        agent: Option<String>,
    },

    /// Create a new job
    Create {
        /// Job name
        #[arg(long)]
        name: String,

        /// Cron schedule
        #[arg(long)]
        schedule: String,

        /// Job type: prompt, tool, js, pipeline
        #[arg(long)]
        job_type: String,

        /// JSON payload
        #[arg(long)]
        payload: String,

        /// Agent name
        #[arg(long)]
        agent: Option<String>,
    },

    /// Trigger a job immediately
    Run {
        /// Job ID
        id: String,
    },

    /// Pause a job
    Pause {
        /// Job ID
        id: String,
    },

    /// Show job run history
    History {
        /// Job ID (optional — all jobs if omitted)
        id: Option<String>,
    },
}

// -- Embedding queue subcommands --

#[derive(Subcommand)]
pub enum EmbeddingsCommand {
    /// Show embedding queue status and per-target breakdown
    Status {
        /// Agent name (defaults to first configured agent)
        #[arg(long)]
        agent: Option<String>,
    },

    /// Move dead embedding jobs back to retry
    RetryDead {
        /// Agent name (defaults to first configured agent)
        #[arg(long)]
        agent: Option<String>,

        /// Optional target filter: facts, messages, tool_calls, policy_audit, chunks
        #[arg(long)]
        target: Option<String>,

        /// Max dead jobs to revive
        #[arg(long, default_value_t = 500)]
        limit: usize,
    },

    /// Enqueue and process a model backfill immediately
    Backfill {
        /// Agent name (defaults to first configured agent)
        #[arg(long)]
        agent: Option<String>,

        /// Override embedding model name (defaults to agent config model)
        #[arg(long)]
        model: Option<String>,

        /// Max rows to enqueue per target
        #[arg(long, default_value_t = 10_000)]
        limit: usize,

        /// Jobs to process per batch iteration
        #[arg(long, default_value_t = 128)]
        batch_size: usize,

        /// Only enqueue jobs, do not run embedding processing
        #[arg(long, default_value_t = false)]
        enqueue_only: bool,
    },
}

// -- Audit subcommands --

#[derive(Subcommand)]
pub enum AuditCommand {
    /// Search audit entries
    Search {
        /// Search query
        query: String,
    },

    /// Export audit trail
    Export {
        /// Output format: sql, json, csv
        #[arg(long, default_value = "json")]
        format: String,
    },
}

// -- Sync subcommands --

#[derive(Subcommand)]
pub enum SyncCommand {
    /// Show CRDT sync status (site ID, DB version, per-table enabled flag)
    Status {
        /// Agent name (defaults to first configured agent)
        agent: Option<String>,
    },

    /// Bidirectional sync with all configured peers and/or cloud backend
    Now {
        /// Agent name (defaults to first configured agent)
        agent: Option<String>,
    },

    /// Push this agent's changes to a peer agent (one-way)
    Push {
        /// Name or DB path of the target agent
        #[arg(long, value_name = "AGENT")]
        to: String,
        /// Agent name to push from (defaults to first configured agent)
        agent: Option<String>,
    },

    /// Pull changes from a peer agent into this one (one-way)
    Pull {
        /// Name or DB path of the source agent
        #[arg(long, value_name = "AGENT")]
        from: String,
        /// Agent name to pull into (defaults to first configured agent)
        agent: Option<String>,
    },

    /// Set (or update) the cloud sync URL for this agent
    Connect {
        /// SQLite Cloud connection string (include `?apikey=…`)
        url: String,
        /// Agent name (defaults to first configured agent)
        agent: Option<String>,
    },
}

// -- Db subcommands --

#[derive(Subcommand)]
pub enum DbCommand {
    /// Run a read-only SQL query against an agent's database
    Query {
        /// SQL query
        sql: String,

        /// Agent name
        agent: Option<String>,
    },

    /// Show database schema
    Schema {
        /// Agent name
        agent: Option<String>,
    },
}

// -- Session subcommands --

#[derive(Subcommand)]
pub enum SessionCommand {
    /// List recent sessions for an agent
    List {
        /// Agent name (defaults to first configured agent)
        agent: Option<String>,

        /// Max sessions to return
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
}

// -- Setup subcommands --

#[derive(Subcommand)]
pub enum SetupCommand {
    /// Register Moneypenny as an MCP server in Cursor
    Cursor {
        /// Agent name (defaults to first configured agent)
        #[arg(long)]
        agent: Option<String>,

        /// Use local binary instead of Docker (default is Docker)
        #[arg(long, default_value_t = false)]
        local: bool,

        /// Docker image name
        #[arg(long, default_value = "moneypenny")]
        image: String,
    },

    /// Download embedding models required by configured agents
    Models,

    /// Seed bootstrap facts into agent databases (safe to re-run)
    Seed,
}
