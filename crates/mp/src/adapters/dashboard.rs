//! Dashboard adapter — static SPA serving and SSE stream.

use std::convert::Infallible;

use axum::extract::State;
use axum::response::IntoResponse;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures_util::stream::{self, Stream};
use tokio::sync::broadcast;

/// State for dashboard routes.
#[derive(Clone)]
pub struct DashboardState {
    pub event_tx: broadcast::Sender<DashboardEvent>,
    pub is_local: bool,
    pub api_key: Option<String>,
}

/// Serve index.html for SPA routing (dev mode).
pub async fn serve_dashboard_index(
    axum::extract::Extension(dist_path): axum::extract::Extension<std::path::PathBuf>,
) -> impl IntoResponse {
    match tokio::fs::read_to_string(dist_path.join("index.html")).await {
        Ok(html) => (
            [("content-type", "text/html")],
            html,
        )
            .into_response(),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to read index.html: {e}"),
        )
            .into_response(),
    }
}

#[derive(Clone, Debug)]
pub struct DashboardEvent {
    pub kind: String,
    pub data: serde_json::Value,
}

/// SSE stream for dashboard live updates.
/// Sends periodic keepalive; real events will be wired when activity log pushes.
pub async fn dashboard_stream(
    State(state): State<DashboardState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>> + Send> {
    let rx = state.event_tx.subscribe();
    let stream = stream::try_unfold(rx, |mut rx| async move {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    let data = serde_json::to_string(&event.data).unwrap_or_default();
                    return Ok(Some((
                        Event::default().event(event.kind.as_str()).data(data),
                        rx,
                    )));
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => return Ok(None),
            }
        }
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

#[cfg(feature = "embed-dashboard")]
pub async fn dashboard_embedded_handler(
    axum::extract::Path(path): axum::extract::Path<String>,
) -> impl axum::response::IntoResponse {
    use axum::http::{header, StatusCode};
    use axum::response::IntoResponse;

    static DASHBOARD_DIR: include_dir::Dir<'_> =
        include_dir::include_dir!("$CARGO_MANIFEST_DIR/dashboard/dist");

    let path = path.trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };

    match DASHBOARD_DIR.get_file(path) {
        Some(file) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            (
                [(header::CONTENT_TYPE, mime.as_ref().to_string())],
                file.contents(),
            )
        }
            .into_response(),
        None => {
            if let Some(index) = DASHBOARD_DIR.get_file("index.html") {
                (
                    [(header::CONTENT_TYPE, "text/html".to_string())],
                    index.contents(),
                )
                    .into_response()
            } else {
                (StatusCode::NOT_FOUND, "dashboard not built").into_response()
            }
        }
    }
}
