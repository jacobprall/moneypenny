/// Channel adapters: HTTP API (REST + SSE + WebSocket), Slack Events API,
/// Discord Interactions, and Telegram long-polling.
///
/// All adapters share a `DispatchFn` — an async closure that routes a message
/// to the appropriate agent worker via the `WorkerBus` and returns the
/// `(response, session_id)` pair.
use std::collections::HashMap;
use std::future::Future;

use std::pin::Pin;
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Query, State, WebSocketUpgrade};
use axum::extract::ws::{Message, WebSocket};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::stream;
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, RwLock};
use tower_http::cors::CorsLayer;
use tracing::{error, info, warn};

use mp_core::config::{
    DiscordChannelConfig, HttpChannelConfig, SlackChannelConfig, TelegramChannelConfig,
};

// ---------------------------------------------------------------------------
// Dispatcher type
// ---------------------------------------------------------------------------

/// Async function that sends a message to an agent and returns `(response, session_id)`.
pub type DispatchFn = Arc<
    dyn Fn(String, String, Option<String>)
            -> Pin<Box<dyn Future<Output = anyhow::Result<(String, String)>> + Send>>
        + Send
        + Sync,
>;

/// Async function that executes a canonical operation request payload and
/// returns the canonical operation response as JSON.
pub type OpDispatchFn = Arc<
    dyn Fn(serde_json::Value)
            -> Pin<Box<dyn Future<Output = anyhow::Result<serde_json::Value>> + Send>>
        + Send
        + Sync,
>;

// ---------------------------------------------------------------------------
// Auth helper
// ---------------------------------------------------------------------------

fn check_auth(headers: &HeaderMap, expected: &Option<String>) -> bool {
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

// ---------------------------------------------------------------------------
// HTTP API adapter
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct HttpState {
    dispatch: DispatchFn,
    op_dispatch: OpDispatchFn,
    default_agent: String,
    api_key: Option<String>,
}

// POST /v1/chat

#[derive(Deserialize)]
struct ChatRequest {
    message: String,
    #[serde(default)]
    agent: Option<String>,
    #[serde(default)]
    session_id: Option<String>,
}

#[derive(Serialize)]
struct ChatResponse {
    response: String,
    session_id: String,
}

async fn http_chat(
    State(state): State<HttpState>,
    headers: HeaderMap,
    Json(req): Json<ChatRequest>,
) -> Response {
    if !check_auth(&headers, &state.api_key) {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }
    let agent = req.agent.unwrap_or_else(|| state.default_agent.clone());
    match (state.dispatch)(agent, req.message, req.session_id).await {
        Ok((response, session_id)) => Json(ChatResponse { response, session_id }).into_response(),
        Err(e) => {
            error!("http_chat error: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}

// POST /v1/ops
//
// Canonical operation HTTP parity endpoint.
async fn http_ops(
    State(state): State<HttpState>,
    headers: HeaderMap,
    Json(req): Json<serde_json::Value>,
) -> Response {
    if !check_auth(&headers, &state.api_key) {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }

    match (state.op_dispatch)(req).await {
        Ok(resp) => Json(resp).into_response(),
        Err(e) => {
            error!("http_ops error: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "ok": false,
                    "code": "http_ops_error",
                    "message": e.to_string(),
                    "data": {},
                    "policy": null,
                    "audit": { "recorded": false }
                })),
            )
                .into_response()
        }
    }
}

// GET /v1/chat/stream  (SSE)

#[derive(Deserialize)]
struct StreamQuery {
    message: String,
    #[serde(default)]
    agent: Option<String>,
    #[serde(default)]
    session_id: Option<String>,
}

async fn http_chat_stream(
    State(state): State<HttpState>,
    headers: HeaderMap,
    Query(params): Query<StreamQuery>,
) -> Response {
    if !check_auth(&headers, &state.api_key) {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }
    let agent = params.agent.unwrap_or_else(|| state.default_agent.clone());
    let dispatch = Arc::clone(&state.dispatch);

    let sse = Sse::new(stream::once(async move {
        let result = dispatch(agent, params.message, params.session_id).await;
        let data = match result {
            Ok((response, session_id)) => {
                serde_json::json!({"response": response, "session_id": session_id}).to_string()
            }
            Err(e) => serde_json::json!({"error": e.to_string()}).to_string(),
        };
        Ok::<Event, std::convert::Infallible>(Event::default().data(data))
    }))
    .keep_alive(KeepAlive::default());

    sse.into_response()
}

// GET /v1/ws  (WebSocket)

async fn http_ws(State(state): State<HttpState>, ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(move |socket| ws_handler(socket, state))
}

async fn ws_handler(mut socket: WebSocket, state: HttpState) {
    while let Some(Ok(msg)) = socket.recv().await {
        let text = match msg {
            Message::Text(t) => t,
            Message::Close(_) => break,
            _ => continue,
        };

        let req: serde_json::Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(_) => {
                let _ = socket
                    .send(Message::Text(
                        serde_json::json!({"error": "invalid JSON"}).to_string().into(),
                    ))
                    .await;
                continue;
            }
        };

        let message = req["message"].as_str().unwrap_or("").to_string();
        let agent = req["agent"]
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| state.default_agent.clone());
        let session_id = req["session_id"].as_str().map(str::to_string);

        let out = match (state.dispatch)(agent, message, session_id).await {
            Ok((response, session_id)) => {
                serde_json::json!({"response": response, "session_id": session_id}).to_string()
            }
            Err(e) => serde_json::json!({"error": e.to_string()}).to_string(),
        };
        let _ = socket.send(Message::Text(out.into())).await;
    }
}

