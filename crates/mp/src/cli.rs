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

    /// Start gateway with MCP sidecar on stdio (combines start + sidecar)
    Serve {
        /// Agent name for MCP sidecar (defaults to first configured agent)
        #[arg(long)]
        agent: Option<String>,
    },

    /// Graceful shutdown
    Stop,

    /// Manage agents
    #[command(subcommand)]
    Agent(AgentCommand),

    /// Brain lifecycle — checkpoint, restore, export
    #[command(subcommand)]
    Brain(BrainCommand),

    /// Experience priors — record, match, search, stats
    #[command(subcommand)]
    Experience(ExperienceCommand),

    /// Focus working set — set, get, list, compose
    #[command(subcommand)]
    Focus(FocusCommand),

    /// Interactive CLI chat with an agent
    Chat {
        /// Agent name (defaults to first configured agent)
        agent: Option<String>,

        /// Resume an existing session by ID (if omitted, resumes the most recent session)
        #[arg(long)]
        session_id: Option<String>,

        /// Force a new session instead of resuming the last one
        #[arg(long, default_value_t = false)]
        new: bool,
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

    /// Fleet operations across multiple agents
    #[command(subcommand)]
    Fleet(FleetCommand),

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

    /// Show token/cost usage summary
    Spend {
        /// Agent name (defaults to first configured agent)
        #[arg(long)]
        agent: Option<String>,

        /// Time period: today, week, month, all
        #[arg(long, default_value = "all")]
        period: String,

        /// Group breakdown by: model, session, day
        #[arg(long, default_value = "model")]
        group_by: String,
    },

    /// Session briefing — recap recent activity, facts, denials, and spend
    Briefing {
        /// Agent name (defaults to first configured agent)
        #[arg(long)]
        agent: Option<String>,
    },

    /// Show system health
    Health,

    /// Run setup diagnostics and suggested fixes
    Doctor,

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

// -- Brain subcommands --

#[derive(Subcommand)]
pub enum BrainCommand {
    /// List brains in the agent DB
    List {
        #[arg(long)]
        agent: Option<String>,
    },

    /// Create a checkpoint (full DB snapshot)
    Checkpoint {
        /// Checkpoint name
        #[arg(long)]
        name: String,

        /// Output path for checkpoint file
        #[arg(long)]
        output: String,

        #[arg(long)]
        agent: Option<String>,
    },

    /// Restore from a checkpoint (replace agent DB)
    Restore {
        /// Path to checkpoint file (or use --checkpoint-id)
        #[arg(long)]
        path: Option<String>,

        /// Checkpoint ID (lookup path from checkpoints table)
        #[arg(long)]
        checkpoint_id: Option<String>,

        #[arg(long)]
        agent: Option<String>,

        #[arg(long, default_value_t = false)]
        confirm: bool,
    },

    /// Export brain data as JSON
    Export {
        #[arg(long)]
        output: Option<String>,

        #[arg(long)]
        agent: Option<String>,
    },
}

// -- Experience subcommands --

#[derive(Subcommand)]
pub enum ExperienceCommand {
    /// Search experience priors
    Search {
        #[arg(long)]
        query: String,

        #[arg(long, default_value_t = 20)]
        limit: usize,

        #[arg(long)]
        agent: Option<String>,
    },

    /// Show experience stats
    Stats {
        #[arg(long)]
        agent: Option<String>,
    },
}

// -- Focus subcommands --

#[derive(Subcommand)]
pub enum FocusCommand {
    /// Set a key in the working set
    Set {
        #[arg(long)]
        key: String,

        #[arg(long)]
        content: String,

        #[arg(long)]
        agent: Option<String>,

        #[arg(long)]
        session_id: Option<String>,
    },

    /// Get a key from the working set
    Get {
        #[arg(long)]
        key: String,

        #[arg(long)]
        agent: Option<String>,

        #[arg(long)]
        session_id: Option<String>,
    },

    /// List working set entries
    List {
        #[arg(long)]
        agent: Option<String>,

        #[arg(long)]
        session_id: Option<String>,
    },

    /// Compose context
    Compose {
        #[arg(long)]
        task_hint: Option<String>,

        #[arg(long, default_value_t = 128000)]
        max_tokens: usize,

        #[arg(long)]
        agent: Option<String>,

        #[arg(long)]
        session_id: Option<String>,
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

        /// Include entries created at or after this Unix timestamp (seconds)
        #[arg(long)]
        since: Option<i64>,

        /// Include entries created at or before this Unix timestamp (seconds)
        #[arg(long)]
        until: Option<i64>,
    },

    /// Export audit trail
    Export {
        /// Output format: sql, json, csv
        #[arg(long, default_value = "json")]
        format: String,

        /// Include entries created at or after this Unix timestamp (seconds)
        #[arg(long)]
        since: Option<i64>,

        /// Include entries created at or before this Unix timestamp (seconds)
        #[arg(long)]
        until: Option<i64>,
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

// -- Fleet subcommands --

#[derive(Subcommand)]
pub enum FleetCommand {
    /// Provision agents from a fleet template
    Init {
        /// Template file (.json or .toml)
        #[arg(long)]
        template: String,

        /// Optional scope filter by tags (comma-separated, e.g. "team:infra,env:prod")
        #[arg(long)]
        scope: Option<String>,

        /// Validate and print planned changes only
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },

    /// Push a signed policy bundle to scoped agents
    PushPolicy {
        /// Policy bundle file (.json or .toml)
        #[arg(long)]
        file: String,

        /// Optional scope filter by tags (comma-separated)
        #[arg(long)]
        scope: Option<String>,

        /// Optional file path to write rollback snapshot
        #[arg(long)]
        rollback_file: Option<String>,

        /// Validate and print planned changes only
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },

    /// Aggregate audit entries across scoped agents
    Audit {
        /// Optional scope filter by tags (comma-separated)
        #[arg(long)]
        scope: Option<String>,

        /// Include entries created at or after this Unix timestamp (seconds)
        #[arg(long)]
        since: Option<i64>,

        /// Include entries created at or before this Unix timestamp (seconds)
        #[arg(long)]
        until: Option<i64>,

        /// Output format: json, csv
        #[arg(long, default_value = "json")]
        format: String,

        /// Max rows per agent
        #[arg(long, default_value_t = 200)]
        limit: usize,
    },

    /// List agents in metadata registry (with tags)
    List {
        /// Optional scope filter by tags (comma-separated)
        #[arg(long)]
        scope: Option<String>,
    },

    /// Show fleet health + sync + drift summary
    Status {
        /// Optional scope filter by tags (comma-separated)
        #[arg(long)]
        scope: Option<String>,
    },

    /// Set tags for an agent (comma-separated)
    Tag {
        /// Agent name
        agent: String,

        /// Tags CSV, e.g. "team:infra,env:prod"
        tags: String,
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

    /// Register Moneypenny as an MCP server in Claude Code
    ClaudeCode {
        /// Agent name (defaults to first configured agent)
        #[arg(long)]
        agent: Option<String>,

        /// Scope: "project" writes .mcp.json (committable), "user" writes ~/.claude.json
        #[arg(long, default_value = "project")]
        scope: String,
    },

    /// Register Moneypenny as an MCP server in Cortex Code CLI
    Cortex {
        /// Agent name (defaults to first configured agent)
        #[arg(long)]
        agent: Option<String>,

        /// Scope: "project" writes .cortex/settings.local.json, "user" writes ~/.snowflake/cortex/mcp.json
        #[arg(long, default_value = "project")]
        scope: String,
    },

    /// Download embedding models required by configured agents
    Models,

    /// Seed bootstrap facts into agent databases (safe to re-run)
    Seed,
}
