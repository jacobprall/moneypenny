//! Chat command — interactive terminal chat.

use anyhow::Result;
use mp_core::config::Config;

use crate::agent;
use crate::helpers::{
    build_embedding_provider, build_provider, embed_pending, embedding_model_id,
    extract_facts, maybe_summarize_session, open_agent_db, resolve_or_create_session,
    resolve_agent,
};
use crate::ui;

pub async fn run(
    config: &Config,
    agent: Option<String>,
    session_id: Option<String>,
    force_new: bool,
) -> Result<()> {
    let ag = resolve_agent(config, agent.as_deref())?;
    let conn = open_agent_db(config, &ag.name)?;
    let provider = build_provider(ag)?;
    let embed = build_embedding_provider(config, ag).ok();
    let (mut sid, resumed) =
        resolve_or_create_session(&conn, &ag.name, Some("cli"), session_id, force_new)?;

    ui::blank();
    if ui::styled() {
        use owo_colors::OwoColorize;
        println!(
            "  {} v{} — agent: {}",
            "Moneypenny".bold(),
            env!("CARGO_PKG_VERSION"),
            ag.name
        );
    } else {
        println!(
            "  Moneypenny v{} — agent: {}",
            env!("CARGO_PKG_VERSION"),
            ag.name
        );
    }
    ui::field("LLM", 11, format!(
        "{} ({})",
        ag.llm.provider,
        ag.llm.model.as_deref().unwrap_or("default")
    ));
    ui::field("Embedding", 11, format!(
        "{} ({}, {}D)",
        ag.embedding.provider, ag.embedding.model, ag.embedding.dimensions
    ));
    if resumed {
        let msg_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM messages WHERE session_id = ?1",
                [&sid],
                |r| r.get(0),
            )
            .unwrap_or(0);
        ui::dim(format!(
            "Resumed session ({msg_count} messages). Use /new for a fresh session."
        ));
    }
    ui::info("Type /help for commands, Ctrl-C to exit.");
    ui::blank();

    let stdin = std::io::stdin();
    loop {
        ui::prompt();

        let mut line = String::new();
        if stdin.read_line(&mut line)? == 0 {
            break;
        }
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        match line {
            "/quit" | "/exit" => break,
            "/help" => {
                ui::info("/facts    — list stored facts");
                ui::info("/scratch  — list scratch entries");
                ui::info("/session  — show session info");
                ui::info("/new      — start a fresh session");
                ui::info("/quit     — exit chat");
                ui::blank();
                continue;
            }
            "/facts" => {
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
            "/scratch" => {
                let entries = mp_core::store::scratch::list(&conn, &sid)?;
                if entries.is_empty() {
                    ui::info("Scratch is empty.");
                } else {
                    for e in &entries {
                        let preview: String = e.content.chars().take(60).collect();
                        ui::info(format!("[{}] {}", e.key, preview));
                    }
                }
                ui::blank();
                continue;
            }
            "/session" => {
                let msgs = mp_core::store::log::get_messages(&conn, &sid)?;
                ui::info(format!("Session: {sid}"));
                ui::info(format!("Messages: {}", msgs.len()));
                ui::blank();
                continue;
            }
            "/new" => {
                sid = mp_core::store::log::create_session(&conn, &ag.name, Some("cli"))?;
                ui::success("Started fresh session.");
                ui::blank();
                continue;
            }
            _ => {}
        }

        match agent::agent_turn(
            &conn,
            provider.as_ref(),
            embed.as_deref(),
            &ag.name,
            &sid,
            ag.persona.as_deref(),
            line,
            ag.policy_mode(),
            None,
        )
        .await
        {
            Ok(response) => {
                ui::blank();
                for resp_line in response.lines() {
                    ui::info(resp_line);
                }
                ui::blank();

                match extract_facts(&conn, provider.as_ref(), &ag.name, &sid).await {
                    Ok(n) if n > 0 => {
                        ui::dim(format!("({n} fact{} learned)", if n == 1 { "" } else { "s" }));
                        ui::blank();
                    }
                    Err(e) => tracing::debug!("extraction error: {e}"),
                    _ => {}
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

    ui::info(format!("Session {sid} ended."));
    Ok(())
}
