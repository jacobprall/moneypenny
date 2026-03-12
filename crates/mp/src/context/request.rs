//! Request context for agent turns — bundles parameters to reduce threading.

use mp_core::policy::PolicyMode;
use mp_llm::provider::EmbeddingProvider;

/// Bundles parameters for a single agent turn.
pub struct RequestContext<'a> {
    pub agent_id: &'a str,
    pub conn: &'a rusqlite::Connection,
    pub session_id: &'a str,
    pub embed_provider: Option<&'a dyn EmbeddingProvider>,
    pub policy_mode: PolicyMode,
    pub persona: Option<&'a str>,
    pub worker_bus: Option<&'a std::sync::Arc<crate::worker::WorkerBus>>,
}
