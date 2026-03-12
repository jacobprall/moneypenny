//! Chat command — interactive terminal chat.

use anyhow::Result;
use rustyline::error::ReadlineError;
use rustyline::Editor;

use crate::agent::{self, AgentEvent};
use crate::helpers::{
    build_embedding_provider, build_provider, extract_facts, maybe_summarize_session,
    open_agent_db, resolve_or_create_session, resolve_agent,
};
use crate::ui;
use crate::worker::run_embedding_processor;

fn build_prompt(agent_name: &str, session_id: &str) -> String {
    let short_sid = &session_id[..session_id.len().min(6)];
    if ui::styled() {
        use owo_colors::OwoColorize;
        format!(
            "  {} {} ",
            format!("{agent_name} [s:{short_sid}]").dimmed(),
            ">".dimmed()
        )
    } else {
        format!("  {agent_name} [s:{short_sid}] > ")
    }
}

pub async fn run(
    ctx: &crate::context::CommandContext<'_>,
    agent: Option<String>,
    session_id: Option<String>,
    force_new: bool,
    _tui: bool,
    verbose: bool,
    quiet: bool,
) -> Result<()> {
    let config = ctx.config;
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

    let history_dir = config.data_dir.join(&ag.name);
    let _ = std::fs::create_dir_all(&history_dir);
    let history_path = history_dir.join("chat_history");
    let mut rl = Editor::<(), _>::new()?;
    let _ = rl.load_history(&history_path);

    let (shutdown_tx, _) = tokio::sync::broadcast::channel::<()>(1);
    let mut embed_shutdown = shutdown_tx.subscribe();
    let embed_config = config.clone();
    let embedding_handle =
        tokio::spawn(async move { run_embedding_processor(&embed_config, &mut embed_shutdown).await });

    loop {
        let prompt = build_prompt(&ag.name, &sid);
        match rl.readline(&prompt) {
            Ok(line) => {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let _ = rl.add_history_entry(line);

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

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let line_for_agent = line.to_string();

        let req_ctx = crate::context::RequestContext {
            agent_id: &ag.name,
            conn: &conn,
            session_id: &sid,
            embed_provider: embed.as_deref(),
            policy_mode: ag.policy_mode(),
            persona: ag.persona.as_deref(),
            worker_bus: None,
        };
        let agent_fut = agent::agent_turn_stream(&req_ctx, provider.as_ref(), &line_for_agent, Some(tx));

        let recv_fut = async {
            let mut response_text = String::new();
            let mut stream_err = None;
            while let Some(ev) = rx.recv().await {
                match ev {
                    AgentEvent::Token(t) => {
                        print!("{t}");
                        ui::flush();
                    }
                    AgentEvent::ToolStart { name, arguments } => {
                        if !quiet {
                            println!();
                            if verbose {
                                let args_preview: String = arguments.chars().take(80).collect();
                                let args_preview = if arguments.len() > 80 {
                                    format!("{args_preview}...")
                                } else {
                                    args_preview
                                };
                                ui::dim(format!("  ◦ {name}({args_preview})..."));
                            } else {
                                ui::dim(format!("  ◦ {name}..."));
                            }
                        }
                    }
                    AgentEvent::ToolEnd {
                        name,
                        success,
                        duration_ms,
                        result_preview,
                    } => {
                        if !quiet {
                            let icon = if success { "✓" } else { "✗" };
                            if verbose {
                                let preview = result_preview
                                    .as_deref()
                                    .unwrap_or("")
                                    .replace('\n', " ");
                                ui::dim(format!("  {icon} {name} — {preview} ({duration_ms}ms)"));
                            } else {
                                ui::dim(format!("  {icon} {name} ({duration_ms}ms)"));
                            }
                        }
                    }
                    AgentEvent::Done { response, .. } => {
                        response_text = response;
                        break;
                    }
                    AgentEvent::Error(e) => {
                        stream_err = Some(e);
                        break;
                    }
                }
            }
            (response_text, stream_err)
        };

        ui::blank();
        let (agent_result, (response_text, stream_err)) = tokio::join!(agent_fut, recv_fut);
        println!();
        ui::blank();

        if let Some(e) = stream_err {
            ui::error(e);
            ui::blank();
        } else if let Err(e) = agent_result {
            ui::error(e);
            ui::blank();
        } else {
                if ui::styled() && !response_text.is_empty() {
                    ui::render_markdown(&response_text);
                } else {
                    for resp_line in response_text.lines() {
                        ui::info(resp_line);
                    }
                }
                ui::blank();

                let extract_spinner = ui::spinner("Extracting facts...");
                match extract_facts(&conn, provider.as_ref(), &ag.name, &sid).await {
                    Ok(n) if n > 0 => {
                        extract_spinner.finish_and_clear();
                        ui::dim(format!("({n} fact{} learned)", if n == 1 { "" } else { "s" }));
                        ui::blank();
                    }
                    Err(e) => {
                        extract_spinner.finish_and_clear();
                        tracing::debug!("extraction error: {e}");
                    }
                    _ => {
                        extract_spinner.finish_and_clear();
                    }
                }
                maybe_summarize_session(&conn, provider.as_ref(), &sid).await;
            }
            }
            Err(ReadlineError::Interrupted | ReadlineError::Eof) => break,
            Err(e) => {
                ui::error(e);
                break;
            }
        }
    }

    let _ = rl.save_history(&history_path);
    let _ = shutdown_tx.send(());
    embedding_handle.abort();
    ui::info(format!("Session {sid} ended."));
    Ok(())
}
