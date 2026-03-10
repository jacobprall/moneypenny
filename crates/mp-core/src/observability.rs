use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};

// =========================================================================
// Health
// =========================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthReport {
    pub status: String,
    pub gateway: GatewayHealth,
    pub agents: Vec<AgentHealth>,
    pub jobs: JobsHealth,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayHealth {
    pub pid: u32,
    pub uptime_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentHealth {
    pub id: String,
    pub name: String,
    pub status: String,
    pub facts: i64,
    pub sessions: i64,
    pub db_size_bytes: u64,
    pub llm_provider: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobsHealth {
    pub active: i64,
    pub failed_24h: i64,
}

/// Build a health report for an agent from its database.
pub fn agent_health(
    conn: &Connection,
    agent_id: &str,
    db_path: &str,
) -> anyhow::Result<AgentHealth> {
    let facts: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM facts WHERE superseded_at IS NULL",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let sessions: i64 = conn
        .query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))
        .unwrap_or(0);

    let db_size = std::fs::metadata(db_path).map(|m| m.len()).unwrap_or(0);

    Ok(AgentHealth {
        id: agent_id.to_string(),
        name: agent_id.to_string(),
        status: "running".into(),
        facts,
        sessions,
        db_size_bytes: db_size,
        llm_provider: "unknown".into(),
    })
}

/// Build a jobs health summary from an agent's database.
pub fn jobs_health(conn: &Connection) -> anyhow::Result<JobsHealth> {
    let active: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM jobs WHERE enabled = 1 AND status = 'active'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let day_ago = chrono::Utc::now().timestamp() - 86400;
    let failed: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM job_runs WHERE status = 'error' AND created_at >= ?1",
            [day_ago],
            |r| r.get(0),
        )
        .unwrap_or(0);

    Ok(JobsHealth {
        active,
        failed_24h: failed,
    })
}

// =========================================================================
// Metrics (Prometheus-compatible counters/gauges)
// =========================================================================

/// Lightweight in-process metrics store.
pub struct Metrics {
    pub messages_total: AtomicU64,
    pub tool_calls_total: AtomicU64,
    pub policy_allow_total: AtomicU64,
    pub policy_deny_total: AtomicU64,
    pub facts_total: AtomicU64,
    pub job_runs_total: AtomicU64,
    pub token_usage_total: AtomicU64,
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

impl Metrics {
    pub fn new() -> Self {
        Self {
            messages_total: AtomicU64::new(0),
            tool_calls_total: AtomicU64::new(0),
            policy_allow_total: AtomicU64::new(0),
            policy_deny_total: AtomicU64::new(0),
            facts_total: AtomicU64::new(0),
            job_runs_total: AtomicU64::new(0),
            token_usage_total: AtomicU64::new(0),
        }
    }

