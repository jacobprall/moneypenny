use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::path::Path;

// =========================================================================
// Agent registry
// =========================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentEntry {
    pub id: String,
    pub name: String,
    pub persona: Option<String>,
    pub trust_level: String,
    pub llm_provider: String,
    pub llm_model: Option<String>,
    pub db_path: String,
    pub sync_enabled: bool,
    pub status: AgentStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Running,
    Stopped,
    Error,
}

/// Load all agents from the metadata database.
pub fn list_agents(meta_conn: &Connection) -> anyhow::Result<Vec<AgentEntry>> {
    let mut stmt = meta_conn.prepare(
        "SELECT id, name, persona, trust_level, llm_provider, llm_model, db_path, sync_enabled
         FROM agents ORDER BY name",
    )?;
    let agents = stmt
        .query_map([], |r| {
            Ok(AgentEntry {
                id: r.get(0)?,
                name: r.get(1)?,
                persona: r.get(2)?,
                trust_level: r.get(3)?,
                llm_provider: r.get(4)?,
                llm_model: r.get(5)?,
                db_path: r.get(6)?,
                sync_enabled: r.get::<_, i64>(7).unwrap_or(1) != 0,
                status: AgentStatus::Stopped,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(agents)
}

/// Get a single agent by name.
pub fn get_agent(meta_conn: &Connection, name: &str) -> anyhow::Result<Option<AgentEntry>> {
    let entry = meta_conn
        .query_row(
            "SELECT id, name, persona, trust_level, llm_provider, llm_model, db_path, sync_enabled
         FROM agents WHERE name = ?1",
            [name],
            |r| {
                Ok(AgentEntry {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    persona: r.get(2)?,
                    trust_level: r.get(3)?,
                    llm_provider: r.get(4)?,
                    llm_model: r.get(5)?,
                    db_path: r.get(6)?,
                    sync_enabled: r.get::<_, i64>(7).unwrap_or(1) != 0,
                    status: AgentStatus::Stopped,
                })
            },
        )
        .ok();
    Ok(entry)
}

// =========================================================================
// Message routing
// =========================================================================

/// A message routed through the gateway.
#[derive(Debug, Clone)]
pub struct RoutedMessage {
    pub source_agent: Option<String>,
    pub target_agent: String,
    pub channel: String,
    pub content: String,
    pub delegation_depth: usize,
}

/// Route a message to the target agent. Returns the response.
pub fn route_message(
    meta_conn: &Connection,
    msg: &RoutedMessage,
    agent_handler: &dyn Fn(&AgentEntry, &str) -> anyhow::Result<String>,
) -> anyhow::Result<String> {
    let agent = get_agent(meta_conn, &msg.target_agent)?
        .ok_or_else(|| anyhow::anyhow!("Agent '{}' not found", msg.target_agent))?;

    // Policy check for delegation
    if let Some(ref source) = msg.source_agent {
        let conn = crate::db::open(Path::new(&agent.db_path))?;
        let policy_req = crate::policy::PolicyRequest {
            actor: source,
            action: "delegate",
            resource: &msg.target_agent,
            sql_content: None,
            channel: Some(&msg.channel),
            arguments: None,
        };
        let decision = crate::policy::evaluate(&conn, &policy_req)?;
        if matches!(decision.effect, crate::policy::Effect::Deny) {
            anyhow::bail!(
                "Delegation denied: {}",
                decision.reason.as_deref().unwrap_or("policy denied")
            );
        }
    }

    agent_handler(&agent, &msg.content)
}

// =========================================================================
// Delegation
// =========================================================================

const MAX_DELEGATION_DEPTH: usize = 3;

/// Delegate a task from one agent to another.
pub fn delegate(
    meta_conn: &Connection,
    source_agent: &str,
    target_agent: &str,
    message: &str,
    current_depth: usize,
    agent_handler: &dyn Fn(&AgentEntry, &str) -> anyhow::Result<String>,
) -> anyhow::Result<String> {
    if current_depth >= MAX_DELEGATION_DEPTH {
        anyhow::bail!("Maximum delegation depth ({MAX_DELEGATION_DEPTH}) exceeded");
    }

    let msg = RoutedMessage {
        source_agent: Some(source_agent.to_string()),
        target_agent: target_agent.to_string(),
        channel: "internal".to_string(),
        content: message.to_string(),
        delegation_depth: current_depth + 1,
    };

    route_message(meta_conn, &msg, agent_handler)
}

// =========================================================================
// Fact scope model
// =========================================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FactScope {
    Private,
    Shared,
    Protected,
}

impl std::fmt::Display for FactScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FactScope::Private => write!(f, "private"),
            FactScope::Shared => write!(f, "shared"),
            FactScope::Protected => write!(f, "protected"),
        }
    }
}

impl std::str::FromStr for FactScope {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "private" => Ok(FactScope::Private),
            "shared" => Ok(FactScope::Shared),
            "protected" => Ok(FactScope::Protected),
            _ => anyhow::bail!("unknown fact scope: {s}"),
        }
    }
}

