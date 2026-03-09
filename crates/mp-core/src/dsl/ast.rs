use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Program {
    pub statements: Vec<Statement>,
    pub raw: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Statement {
    pub head: Head,
    pub pipeline: Vec<PipeStage>,
    pub raw: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Head {
    Search(SearchHead),
    Insert(InsertHead),
    Update(UpdateHead),
    Delete(DeleteHead),
    Ingest(IngestHead),
    CreatePolicy(CreatePolicyHead),
    EvaluatePolicy(EvalPolicyHead),
    ExplainPolicy(EvalPolicyHead),
    CreateJob(CreateJobHead),
    RunJob(StringArg),
    PauseJob(StringArg),
    ResumeJob(StringArg),
    ListJobs,
    HistoryJob(StringArg),
    CreateAgent(CreateAgentHead),
    DeleteAgent(StringArg),
    ConfigAgent(ConfigAgentHead),
    ResolveSession(OptionalStringArg),
    ListSessions,
    CreateSkill(StringArg),
    PromoteSkill(StringArg),
    CreateTool(CreateToolHead),
    ListTools,
    DeleteTool(StringArg),
    EmbeddingStatus,
    EmbeddingRetryDead,
    EmbeddingBackfill,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHead {
    pub store: Store,
    pub query: Option<String>,
    pub conditions: Vec<Condition>,
    pub mode: SearchMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InsertHead {
    pub store: Store,
    pub content: String,
    pub fields: Vec<(String, Literal)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateHead {
    pub store: Store,
    pub assignments: Vec<(String, Literal)>,
    pub conditions: Vec<Condition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteHead {
    pub store: Store,
    pub conditions: Vec<Condition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestHead {
    pub url: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePolicyHead {
    pub effect: PolicyEffect,
    pub action: String,
    pub resource: String,
    pub agent: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalPolicyHead {
    pub actor: String,
    pub action: String,
    pub resource: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateJobHead {
    pub name: String,
    pub schedule: String,
    pub job_type: Option<String>,
    pub payload: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAgentHead {
    pub name: String,
    pub config: Vec<(String, Literal)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigAgentHead {
    pub name: String,
    pub assignments: Vec<(String, Literal)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateToolHead {
    pub name: String,
    pub language: String,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StringArg {
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptionalStringArg {
    pub value: Option<String>,
}

// ── Pipeline stages ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PipeStage {
    Sort { field: String, order: SortOrder },
    Take(usize),
    Offset(usize),
    Count,
    Process,
}

// ── Filters ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Condition {
    Cmp {
        field: String,
        op: CmpOp,
        value: Literal,
    },
    Like {
        field: String,
        pattern: String,
    },
    Scope(String),
    Agent(String),
    Since(DurationLit),
    Before(DurationLit),
}

// ── Scalars & enums ──

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Literal {
    Str(String),
    Int(i64),
    Float(f64),
    Bool(bool),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CmpOp {
    Eq,
    Ne,
    Gt,
    Lt,
    Ge,
    Le,
}

impl fmt::Display for CmpOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CmpOp::Eq => write!(f, "="),
            CmpOp::Ne => write!(f, "!="),
            CmpOp::Gt => write!(f, ">"),
            CmpOp::Lt => write!(f, "<"),
            CmpOp::Ge => write!(f, ">="),
            CmpOp::Le => write!(f, "<="),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Store {
    Facts,
    Knowledge,
    Log,
    Audit,
}

impl Store {
    pub fn from_str(s: &str) -> Option<Store> {
        match s.to_ascii_lowercase().as_str() {
            "facts" => Some(Store::Facts),
            "knowledge" => Some(Store::Knowledge),
            "log" => Some(Store::Log),
            "audit" => Some(Store::Audit),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Store::Facts => "facts",
            Store::Knowledge => "knowledge",
            Store::Log => "log",
            Store::Audit => "audit",
        }
    }
}

impl fmt::Display for Store {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SearchMode {
    Fts,
    Vector,
    Hybrid,
}

impl Default for SearchMode {
    fn default() -> Self {
        SearchMode::Hybrid
    }
}

impl SearchMode {
    pub fn from_str(s: &str) -> Option<SearchMode> {
        match s.to_ascii_lowercase().as_str() {
            "fts" => Some(SearchMode::Fts),
            "vector" => Some(SearchMode::Vector),
            "hybrid" => Some(SearchMode::Hybrid),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SortOrder {
    Asc,
    Desc,
}

impl Default for SortOrder {
    fn default() -> Self {
        SortOrder::Asc
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum PolicyEffect {
    Allow,
    Deny,
    Audit,
}

impl PolicyEffect {
    pub fn from_str(s: &str) -> Option<PolicyEffect> {
        match s.to_ascii_lowercase().as_str() {
            "allow" => Some(PolicyEffect::Allow),
            "deny" => Some(PolicyEffect::Deny),
            "audit" => Some(PolicyEffect::Audit),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            PolicyEffect::Allow => "allow",
            PolicyEffect::Deny => "deny",
            PolicyEffect::Audit => "audit",
        }
    }
}

/// Duration literal: a count + unit (e.g. `7d`, `24h`, `30m`, `90s`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct DurationLit {
    pub amount: u64,
    pub unit: DurationUnit,
}

impl DurationLit {
    pub fn to_seconds(&self) -> i64 {
        let mult: i64 = match self.unit {
            DurationUnit::Seconds => 1,
            DurationUnit::Minutes => 60,
            DurationUnit::Hours => 3600,
            DurationUnit::Days => 86400,
        };
        self.amount as i64 * mult
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum DurationUnit {
    Seconds,
    Minutes,
    Hours,
    Days,
}
