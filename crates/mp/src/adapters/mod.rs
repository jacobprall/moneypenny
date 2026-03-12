//! Channel adapters: HTTP API (REST + SSE + WebSocket), Slack Events API,
//! Discord Interactions, and Telegram long-polling.
//!
//! All adapters share a `DispatchFn` — an async closure that routes a message
//! to the appropriate agent worker via the `WorkerBus` and returns the
//! `(response, session_id)` pair.

mod dashboard;
mod discord;
mod http;
mod slack;
mod telegram;

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use axum::http::HeaderMap;

pub use http::run_http_server;
pub use telegram::run_telegram_polling;

// ---------------------------------------------------------------------------
// Dispatcher types (shared)
// ---------------------------------------------------------------------------

/// Async function that sends a message to an agent and returns `(response, session_id)`.
pub type DispatchFn = Arc<
    dyn Fn(
            String,
            String,
            Option<String>,
        ) -> Pin<Box<dyn Future<Output = anyhow::Result<(String, String)>> + Send>>
        + Send
        + Sync,
>;

/// Async function that executes a canonical operation request payload and
/// returns the canonical operation response as JSON.
pub type OpDispatchFn = Arc<
    dyn Fn(
            serde_json::Value,
        ) -> Pin<Box<dyn Future<Output = anyhow::Result<serde_json::Value>> + Send>>
        + Send
        + Sync,
>;

// ---------------------------------------------------------------------------
// Auth helper (shared by HTTP; usable by other adapters)
// ---------------------------------------------------------------------------

pub(crate) fn check_auth(headers: &HeaderMap, expected: &Option<String>) -> bool {
    match expected {
        None => true,
        Some(key) => headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(|tok| tok.trim() == key.as_str())
            .unwrap_or(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_auth_no_key_always_passes() {
        let headers = HeaderMap::new();
        assert!(check_auth(&headers, &None));
    }

    #[test]
    fn check_auth_correct_key_passes() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer secret123".parse().unwrap());
        assert!(check_auth(&headers, &Some("secret123".into())));
    }

    #[test]
    fn check_auth_wrong_key_fails() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer wrong".parse().unwrap());
        assert!(!check_auth(&headers, &Some("secret123".into())));
    }

    #[test]
    fn check_auth_missing_header_fails() {
        let headers = HeaderMap::new();
        assert!(!check_auth(&headers, &Some("secret123".into())));
    }
}
