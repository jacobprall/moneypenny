//! Send command — one-shot message to agent.

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
    agent_name: &str,
    message: &str,
    session_id: Option<String>,
) -> Result<()> {
    let agent = resolve_agent(config, Some(agent_name))?;
    let conn = open_agent_db(config, &agent.name)?;
    let provider = build_provider(agent)?;
    let embed = build_embedding_provider(config, agent).ok();
    let (sid, _) = resolve_or_create_session(&conn, &agent.name, Some("cli"), session_id, false)?;

    let response = agent::agent_turn(
        &conn,
        provider.as_ref(),
        embed.as_deref(),
        &agent.name,
        &sid,
        agent.persona.as_deref(),
        message,
        agent.policy_mode(),
        None,
    )
    .await?;

    ui::blank();
    for line in response.lines() {
        ui::info(line);
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