/// Check if an agent can access a fact based on its scope.
pub fn can_access_fact(
    agent_trust: &str,
    fact_scope: &FactScope,
    fact_agent_id: &str,
    requesting_agent_id: &str,
) -> bool {
    match fact_scope {
        FactScope::Private => fact_agent_id == requesting_agent_id,
        FactScope::Shared => true,
        FactScope::Protected => {
            fact_agent_id == requesting_agent_id
                || agent_trust == "elevated"
                || agent_trust == "admin"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db, schema};
    use rusqlite::params;

    fn setup_meta() -> Connection {
        let conn = db::open_memory().unwrap();
        schema::init_metadata_db(&conn).unwrap();
        conn
    }

    fn insert_agent(conn: &Connection, name: &str) {
        conn.execute(
            "INSERT INTO agents (id, name, trust_level, llm_provider, db_path, created_at)
             VALUES (?1, ?2, 'standard', 'local', ':memory:', 1)",
            params![name, name],
        )
        .unwrap();
    }

    // ========================================================================
    // Agent registry
    // ========================================================================

    #[test]
    fn list_agents_returns_all() {
        let conn = setup_meta();
        insert_agent(&conn, "alpha");
        insert_agent(&conn, "beta");
        let agents = list_agents(&conn).unwrap();
        assert_eq!(agents.len(), 2);
    }

    #[test]
    fn get_agent_by_name() {
        let conn = setup_meta();
        insert_agent(&conn, "alpha");
        let agent = get_agent(&conn, "alpha").unwrap().unwrap();
        assert_eq!(agent.name, "alpha");
        assert_eq!(agent.trust_level, "standard");
    }

    #[test]
    fn get_agent_nonexistent() {
        let conn = setup_meta();
        assert!(get_agent(&conn, "nope").unwrap().is_none());
    }

    // ========================================================================
    // Message routing
    // ========================================================================

    #[test]
    fn route_message_to_agent() {
        let conn = setup_meta();
        insert_agent(&conn, "alpha");

        let msg = RoutedMessage {
            source_agent: None,
            target_agent: "alpha".into(),
            channel: "cli".into(),
            content: "hello".into(),
            delegation_depth: 0,
        };

        let response = route_message(&conn, &msg, &|agent, content| {
            Ok(format!("{} received: {}", agent.name, content))
        })
        .unwrap();

        assert_eq!(response, "alpha received: hello");
    }

    #[test]
    fn route_message_unknown_agent() {
        let conn = setup_meta();
        let msg = RoutedMessage {
            source_agent: None,
            target_agent: "nope".into(),
            channel: "cli".into(),
            content: "hello".into(),
            delegation_depth: 0,
        };

        let result = route_message(&conn, &msg, &|_, _| Ok("ok".into()));
        assert!(result.is_err());
    }

    // ========================================================================
    // Delegation
    // ========================================================================

    #[test]
    fn delegate_routes_to_target() {
        let conn = setup_meta();
        insert_agent(&conn, "beta");

        // Direct route without source_agent to skip policy check
        let msg = RoutedMessage {
            source_agent: None,
            target_agent: "beta".into(),
            channel: "internal".into(),
            content: "research this".into(),
            delegation_depth: 1,
        };
        let result = route_message(&conn, &msg, &|agent, content| {
            Ok(format!("{}: done with {}", agent.name, content))
        })
        .unwrap();
        assert!(result.contains("beta: done with"));
    }

    #[test]
    fn delegate_exceeds_max_depth() {
        let conn = setup_meta();
        insert_agent(&conn, "alpha");

        let result = delegate(
            &conn,
            "alpha",
            "alpha",
            "loop forever",
            MAX_DELEGATION_DEPTH,
            &|_, _| Ok("ok".into()),
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("depth"));
    }

    // ========================================================================
    // Fact scope
    // ========================================================================

    #[test]
    fn fact_scope_roundtrip() {
        for scope in [FactScope::Private, FactScope::Shared, FactScope::Protected] {
            let s = scope.to_string();
            let parsed: FactScope = s.parse().unwrap();
            assert_eq!(parsed, scope);
        }
    }

    #[test]
    fn private_fact_only_accessible_by_owner() {
        assert!(can_access_fact("standard", &FactScope::Private, "a", "a"));
        assert!(!can_access_fact("standard", &FactScope::Private, "a", "b"));
    }

    #[test]
    fn shared_fact_accessible_by_anyone() {
        assert!(can_access_fact("standard", &FactScope::Shared, "a", "b"));
    }

    #[test]
    fn protected_fact_requires_trust() {
        assert!(can_access_fact("standard", &FactScope::Protected, "a", "a"));
        assert!(!can_access_fact(
            "standard",
            &FactScope::Protected,
            "a",
            "b"
        ));
        assert!(can_access_fact("elevated", &FactScope::Protected, "a", "b"));
        assert!(can_access_fact("admin", &FactScope::Protected, "a", "b"));
    }
}
