//! Discord Interactions adapter.

use std::collections::HashMap;
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use tokio::sync::RwLock;
use tracing::error;

use super::DispatchFn;

#[derive(Clone)]
pub struct DiscordState {
    pub public_key: String,
    pub dispatch: DispatchFn,
    pub default_agent: String,
    pub sessions: Arc<RwLock<HashMap<String, String>>>,
}

pub fn verify_discord_signature(headers: &HeaderMap, body: &[u8], public_key_hex: &str) -> bool {
    use ed25519_dalek::{Signature, Verifier, VerifyingKey};

    let sig_hex = match headers
        .get("x-signature-ed25519")
        .and_then(|v| v.to_str().ok())
    {
        Some(s) => s,
        None => return false,
    };
    let timestamp = match headers
        .get("x-signature-timestamp")
        .and_then(|v| v.to_str().ok())
    {
        Some(t) => t,
        None => return false,
    };

    let sig_bytes: [u8; 64] = match hex::decode(sig_hex).ok().and_then(|b| b.try_into().ok()) {
        Some(a) => a,
        None => return false,
    };
    let key_bytes: [u8; 32] = match hex::decode(public_key_hex)
        .ok()
        .and_then(|b| b.try_into().ok())
    {
        Some(a) => a,
        None => return false,
    };

    let signature = Signature::from_bytes(&sig_bytes);
    let vk = match VerifyingKey::from_bytes(&key_bytes) {
        Ok(k) => k,
        Err(_) => return false,
    };

    let mut msg = timestamp.as_bytes().to_vec();
    msg.extend_from_slice(body);

    vk.verify(&msg, &signature).is_ok()
}

pub async fn discord_interactions(
    State(state): State<DiscordState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if !verify_discord_signature(&headers, &body, &state.public_key) {
        return (StatusCode::UNAUTHORIZED, "invalid signature").into_response();
    }

    let interaction: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => return (StatusCode::BAD_REQUEST, "invalid JSON").into_response(),
    };

    let kind = interaction["type"].as_u64().unwrap_or(0);

    if kind == 1 {
        return axum::Json(serde_json::json!({"type": 1})).into_response();
    }

    if kind == 2 {
        let data = &interaction["data"];
        let message = data["options"]
            .as_array()
            .and_then(|opts| opts.iter().find(|o| o["name"] == "message"))
            .and_then(|o| o["value"].as_str())
            .unwrap_or("")
            .to_string();

        if !message.is_empty() {
            let user_id = interaction["member"]["user"]["id"]
                .as_str()
                .or_else(|| interaction["user"]["id"].as_str())
                .unwrap_or("unknown")
                .to_string();

            let session_id = state.sessions.read().await.get(&user_id).cloned();
            let dispatch = Arc::clone(&state.dispatch);
            let agent = state.default_agent.clone();
            let sessions = Arc::clone(&state.sessions);
            let interaction_token = interaction["token"].as_str().unwrap_or("").to_string();
            let app_id = interaction["application_id"]
                .as_str()
                .unwrap_or("")
                .to_string();

            tokio::spawn(async move {
                match dispatch(agent, message, session_id).await {
                    Ok((response, new_session_id)) => {
                        sessions.write().await.insert(user_id, new_session_id);
                        if let Err(e) =
                            discord_send_followup(&app_id, &interaction_token, &response).await
                        {
                            error!("Discord followup failed: {e}");
                        }
                    }
                    Err(e) => error!("Discord dispatch error: {e}"),
                }
            });

            return axum::Json(serde_json::json!({"type": 5})).into_response();
        }
    }

    StatusCode::OK.into_response()
}

async fn discord_send_followup(
    app_id: &str,
    interaction_token: &str,
    content: &str,
) -> anyhow::Result<()> {
    let url = format!("https://discord.com/api/v10/webhooks/{app_id}/{interaction_token}");
    let resp = reqwest::Client::new()
        .post(&url)
        .json(&serde_json::json!({"content": content}))
        .send()
        .await?;
    if !resp.status().is_success() {
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("Discord API error: {text}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discord_sig_rejects_missing_headers() {
        let headers = HeaderMap::new();
        assert!(!verify_discord_signature(&headers, b"body", "deadbeef"));
    }
}
