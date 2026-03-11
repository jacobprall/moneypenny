//! Slack Events API adapter.

use std::collections::HashMap;
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use tokio::sync::RwLock;
use tracing::error;

use super::DispatchFn;

#[derive(Clone)]
pub struct SlackState {
    pub bot_token: String,
    pub signing_secret: Option<String>,
    pub dispatch: DispatchFn,
    pub default_agent: String,
    pub sessions: Arc<RwLock<HashMap<String, String>>>,
}

#[derive(Deserialize)]
pub struct SlackPayload {
    #[serde(rename = "type")]
    pub kind: String,
    pub challenge: Option<String>,
    pub event: Option<SlackEvent>,
}

#[derive(Deserialize)]
pub struct SlackEvent {
    #[serde(rename = "type")]
    pub kind: String,
    pub user: Option<String>,
    pub text: Option<String>,
    pub channel: Option<String>,
    pub thread_ts: Option<String>,
    pub bot_id: Option<String>,
}

pub fn verify_slack_signature(headers: &HeaderMap, body: &[u8], signing_secret: &str) -> bool {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    let timestamp = match headers
        .get("x-slack-request-timestamp")
        .and_then(|v| v.to_str().ok())
    {
        Some(ts) => ts,
        None => return false,
    };
    let sig_header = match headers
        .get("x-slack-signature")
        .and_then(|v| v.to_str().ok())
    {
        Some(s) => s,
        None => return false,
    };

    let base = format!(
        "v0:{}:{}",
        timestamp,
        std::str::from_utf8(body).unwrap_or("")
    );
    let mut mac = Hmac::<Sha256>::new_from_slice(signing_secret.as_bytes())
        .expect("HMAC accepts any key size");
    mac.update(base.as_bytes());
    let result = mac.finalize().into_bytes();
    let expected = format!("v0={}", hex::encode(result));

    expected.len() == sig_header.len()
        && expected
            .bytes()
            .zip(sig_header.bytes())
            .fold(0u8, |acc, (a, b)| acc | (a ^ b))
            == 0
}

pub async fn slack_events(
    State(state): State<SlackState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Some(ref secret) = state.signing_secret {
        if !verify_slack_signature(&headers, &body, secret) {
            return (StatusCode::UNAUTHORIZED, "invalid signature").into_response();
        }
    }

    let payload: SlackPayload = match serde_json::from_slice(&body) {
        Ok(p) => p,
        Err(_) => return (StatusCode::BAD_REQUEST, "invalid JSON").into_response(),
    };

    if payload.kind == "url_verification" {
        if let Some(ch) = payload.challenge {
            return axum::Json(serde_json::json!({"challenge": ch})).into_response();
        }
    }

    if payload.kind == "event_callback" {
        if let Some(event) = payload.event {
            if event.bot_id.is_some() {
                return StatusCode::OK.into_response();
            }
            if event.kind == "app_mention" || event.kind == "message" {
                let text = event.text.unwrap_or_default();
                let user_id = event.user.unwrap_or_else(|| "unknown".into());
                let channel = event.channel.unwrap_or_default();
                let thread_ts = event.thread_ts;

                let session_id = state.sessions.read().await.get(&user_id).cloned();
                let dispatch = Arc::clone(&state.dispatch);
                let agent = state.default_agent.clone();
                let bot_token = state.bot_token.clone();
                let sessions = Arc::clone(&state.sessions);

                tokio::spawn(async move {
                    match dispatch(agent, text, session_id).await {
                        Ok((response, new_session_id)) => {
                            sessions.write().await.insert(user_id, new_session_id);
                            if let Err(e) =
                                slack_post_message(&bot_token, &channel, &response, thread_ts.as_deref()).await
                            {
                                error!("Slack postMessage failed: {e}");
                            }
                        }
                        Err(e) => error!("Slack dispatch error: {e}"),
                    }
                });
            }
        }
    }

    StatusCode::OK.into_response()
}

async fn slack_post_message(
    bot_token: &str,
    channel: &str,
    text: &str,
    thread_ts: Option<&str>,
) -> anyhow::Result<()> {
    let mut body = serde_json::json!({"channel": channel, "text": text});
    if let Some(ts) = thread_ts {
        body["thread_ts"] = serde_json::Value::String(ts.to_string());
    }
    let resp = reqwest::Client::new()
        .post("https://slack.com/api/chat.postMessage")
        .bearer_auth(bot_token)
        .json(&body)
        .send()
        .await?;
    let json: serde_json::Value = resp.json().await?;
    if json["ok"].as_bool() != Some(true) {
        anyhow::bail!(
            "Slack API error: {}",
            json["error"].as_str().unwrap_or("unknown")
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slack_sig_rejects_missing_headers() {
        let headers = HeaderMap::new();
        assert!(!verify_slack_signature(&headers, b"body", "secret"));
    }
}
