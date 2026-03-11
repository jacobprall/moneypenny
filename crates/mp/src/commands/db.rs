//! Db command — query and schema.

use anyhow::Result;
use mp_core::config::Config;

use crate::cli;
use crate::helpers::{open_agent_db, resolve_agent};
use crate::ui;

pub async fn run(config: &Config, cmd: cli::DbCommand) -> Result<()> {
    let ag = resolve_agent(config, None)?;
    let conn = open_agent_db(config, &ag.name)?;

    match cmd {
        cli::DbCommand::Query { sql, .. } => {
            let mut stmt = conn.prepare(&sql)?;
            let col_count = stmt.column_count();
            let col_names: Vec<String> = (0..col_count)
                .map(|i| stmt.column_name(i).unwrap_or("?").to_string())
                .collect();

            ui::blank();
            let header_cols: Vec<(&str, usize)> = col_names.iter().map(|n| (n.as_str(), n.len())).collect();
            ui::table_header(&header_cols);

            let mut rows = stmt.query([])?;
            while let Some(row) = rows.next()? {
                let vals: Vec<String> = (0..col_count)
                    .map(|i| row.get::<_, String>(i).unwrap_or_else(|_| "NULL".into()))
                    .collect();
                ui::info(vals.join(" | "));
            }
            ui::blank();
        }
        cli::DbCommand::Schema { .. } => {
            let mut stmt = conn
                .prepare("SELECT name, sql FROM sqlite_master WHERE type='table' ORDER BY name")?;
            let tables: Vec<(String, Option<String>)> = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<Result<Vec<_>, _>>()?;

            ui::blank();
            for (name, sql) in &tables {
                ui::dim(format!("-- {name}"));
                if let Some(s) = sql {
                    for line in s.lines() {
                        ui::info(line);
                    }
                }
                ui::blank();
            }
        }
    }
    Ok(())
}
