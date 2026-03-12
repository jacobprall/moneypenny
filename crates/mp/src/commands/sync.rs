//! Sync command — status, now, push, pull, connect.

use anyhow::Result;
use mp_core::config::Config;

use crate::cli;
use crate::helpers::{open_agent_db, resolve_agent};
use crate::ui;

pub async fn run(ctx: &crate::context::CommandContext<'_>, cmd: cli::SyncCommand) -> Result<()> {
    let config = ctx.config;
    let sync_tables: Vec<&str> = config.sync.tables.iter().map(String::as_str).collect();

    match cmd {
        cli::SyncCommand::Status { agent } => {
            let ag = resolve_agent(config, agent.as_deref())?;
            let conn = open_agent_db(config, &ag.name)?;
            let st = mp_core::sync::status(&conn, &sync_tables)?;
            ui::blank();
            ui::info(format!("Sync status for agent \"{}\"", ag.name));
            println!("{st}");
        }

        cli::SyncCommand::Now { agent } => {
            let ag = resolve_agent(config, agent.as_deref())?;
            let conn = open_agent_db(config, &ag.name)?;

            let mut total_sent = 0usize;
            let mut total_received = 0usize;

            for peer in &config.sync.peers {
                let peer_path = resolve_peer_path(config, peer);
                if !peer_path.exists() {
                    ui::warn(format!("Peer DB not found: {}", peer_path.display()));
                    continue;
                }
                print!("  Syncing with peer \"{}\"… ", peer);
                ui::flush();
                let peer_conn = match open_peer_db(&peer_path, &sync_tables) {
                    Ok(c) => c,
                    Err(e) => {
                        ui::error(format!("error opening peer: {e}"));
                        continue;
                    }
                };
                match mp_core::sync::local_sync_bidirectional(
                    &conn,
                    &peer_conn,
                    &ag.name,
                    peer,
                    &sync_tables,
                ) {
                    Ok(r) => {
                        println!("sent {}B, received {}B", r.sent, r.received);
                        total_sent += r.sent;
                        total_received += r.received;
                    }
                    Err(e) => ui::error(format!("sync error: {e}")),
                }
            }

            if let Some(ref url) = config.sync.cloud_url {
                print!("  Cloud sync… ");
                ui::flush();
                match mp_core::sync::cloud_sync(&conn, url) {
                    Ok(r) => {
                        println!("{} batch(es)", r.sent);
                        total_sent += r.sent;
                    }
                    Err(e) => ui::error(format!("cloud sync error: {e}")),
                }
            }

            if config.sync.peers.is_empty() && config.sync.cloud_url.is_none() {
                ui::info("No peers or cloud URL configured.");
                ui::info("Add [sync] peers = [\"other-agent\"] or cloud_url = \"…\" to moneypenny.toml");
            } else {
                ui::blank();
                ui::success(format!(
                    "Sync complete. Sent {}B, received {}B.",
                    total_sent, total_received
                ));
            }
        }

        cli::SyncCommand::Push { to, agent } => {
            let ag = resolve_agent(config, agent.as_deref())?;
            let conn = open_agent_db(config, &ag.name)?;
            let peer_path = resolve_peer_path(config, &to);
            if !peer_path.exists() {
                anyhow::bail!("target DB not found: {}", peer_path.display());
            }
            print!("  Pushing \"{}\" → \"{}\"… ", ag.name, to);
            ui::flush();
            let peer_conn = open_peer_db(&peer_path, &sync_tables)?;
            let r = mp_core::sync::local_sync_push(&conn, &peer_conn, &to, &sync_tables)?;
            println!("sent {}B", r.sent);
        }

        cli::SyncCommand::Pull { from, agent } => {
            let ag = resolve_agent(config, agent.as_deref())?;
            let conn = open_agent_db(config, &ag.name)?;
            let peer_path = resolve_peer_path(config, &from);
            if !peer_path.exists() {
                anyhow::bail!("source DB not found: {}", peer_path.display());
            }
            print!("  Pulling \"{}\" → \"{}\"… ", from, ag.name);
            ui::flush();
            let peer_conn = open_peer_db(&peer_path, &sync_tables)?;
            let r = mp_core::sync::local_sync_pull(&conn, &peer_conn, &ag.name, &sync_tables)?;
            println!("received {}B", r.received);
        }

        cli::SyncCommand::Connect { url, agent: _ } => {
            ui::info(format!("Cloud sync URL set to: {url}"));
            ui::info("Add this to your moneypenny.toml:");
            ui::blank();
            ui::hint("[sync]");
            ui::hint(format!("cloud_url = \"{url}\""));
            ui::blank();
            ui::info("Then run `mp sync now` to trigger an initial sync.");
        }
    }
    Ok(())
}

fn resolve_peer_path(config: &Config, peer: &str) -> std::path::PathBuf {
    let p = std::path::Path::new(peer);
    if p.is_absolute() || peer.ends_with(".db") {
        p.to_path_buf()
    } else {
        config.agent_db_path(peer)
    }
}

fn open_peer_db(db_path: &std::path::Path, tables: &[&str]) -> Result<rusqlite::Connection> {
    let conn = rusqlite::Connection::open(db_path)?;
    mp_ext::init_all_extensions(&conn)?;
    mp_core::sync::init_sync_tables(&conn, tables)?;
    Ok(conn)
}
