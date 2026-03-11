//! Helpers — split by responsibility.

mod bootstrap;
mod db;
mod embedding;
mod extraction;
mod formatting;
mod models;
mod providers;
mod session;
mod sidecar;

pub use bootstrap::seed_bootstrap_facts;
pub use db::{open_agent_db, resolve_agent};
pub use embedding::embed_pending;
pub use extraction::{extract_facts, maybe_summarize_session};
pub use formatting::{
    csv_escape, default_model_url, normalize_embedding_target, parse_duration_hours, sql_quote,
    toml_to_json, truncate,
};
pub use models::{download_model, ensure_embedding_models};
pub use providers::{
    build_embedding_provider, build_embedding_provider_with_override, build_provider,
    embedding_model_id,
};
pub use session::resolve_or_create_session;
pub use sidecar::{
    build_sidecar_request, op_request, sidecar_error_response, SidecarOperationInput,
};
