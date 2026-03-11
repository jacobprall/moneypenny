//! Knowledge command — search and list documents.

use anyhow::Result;
use mp_core::config::Config;

use crate::cli;
use crate::helpers::{open_agent_db, resolve_agent};
use crate::ui;

pub async fn run(config: &Config, cmd: cli::KnowledgeCommand) -> Result<()> {
    let ag = resolve_agent(config, None)?;
    let conn = open_agent_db(config, &ag.name)?;

    match cmd {
        cli::KnowledgeCommand::Search { query } => {
            let results = mp_core::search::fts5_search_knowledge(&conn, &query, 20)?;
            ui::blank();
            if results.is_empty() {
                ui::info(format!("No knowledge results for \"{query}\"."));
            } else {
                for (id, content, _score) in &results {
                    let preview: String = content.chars().take(80).collect();
                    ui::info(format!("{id}: {preview}"));
                }
            }
            ui::blank();
        }
        cli::KnowledgeCommand::List => {
            let docs = mp_core::store::knowledge::list_documents(&conn)?;
            ui::blank();
            if docs.is_empty() {
                ui::info("No documents ingested.");
            } else {
                ui::table_header(&[("ID", 36), ("TITLE", 30), ("PATH", 20)]);
                for d in &docs {
                    println!(
                        "  {:36} {:30} {:20}",
                        d.id,
                        d.title.as_deref().unwrap_or("-"),
                        d.path.as_deref().unwrap_or("-"),
                    );
                }
            }
            ui::blank();
        }
    }
    Ok(())
}
