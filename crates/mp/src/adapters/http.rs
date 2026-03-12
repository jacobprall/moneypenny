//! HTTP API adapter — REST, SSE, WebSocket.

use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Query, State, WebSocketUpgrade};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::stream;
use serde::{Deserialize, Serialize};
use tower_http::cors::CorsLayer;
use tracing::error;

use mp_core::config::{DiscordChannelConfig, HttpChannelConfig, SlackChannelConfig};

use super::{check_auth, dashboard, discord, slack, DispatchFn, OpDispatchFn};

#[derive(Clone)]
struct HttpState {
    dispatch: DispatchFn,
    op_dispatch: OpDispatchFn,
    default_agent: String,
    api_key: Option<String>,
}

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
    headers: axum::http::HeaderMap,
    Json(req): Json<ChatRequest>,
) -> Response {
    if !check_auth(&headers, &state.api_key) {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }
    let agent = req.agent.unwrap_or_else(|| state.default_agent.clone());
    match (state.dispatch)(agent, req.message, req.session_id).await {
        Ok((response, session_id)) => Json(ChatResponse {
            response,
            session_id,
        })
        .into_response(),
        Err(e) => {
            error!("http_chat error: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}

async fn http_ops(
    State(state): State<HttpState>,
    headers: axum::http::HeaderMap,
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
    headers: axum::http::HeaderMap,
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
                        serde_json::json!({"error": "invalid JSON"})
                            .to_string()
                            .into(),
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

async fn health() -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION")
    }))
}

/// Build and run the axum server that serves all HTTP-facing channel routes.
pub async fn run_http_server(
    config: std::sync::Arc<mp_core::config::Config>,
    http_cfg: Option<HttpChannelConfig>,
    slack_cfg: Option<SlackChannelConfig>,
    discord_cfg: Option<DiscordChannelConfig>,
    default_agent: String,
    dispatch: DispatchFn,
    op_dispatch: OpDispatchFn,
    mut shutdown: tokio::sync::broadcast::Receiver<()>,
) -> anyhow::Result<()> {
    use std::collections::HashMap;
    use tokio::sync::RwLock;

    let port = http_cfg
        .as_ref()
        .map(|c| c.port)
        .or_else(|| config.dashboard.enabled.then_some(config.gateway.port))
        .unwrap_or(8080);
    let api_key = http_cfg.as_ref().and_then(|c| c.api_key.clone());
    let api_key_clone = api_key.clone();

    let mut router: Router = Router::new()
        .route("/health", get(health))
        .layer(CorsLayer::permissive());

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

    if let Some(ref scfg) = slack_cfg {
        let slack_state = slack::SlackState {
            bot_token: scfg.bot_token.clone(),
            signing_secret: scfg.signing_secret.clone(),
            dispatch: Arc::clone(&dispatch),
            default_agent: scfg.agent.clone().unwrap_or_else(|| default_agent.clone()),
            sessions: Arc::new(RwLock::new(HashMap::new())),
        };
        let slack_router: Router = Router::new()
            .route("/slack/events", post(slack::slack_events))
            .with_state(slack_state);
        router = router.merge(slack_router);
    }

    if let Some(ref dcfg) = discord_cfg {
        let discord_state = discord::DiscordState {
            public_key: dcfg.public_key.clone(),
            dispatch: Arc::clone(&dispatch),
            default_agent: dcfg.agent.clone().unwrap_or_else(|| default_agent.clone()),
            sessions: Arc::new(RwLock::new(HashMap::new())),
        };
        let discord_router: Router = Router::new()
            .route("/discord/interactions", post(discord::discord_interactions))
            .with_state(discord_state);
        router = router.merge(discord_router);
    }

    if config.dashboard.enabled {
        let (event_tx, _) = tokio::sync::broadcast::channel::<dashboard::DashboardEvent>(64);
        let is_local = config.gateway.host == "127.0.0.1" || config.gateway.host == "localhost";
        let dist_path = config.data_dir.join("dashboard").join("dist");
        let dashboard_state = dashboard::DashboardState {
            event_tx: event_tx.clone(),
            is_local,
            api_key: api_key_clone,
        };
        let mut dashboard_router = Router::new()
            .route("/v1/dashboard/stream", get(dashboard::dashboard_stream))
            .with_state(dashboard_state);

        #[cfg(not(feature = "embed-dashboard"))]
        {
            if dist_path.exists() {
                let index_path = dist_path.join("index.html");
                let serve_dir = tower_http::services::ServeDir::new(&dist_path)
                    .not_found_service(tower_http::services::ServeFile::new(&index_path));
                dashboard_router = dashboard_router.nest_service("/dashboard", serve_dir);
            } else {
                tracing::info!("Dashboard enabled but dashboard/dist not found at {:?} — run `cd dashboard && npm run build`", dist_path);
            }
        }

        #[cfg(feature = "embed-dashboard")]
        {
            dashboard_router = dashboard_router.route(
                "/dashboard/{*path:path}",
                get(dashboard::dashboard_embedded_handler),
            );
        }

        router = router.merge(dashboard_router);
    }

    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("HTTP adapter listening on {addr}");

    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            let _ = shutdown.recv().await;
        })
        .await?;

    Ok(())
}
