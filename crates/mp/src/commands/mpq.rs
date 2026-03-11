//! MPQ command — run MP DSL expression.

use anyhow::Result;
use mp_core::config::Config;

use crate::helpers::{open_agent_db, resolve_agent};

pub async fn run(
    config: &Config,
    expression: &str,
    agent: Option<String>,
    dry_run: bool,
) -> Result<()> {
    let ag = resolve_agent(config, agent.as_deref())?;
    let conn = open_agent_db(config, &ag.name)?;

    let ctx = mp_core::dsl::ExecuteContext {
        agent_id: ag.name.clone(),
        channel: Some("cli".into()),
        session_id: None,
        trace_id: None,
    };

    let response = mp_core::dsl::run(&conn, expression, dry_run, &ctx);
    let output = serde_json::to_string_pretty(&serde_json::json!({
        "ok": response.ok,
        "code": response.code,
        "message": response.message,
        "data": response.data,
    }))?;
    println!("{output}");

    if !response.ok {
        std::process::exit(1);
    }
    Ok(())
}
