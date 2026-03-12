//! Serve command — MCP sidecar on stdio, HTTP on port.

use anyhow::Result;
use mp_core::config::Config;
use std::path::Path;
use std::sync::Arc;

use crate::adapters;
use crate::helpers::{
    build_embedding_provider, build_sidecar_request, embedding_model_id, open_agent_db,
    resolve_agent, sidecar_error_response,
};
use crate::sidecar;
use crate::worker::{run_scheduler, spawn_worker, WorkerBus, WorkerHandle};

pub async fn run(ctx: &crate::CommandContext<'_>, agent: Option<String>) -> Result<()> {
    let config = ctx.config;
    let config_path = ctx.config_path;
    let shutdown = tokio::sync::broadcast::channel::<()>(1).0;

    let bus = WorkerBus::new();
    let mut workers: Vec<WorkerHandle> = Vec::new();
    for ag in &config.agents {
        let (handle, w_stdin, w_stdout) = spawn_worker(config, config_path, &ag.name)?;
        tracing::info!(agent = %ag.name, pid = handle.pid, "worker started");
        bus.register(ag.name.clone(), w_stdin, w_stdout).await;
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

    let http_cfg = config.channels.http.clone().or_else(|| {
        Some(mp_core::config::HttpChannelConfig {
            port: config.gateway.port,
            api_key: None,
        })
    });
    let slack_cfg = config.channels.slack.clone();
    let discord_cfg = config.channels.discord.clone();
    let default_agent_name = config
        .agents
        .first()
        .map(|a| a.name.clone())
        .unwrap_or_else(|| "main".into());

    {
        let dispatch_clone = Arc::clone(&dispatch);
        let op_dispatch_clone = Arc::clone(&op_dispatch);
        let srv_shutdown = shutdown.subscribe();
        let da = default_agent_name.clone();
        let config_ref = Arc::new(config.clone());
        tokio::spawn(async move {
            if let Err(e) = adapters::run_http_server(
                config_ref,
                http_cfg.clone(),
                slack_cfg.clone(),
                discord_cfg.clone(),
                da,
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
        let dispatch_clone = Arc::clone(&dispatch);
        let tg_shutdown = shutdown.subscribe();
        let da = default_agent_name.clone();
        tokio::spawn(async move {
            adapters::run_telegram_polling(tg_cfg, da, dispatch_clone, tg_shutdown).await;
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

    let http_port = config
        .channels
        .http
        .as_ref()
        .map(|h| h.port)
        .unwrap_or(config.gateway.port);
    tracing::info!(
        agents = config.agents.len(),
        http_port = http_port,
        "serve mode ready — MCP on stdio, HTTP on port {http_port}"
    );

    let ag = resolve_agent(config, agent.as_deref())?;
    let conn = open_agent_db(config, &ag.name)?;
    let embed_provider = build_embedding_provider(config, ag).ok();
    let sidecar_embedding_model_id = embedding_model_id(ag);

    let stdin = tokio::io::stdin();
    let reader = tokio::io::BufReader::new(stdin);
    let mut lines = tokio::io::AsyncBufReadExt::lines(reader);
    let mut stdout = tokio::io::stdout();
    let mut shutdown_rx = shutdown.subscribe();

    loop {
        tokio::select! {
            line_result = lines.next_line() => {
                match line_result {
                    Ok(Some(line)) => {
                        let parsed: serde_json::Value = match serde_json::from_str(&line) {
                            Ok(v) => v,
                            Err(e) => {
                                let err = sidecar_error_response("invalid_json", e.to_string());
                                tokio::io::AsyncWriteExt::write_all(&mut stdout, format!("{err}\n").as_bytes()).await?;
                                tokio::io::AsyncWriteExt::flush(&mut stdout).await?;
                                continue;
                            }
                        };

                        if let Some(mcp_response) = sidecar::handle_sidecar_mcp_request(
                            &conn, &parsed, &ag.name,
                            embed_provider.as_deref(), &sidecar_embedding_model_id,
                        ).await? {
                            tokio::io::AsyncWriteExt::write_all(&mut stdout, format!("{mcp_response}\n").as_bytes()).await?;
                            tokio::io::AsyncWriteExt::flush(&mut stdout).await?;
                            continue;
                        }

                        if parsed.get("method").is_some() && parsed.get("id").is_none() {
                            continue;
                        }

                        let request = match build_sidecar_request(parsed, &ag.name) {
                            Ok(r) => r,
                            Err(e) => {
                                let err = sidecar_error_response("invalid_request", e.to_string());
                                tokio::io::AsyncWriteExt::write_all(&mut stdout, format!("{err}\n").as_bytes()).await?;
                                tokio::io::AsyncWriteExt::flush(&mut stdout).await?;
                                continue;
                            }
                        };

                        let response = match sidecar::execute_sidecar_operation(
                            &conn, &request,
                            embed_provider.as_deref(), &sidecar_embedding_model_id,
                        ).await {
                            Ok(resp) => serde_json::to_value(resp)
                                .unwrap_or_else(|e| sidecar_error_response("serialization_error", e.to_string())),
                            Err(e) => sidecar_error_response("sidecar_execute_error", e.to_string()),
                        };

                        tokio::io::AsyncWriteExt::write_all(&mut stdout, format!("{response}\n").as_bytes()).await?;
                        tokio::io::AsyncWriteExt::flush(&mut stdout).await?;
                    }
                    Ok(None) => break,
                    Err(e) => {
                        tracing::error!("stdin read error: {e}");
                        break;
                    }
                }
            }
            _ = shutdown_rx.recv() => break,
        }
    }

    tracing::info!("shutting down serve mode");
    let _ = shutdown.send(());
    scheduler_handle.abort();
    for mut w in workers {
        w.shutdown().await;
    }
    let _ = std::fs::remove_file(&pid_path);
    Ok(())
}
