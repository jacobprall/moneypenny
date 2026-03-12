//! Start command — run the gateway with workers and channels.

use anyhow::Result;
use std::sync::Arc;

use crate::adapters;
use crate::agent;
use crate::helpers::{
    build_embedding_provider, build_provider, build_sidecar_request, embed_pending,
    embedding_model_id, extract_facts, maybe_summarize_session, open_agent_db,
    resolve_agent, sidecar_error_response,
};
use crate::ui;
use crate::worker::{run_scheduler, spawn_worker, WorkerBus, WorkerHandle};

pub async fn run(ctx: &crate::CommandContext<'_>) -> Result<()> {
    ui::banner();

    let config = ctx.config;
    let shutdown = tokio::sync::broadcast::channel::<()>(1).0;

    let bus = WorkerBus::new();
    let mut workers: Vec<WorkerHandle> = Vec::new();
    for agent in &config.agents {
        let (handle, w_stdin, w_stdout) = spawn_worker(config, ctx.config_path, &agent.name)?;
        ui::info(format!("Worker \"{}\" started (pid {})", agent.name, handle.pid));
        bus.register(agent.name.clone(), w_stdin, w_stdout).await;
        workers.push(handle);
    }

    let sched_config = config.clone();
    let mut sched_shutdown = shutdown.subscribe();
    let scheduler_handle =
        tokio::spawn(async move { run_scheduler(&sched_config, &mut sched_shutdown).await });

    let bus_for_dispatch = Arc::clone(&bus);
    let dispatch: adapters::DispatchFn = Arc::new(move |agent, message, session_id| {
        let bus = Arc::clone(&bus_for_dispatch);
        Box::pin(async move {
            bus.route_full(&agent, &message, session_id.as_deref())
                .await
        })
    });

    let config_for_ops = config.clone();
    let op_dispatch: adapters::OpDispatchFn = Arc::new(move |payload| {
        let config = config_for_ops.clone();
        Box::pin(async move {
            let default_agent = config
                .agents
                .first()
                .map(|a| a.name.clone())
                .unwrap_or_else(|| "main".into());

            let req = match build_sidecar_request(payload, &default_agent) {
                Ok(r) => r,
                Err(e) => return Ok(sidecar_error_response("invalid_request", e.to_string())),
            };

            // config.get is handled here (needs Config, not DB)
            if req.op == "config.get" {
                match config.to_json_redacted() {
                    Ok(data) => {
                        return Ok(serde_json::json!({
                            "ok": true,
                            "code": "ok",
                            "message": "config",
                            "data": data,
                            "policy": null,
                            "audit": { "recorded": false }
                        }));
                    }
                    Err(e) => {
                        return Ok(sidecar_error_response("config_error", e.to_string()));
                    }
                }
            }

            let conn = match open_agent_db(&config, &req.actor.agent_id) {
                Ok(c) => c,
                Err(e) => return Ok(sidecar_error_response("invalid_agent", e.to_string())),
            };

            // Inject server-side args for ops that need config
            let mut req = req;
            if req.op == "db.stats" {
                req.args["data_dir"] =
                    serde_json::Value::String(config.data_dir.to_string_lossy().to_string());
            }
            if req.op == "sync.status" {
                req.args["tables"] = serde_json::to_value(config.sync.tables.clone())
                    .unwrap_or_else(|_| serde_json::json!([]));
            }

            let resp = match mp_core::operations::execute(&conn, &req) {
                Ok(r) => r,
                Err(e) => {
                    return Ok(sidecar_error_response(
                        "http_ops_execute_error",
                        e.to_string(),
                    ));
                }
            };

            Ok(serde_json::to_value(resp)
                .unwrap_or_else(|e| sidecar_error_response("serialization_error", e.to_string())))
        })
    });

    let has_http_channel = config.channels.http.is_some()
        || config.channels.slack.is_some()
        || config.channels.discord.is_some()
        || config.dashboard.enabled;

    if has_http_channel {
        let config_for_http = Arc::new(config.clone());
        let http_cfg = config_for_http.channels.http.clone();
        let slack_cfg = config_for_http.channels.slack.clone();
        let discord_cfg = config_for_http.channels.discord.clone();
        let default_agent = config_for_http
            .agents
            .first()
            .map(|a| a.name.clone())
            .unwrap_or_else(|| "main".into());
        let dispatch_clone = Arc::clone(&dispatch);
        let op_dispatch_clone = Arc::clone(&op_dispatch);
        let srv_shutdown = shutdown.subscribe();
        tokio::spawn(async move {
            if let Err(e) = adapters::run_http_server(
                config_for_http,
                http_cfg,
                slack_cfg,
                discord_cfg,
                default_agent,
                dispatch_clone,
                op_dispatch_clone,
                srv_shutdown,
            )
            .await
            {
                tracing::error!("HTTP server error: {e}");
            }
        });
    }

    if let Some(tg_cfg) = config.channels.telegram.clone() {
        let default_agent = config
            .agents
            .first()
            .map(|a| a.name.clone())
            .unwrap_or_else(|| "main".into());
        let dispatch_clone = Arc::clone(&dispatch);
        let tg_shutdown = shutdown.subscribe();
        tokio::spawn(async move {
            adapters::run_telegram_polling(tg_cfg, default_agent, dispatch_clone, tg_shutdown)
                .await;
        });
    }

    let has_sync = config.sync.interval_secs > 0
        && (!config.sync.peers.is_empty() || config.sync.cloud_url.is_some());
    if has_sync {
        let sync_config = config.sync.clone();
        let sync_data_dir = config.data_dir.clone();
        let sync_agents: Vec<String> = config.agents.iter().map(|a| a.name.clone()).collect();
        let mut sync_shutdown = shutdown.subscribe();
        tokio::spawn(async move {
            let interval = std::time::Duration::from_secs(sync_config.interval_secs);
            let tables: Vec<&str> = sync_config.tables.iter().map(String::as_str).collect();
            loop {
                tokio::select! {
                    _ = tokio::time::sleep(interval) => {}
                    _ = sync_shutdown.recv() => break,
                }
                for agent_name in &sync_agents {
                    let db_path = sync_data_dir.join(format!("{agent_name}.db"));
                    let conn = match rusqlite::Connection::open(&db_path) {
                        Ok(c) => c,
                        Err(e) => {
                            tracing::warn!("sync: cannot open {agent_name}: {e}");
                            continue;
                        }
                    };
                    if let Err(e) = mp_ext::init_all_extensions(&conn) {
                        tracing::warn!("sync: ext init for {agent_name}: {e}");
                        continue;
                    }
                    let _ = mp_core::sync::init_sync_tables(&conn, &tables);
                    for peer in &sync_config.peers {
                        let peer_path =
                            if std::path::Path::new(peer).is_absolute() || peer.ends_with(".db") {
                                std::path::PathBuf::from(peer)
                            } else {
                                sync_data_dir.join(format!("{peer}.db"))
                            };
                        if !peer_path.exists() {
                            continue;
                        }
                        let peer_conn = match rusqlite::Connection::open(&peer_path).and_then(|c| {
                            mp_ext::init_all_extensions(&c).ok();
                            Ok(c)
                        }) {
                            Ok(c) => c,
                            Err(e) => {
                                tracing::warn!("auto-sync: cannot open peer {peer}: {e}");
                                continue;
                            }
                        };
                        let _ = mp_core::sync::init_sync_tables(&peer_conn, &tables);
                        match mp_core::sync::local_sync_bidirectional(
                            &conn,
                            &peer_conn,
                            agent_name,
                            peer,
                            &tables,
                        ) {
                            Ok(r) => {
                                tracing::debug!(agent = %agent_name, peer = %peer, sent = r.sent, received = r.received, "auto-sync")
                            }
                            Err(e) => {
                                tracing::warn!(agent = %agent_name, peer = %peer, "auto-sync error: {e}")
                            }
                        }
                    }
                    if let Some(ref url) = sync_config.cloud_url {
                        match mp_core::sync::cloud_sync(&conn, url) {
                            Ok(r) => {
                                tracing::debug!(agent = %agent_name, batches = r.sent, "cloud auto-sync")
                            }
                            Err(e) => tracing::warn!(agent = %agent_name, "cloud sync error: {e}"),
                        }
                    }
                }
            }
        });
    }

    let pid_path = config.data_dir.join("mp.pid");
    std::fs::write(&pid_path, std::process::id().to_string())?;

    ui::blank();
    ui::info(format!("Gateway ready. {} agent(s) running.", config.agents.len()));
    if has_http_channel {
        let port = config
            .channels
            .http
            .as_ref()
            .map(|h| h.port)
            .or_else(|| config.dashboard.enabled.then_some(config.gateway.port))
            .unwrap_or(8080);
        let mut endpoints = vec!["POST /v1/chat", "POST /v1/ops", "WS /v1/ws", "GET /health"];
        if config.dashboard.enabled {
            endpoints.push("GET /dashboard");
        }
        ui::info(format!("HTTP API listening on port {port}  ({})", endpoints.join(", ")));
    }
    if config.channels.slack.is_some() {
        ui::info("Slack Events API endpoint: POST /slack/events");
    }
    if config.channels.discord.is_some() {
        ui::info("Discord Interactions endpoint: POST /discord/interactions");
    }
    if config.channels.telegram.is_some() {
        ui::info("Telegram long-polling active");
    }
    if has_sync {
        ui::info(format!(
            "Auto-sync every {}s ({} peer(s){})",
            config.sync.interval_secs,
            config.sync.peers.len(),
            if config.sync.cloud_url.is_some() {
                " + cloud"
            } else {
                ""
            }
        ));
    }
    ui::info("Press Ctrl-C to shut down.");
    ui::blank();

    if config.channels.cli {
        let default_agent = config
            .agents
            .first()
            .map(|a| a.name.clone())
            .unwrap_or_else(|| "main".into());
        let ag = resolve_agent(config, Some(&default_agent))?;
        let conn = open_agent_db(config, &ag.name)?;
        let provider = build_provider(ag)?;
        let embed = build_embedding_provider(config, ag).ok();
        let sid = mp_core::store::log::create_session(&conn, &ag.name, Some("cli"))?;

        ui::info(format!("CLI channel active — agent: {}", ag.name));
        ui::info("Type /help for commands, Ctrl-C to shut down.");
        ui::blank();

        let mut shutdown_rx = shutdown.subscribe();
        let stdin = tokio::io::stdin();
        let mut reader = tokio::io::BufReader::new(stdin);

        loop {
            ui::prompt();

            let mut line = String::new();
            let read = tokio::select! {
                r = tokio::io::AsyncBufReadExt::read_line(&mut reader, &mut line) => r?,
                _ = shutdown_rx.recv() => break,
            };

            if read == 0 {
                break;
            }
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if trimmed == "/quit" || trimmed == "/exit" {
                break;
            }

            if trimmed == "/help" {
                ui::info("/facts    — list stored facts");
                ui::info("/scratch  — list scratch entries");
                ui::info("/quit     — exit");
                ui::blank();
                continue;
            }
            if trimmed == "/facts" {
                let facts = mp_core::store::facts::list_active(&conn, &ag.name)?;
                if facts.is_empty() {
                    ui::info("No facts stored.");
                } else {
                    for f in &facts {
                        ui::info(format!("[{:.1}] {}", f.confidence, f.pointer));
                    }
                }
                ui::blank();
                continue;
            }

            let req_ctx = crate::context::RequestContext {
                agent_id: &ag.name,
                conn: &conn,
                session_id: &sid,
                embed_provider: embed.as_deref(),
                policy_mode: ag.policy_mode(),
                persona: ag.persona.as_deref(),
                worker_bus: Some(&bus),
            };
            match agent::agent_turn(&req_ctx, provider.as_ref(), trimmed)
            .await
            {
                Ok(response) => {
                    ui::blank();
                    for l in response.lines() {
                        ui::info(l);
                    }
                    ui::blank();
                    if let Ok(n) = extract_facts(&conn, provider.as_ref(), &ag.name, &sid).await {
                        if n > 0 {
                            ui::dim(format!("({n} fact{} learned)", if n == 1 { "" } else { "s" }));
                            ui::blank();
                        }
                    }
                    if let Some(ref ep) = embed {
                        let model_id = embedding_model_id(ag);
                        embed_pending(&conn, ep.as_ref(), &ag.name, &model_id).await;
                    }
                    maybe_summarize_session(&conn, provider.as_ref(), &sid).await;
                }
                Err(e) => {
                    ui::error(e);
                    ui::blank();
                }
            }
        }
    } else {
        tokio::signal::ctrl_c().await?;
    }

    println!();
    ui::info("Shutting down...");
    let _ = shutdown.send(());
    scheduler_handle.abort();

    for mut w in workers {
        w.shutdown().await;
    }

    let _ = std::fs::remove_file(&pid_path);
    ui::info("Goodbye.");
    Ok(())
}
