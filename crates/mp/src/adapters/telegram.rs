//! Telegram long-polling adapter.

use std::collections::HashMap;
use std::sync::Arc;

use mp_core::config::TelegramChannelConfig;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

use super::DispatchFn;

pub async fn run_telegram_polling(
    cfg: TelegramChannelConfig,
    default_agent: String,
    dispatch: DispatchFn,
    mut shutdown: tokio::sync::broadcast::Receiver<()>,
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
