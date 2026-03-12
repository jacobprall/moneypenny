//! Send command — one-shot message to agent.

use anyhow::Result;

use crate::agent::{self, AgentEvent};
use crate::helpers::{
    build_embedding_provider, build_provider, embed_pending, embedding_model_id,
    extract_facts, maybe_summarize_session, open_agent_db, resolve_or_create_session,
    resolve_agent,
};
use crate::ui;

pub async fn run(
    ctx: &crate::CommandContext<'_>,
    agent_name: &str,
    message: &str,
    session_id: Option<String>,
    verbose: bool,
    quiet: bool,
) -> Result<()> {
    let config = ctx.config;
    let agent = resolve_agent(config, Some(agent_name))?;
    let conn = open_agent_db(config, &agent.name)?;
    let provider = build_provider(&agent)?;
    let embed = build_embedding_provider(config, &agent).ok();
    let (sid, _) = resolve_or_create_session(&conn, &agent.name, Some("cli"), session_id, false)?;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let req_ctx = crate::context::RequestContext {
        agent_id: &agent.name,
        conn: &conn,
        session_id: &sid,
        embed_provider: embed.as_deref(),
        policy_mode: agent.policy_mode(),
        persona: agent.persona.as_deref(),
        worker_bus: None,
    };
    let agent_fut = agent::agent_turn_stream(&req_ctx, provider.as_ref(), message, Some(tx));

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
        anyhow::bail!("{e}");
    }
    agent_result?;

    if ui::styled() && !response_text.is_empty() {
        ui::render_markdown(&response_text);
    } else {
        for line in response_text.lines() {
            ui::info(line);
        }
    }
    ui::blank();

    if let Ok(n) = extract_facts(&conn, provider.as_ref(), &agent.name, &sid).await {
        if n > 0 {
            ui::dim(format!("({n} fact{} learned)", if n == 1 { "" } else { "s" }));
            ui::blank();
        }
    }
    if let Some(ref ep) = embed {
        let model_id = embedding_model_id(agent);
        embed_pending(&conn, ep.as_ref(), &agent.name, &model_id).await;
    }
    maybe_summarize_session(&conn, provider.as_ref(), &sid).await;

    Ok(())
}