    pub fn inc_messages(&self) {
        self.messages_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_tool_calls(&self) {
        self.tool_calls_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_policy_allow(&self) {
        self.policy_allow_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_policy_deny(&self) {
        self.policy_deny_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn set_facts(&self, count: u64) {
        self.facts_total.store(count, Ordering::Relaxed);
    }

    pub fn inc_job_runs(&self) {
        self.job_runs_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn add_tokens(&self, count: u64) {
        self.token_usage_total.fetch_add(count, Ordering::Relaxed);
    }

    /// Render metrics in Prometheus exposition format.
    pub fn render_prometheus(&self, agent_id: &str) -> String {
        let mut lines = Vec::new();
        lines.push(format!(
            "mp_messages_total{{agent=\"{agent_id}\"}} {}",
            self.messages_total.load(Ordering::Relaxed)
        ));
        lines.push(format!(
            "mp_tool_calls_total{{agent=\"{agent_id}\"}} {}",
            self.tool_calls_total.load(Ordering::Relaxed)
        ));
        lines.push(format!(
            "mp_policy_decisions_total{{agent=\"{agent_id}\",effect=\"allow\"}} {}",
            self.policy_allow_total.load(Ordering::Relaxed)
        ));
        lines.push(format!(
            "mp_policy_decisions_total{{agent=\"{agent_id}\",effect=\"deny\"}} {}",
            self.policy_deny_total.load(Ordering::Relaxed)
        ));
        lines.push(format!(
            "mp_facts_total{{agent=\"{agent_id}\"}} {}",
            self.facts_total.load(Ordering::Relaxed)
        ));
        lines.push(format!(
            "mp_job_runs_total{{agent=\"{agent_id}\"}} {}",
            self.job_runs_total.load(Ordering::Relaxed)
        ));
        lines.push(format!(
            "mp_token_usage_total{{agent=\"{agent_id}\"}} {}",
            self.token_usage_total.load(Ordering::Relaxed)
        ));
        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db, schema};

    fn setup() -> Connection {
        let conn = db::open_memory().unwrap();
        schema::init_agent_db(&conn).unwrap();
        conn
    }

    // ========================================================================
    // Health
    // ========================================================================

    #[test]
    fn agent_health_counts_facts_and_sessions() {
        let conn = setup();
        crate::store::facts::add(
            &conn,
            &crate::store::facts::NewFact {
                agent_id: "a".into(),
                scope: "shared".into(),
                content: "test".into(),
                summary: "t".into(),
                pointer: "p".into(),
                keywords: None,
                source_message_id: None,
                confidence: 1.0,
            },
            None,
        )
        .unwrap();
        crate::store::log::create_session(&conn, "a", None).unwrap();

        let health = agent_health(&conn, "a", ":memory:").unwrap();
        assert_eq!(health.facts, 1);
        assert_eq!(health.sessions, 1);
    }

    #[test]
    fn agent_health_empty_db() {
        let conn = setup();
        let health = agent_health(&conn, "a", ":memory:").unwrap();
        assert_eq!(health.facts, 0);
        assert_eq!(health.sessions, 0);
    }

    #[test]
    fn jobs_health_counts_active_and_failed() {
        let conn = setup();
        let now = chrono::Utc::now().timestamp();
        crate::scheduler::create_job(
            &conn,
            &crate::scheduler::NewJob {
                agent_id: "a".into(),
                name: "test".into(),
                description: None,
                schedule: "* * * * *".into(),
                next_run_at: now + 60,
                job_type: "prompt".into(),
                payload: "{}".into(),
                max_retries: None,
                retry_delay_ms: None,
                timeout_ms: None,
                overlap_policy: None,
            },
        )
        .unwrap();

        let jh = jobs_health(&conn).unwrap();
        assert_eq!(jh.active, 1);
        assert_eq!(jh.failed_24h, 0);
    }

    // ========================================================================
    // Metrics
    // ========================================================================

    #[test]
    fn metrics_increment() {
        let m = Metrics::new();
        m.inc_messages();
        m.inc_messages();
        m.inc_tool_calls();
        m.inc_policy_allow();
        m.inc_policy_deny();
        m.set_facts(42);
        m.add_tokens(1000);

        assert_eq!(m.messages_total.load(Ordering::Relaxed), 2);
        assert_eq!(m.tool_calls_total.load(Ordering::Relaxed), 1);
        assert_eq!(m.policy_allow_total.load(Ordering::Relaxed), 1);
        assert_eq!(m.policy_deny_total.load(Ordering::Relaxed), 1);
        assert_eq!(m.facts_total.load(Ordering::Relaxed), 42);
        assert_eq!(m.token_usage_total.load(Ordering::Relaxed), 1000);
    }

    #[test]
    fn metrics_prometheus_format() {
        let m = Metrics::new();
        m.inc_messages();
        m.set_facts(10);

        let output = m.render_prometheus("main");
        assert!(output.contains("mp_messages_total{agent=\"main\"} 1"));
        assert!(output.contains("mp_facts_total{agent=\"main\"} 10"));
        assert!(output.contains("mp_policy_decisions_total{agent=\"main\",effect=\"allow\"} 0"));
    }

    #[test]
    fn metrics_default() {
        let m = Metrics::default();
        assert_eq!(m.messages_total.load(Ordering::Relaxed), 0);
    }

    // ========================================================================
    // Health report serialization
    // ========================================================================

    #[test]
    fn health_report_serializes_to_json() {
        let report = HealthReport {
            status: "healthy".into(),
            gateway: GatewayHealth {
                pid: 1234,
                uptime_seconds: 3600,
            },
            agents: vec![AgentHealth {
                id: "main".into(),
                name: "main".into(),
                status: "running".into(),
                facts: 42,
                sessions: 10,
                db_size_bytes: 1024000,
                llm_provider: "local".into(),
            }],
            jobs: JobsHealth {
                active: 2,
                failed_24h: 0,
            },
        };

        let json = serde_json::to_string_pretty(&report).unwrap();
        assert!(json.contains("\"healthy\""));
        assert!(json.contains("\"main\""));
        assert!(json.contains("42"));
    }
}