// GET /health

async fn health() -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION")
    }))
}

// ---------------------------------------------------------------------------
// Slack Events API adapter
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct SlackState {
    bot_token: String,
    signing_secret: Option<String>,
    dispatch: DispatchFn,
    default_agent: String,
    sessions: Arc<RwLock<HashMap<String, String>>>,
}

#[derive(Deserialize)]
struct SlackPayload {
    #[serde(rename = "type")]
    kind: String,
    challenge: Option<String>,
    event: Option<SlackEvent>,
}

#[derive(Deserialize)]
struct SlackEvent {
    #[serde(rename = "type")]
    kind: String,
    user: Option<String>,
    text: Option<String>,
    channel: Option<String>,
    thread_ts: Option<String>,
    bot_id: Option<String>,
}

fn verify_slack_signature(headers: &HeaderMap, body: &[u8], signing_secret: &str) -> bool {
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

    let base = format!("v0:{}:{}", timestamp, std::str::from_utf8(body).unwrap_or(""));
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

async fn slack_events(
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
            return Json(serde_json::json!({"challenge": ch})).into_response();
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
                            if let Err(e) = slack_post_message(
                                &bot_token,
                                &channel,
                                &response,
                                thread_ts.as_deref(),
                            )
                            .await
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

// ---------------------------------------------------------------------------
// Discord Interactions adapter
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct DiscordState {
    public_key: String,
    dispatch: DispatchFn,
    default_agent: String,
    sessions: Arc<RwLock<HashMap<String, String>>>,
}

fn verify_discord_signature(headers: &HeaderMap, body: &[u8], public_key_hex: &str) -> bool {
    use ed25519_dalek::{Signature, VerifyingKey};

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

    let sig_bytes: [u8; 64] = match hex::decode(sig_hex)
        .ok()
        .and_then(|b| b.try_into().ok())
    {
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

    use ed25519_dalek::Verifier;
    vk.verify(&msg, &signature).is_ok()
}

async fn discord_interactions(
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

    // PING — Discord endpoint verification
    if kind == 1 {
        return Json(serde_json::json!({"type": 1})).into_response();
    }

    // APPLICATION_COMMAND — slash command
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
            let app_id = interaction["application_id"].as_str().unwrap_or("").to_string();

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

            // Deferred channel message — tells Discord we'll respond later
            return Json(serde_json::json!({"type": 5})).into_response();
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

// ---------------------------------------------------------------------------
// Combined HTTP server (HTTP API + Slack + Discord on one port)
// ---------------------------------------------------------------------------

/// Build and run the axum server that serves all HTTP-facing channel routes.
pub async fn run_http_server(
    http_cfg: Option<&HttpChannelConfig>,
    slack_cfg: Option<&SlackChannelConfig>,
    discord_cfg: Option<&DiscordChannelConfig>,
    default_agent: String,
    dispatch: DispatchFn,
    op_dispatch: OpDispatchFn,
    mut shutdown: broadcast::Receiver<()>,
) -> anyhow::Result<()> {
    let port = http_cfg.map(|c| c.port).unwrap_or(8080);
    let api_key = http_cfg.and_then(|c| c.api_key.clone());

    // Health is stateless — no state needed
    let mut router: Router = Router::new()
        .route("/health", get(health))
        .layer(CorsLayer::permissive());

    // HTTP API routes
    let http_state = HttpState {
        dispatch: Arc::clone(&dispatch),
        op_dispatch,
        default_agent: default_agent.clone(),
        api_key,
    };
    let http_router: Router = Router::new()
        .route("/v1/chat", post(http_chat))
        .route("/v1/ops", post(http_ops))
        .route("/v1/chat/stream", get(http_chat_stream))
        .route("/v1/ws", get(http_ws))
        .with_state(http_state);
    router = router.merge(http_router);

    // Slack routes
    if let Some(scfg) = slack_cfg {
        let slack_state = SlackState {
            bot_token: scfg.bot_token.clone(),
            signing_secret: scfg.signing_secret.clone(),
            dispatch: Arc::clone(&dispatch),
            default_agent: scfg.agent.clone().unwrap_or_else(|| default_agent.clone()),
            sessions: Arc::new(RwLock::new(HashMap::new())),
        };
        let slack_router: Router = Router::new()
            .route("/slack/events", post(slack_events))
            .with_state(slack_state);
        router = router.merge(slack_router);
    }

    // Discord routes
    if let Some(dcfg) = discord_cfg {
        let discord_state = DiscordState {
            public_key: dcfg.public_key.clone(),
            dispatch: Arc::clone(&dispatch),
            default_agent: dcfg.agent.clone().unwrap_or_else(|| default_agent.clone()),
            sessions: Arc::new(RwLock::new(HashMap::new())),
        };
        let discord_router: Router = Router::new()
            .route("/discord/interactions", post(discord_interactions))
            .with_state(discord_state);
        router = router.merge(discord_router);
    }

    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    info!("HTTP adapter listening on {addr}");

    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            let _ = shutdown.recv().await;
        })
        .await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Telegram long-polling adapter
// ---------------------------------------------------------------------------

pub async fn run_telegram_polling(
    cfg: TelegramChannelConfig,
    default_agent: String,
    dispatch: DispatchFn,
    mut shutdown: broadcast::Receiver<()>,
) {
    let agent = cfg.agent.unwrap_or(default_agent);
    let base = format!("https://api.telegram.org/bot{}", cfg.bot_token);
    let client = reqwest::Client::new();
    let sessions: Arc<RwLock<HashMap<i64, String>>> = Arc::new(RwLock::new(HashMap::new()));
    let mut offset: i64 = 0;

    info!("Telegram adapter polling for updates");

    loop {
        let poll_fut = client
            .get(format!("{base}/getUpdates"))
            .query(&[
                ("offset", offset.to_string()),
                ("timeout", "20".into()),
                ("allowed_updates", r#"["message"]"#.into()),
            ])
            .send();

        let result = tokio::select! {
            r = poll_fut => r,
            _ = shutdown.recv() => {
                info!("Telegram adapter shutting down");
                return;
            }
        };

        let response = match result {
            Ok(r) => r,
            Err(e) => {
                warn!("Telegram getUpdates error: {e}");
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                continue;
            }
        };

        let body: serde_json::Value = match response.json().await {
            Ok(v) => v,
            Err(e) => {
                warn!("Telegram response parse error: {e}");
                continue;
            }
        };

        if body["ok"].as_bool() != Some(true) {
            warn!("Telegram API not ok: {body}");
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            continue;
        }

        let updates = match body["result"].as_array() {
            Some(arr) => arr.clone(),
            None => continue,
        };

        for update in &updates {
            let update_id = update["update_id"].as_i64().unwrap_or(0);
            offset = offset.max(update_id + 1);

            let msg = &update["message"];
            let chat_id = match msg["chat"]["id"].as_i64() {
                Some(id) => id,
                None => continue,
            };
            let text = match msg["text"].as_str() {
                Some(t) => t.to_string(),
                None => continue,
            };

            let session_id = sessions.read().await.get(&chat_id).cloned();
            let dispatch_clone = Arc::clone(&dispatch);
            let agent_clone = agent.clone();
            let sessions_clone = Arc::clone(&sessions);
            let base_clone = base.clone();
            let client_clone = client.clone();

            tokio::spawn(async move {
                match dispatch_clone(agent_clone, text, session_id).await {
                    Ok((response, new_session_id)) => {
                        sessions_clone.write().await.insert(chat_id, new_session_id);
                        if let Err(e) =
                            telegram_send_message(&client_clone, &base_clone, chat_id, &response)
                                .await
                        {
                            error!("Telegram sendMessage failed: {e}");
                        }
                    }
                    Err(e) => error!("Telegram dispatch error: {e}"),
                }
            });
        }
    }
}

async fn telegram_send_message(
    client: &reqwest::Client,
    base: &str,
    chat_id: i64,
    text: &str,
) -> anyhow::Result<()> {
    let resp = client
        .post(format!("{base}/sendMessage"))
        .json(&serde_json::json!({"chat_id": chat_id, "text": text}))
        .send()
        .await?;
    let body: serde_json::Value = resp.json().await?;
    if body["ok"].as_bool() != Some(true) {
        anyhow::bail!(
            "Telegram sendMessage error: {}",
            body["description"].as_str().unwrap_or("unknown")
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

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

    #[test]
    fn discord_sig_rejects_missing_headers() {
        let headers = HeaderMap::new();
        assert!(!verify_discord_signature(&headers, b"body", "deadbeef"));
    }

    #[test]
    fn slack_sig_rejects_missing_headers() {
        let headers = HeaderMap::new();
        assert!(!verify_slack_signature(&headers, b"body", "secret"));
    }
}
