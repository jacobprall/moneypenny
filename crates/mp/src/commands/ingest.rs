//! Ingest command — ingest files, URLs, or session data.

use anyhow::Result;
use std::path::Path;

use crate::helpers::{
    open_agent_db,
    op_request, resolve_agent,
};
use crate::ui;

/// Parameter object for ingest command — bundles all flags and options.
#[derive(Debug, Clone)]
pub struct IngestArgs {
    pub path: Option<String>,
    pub url: Option<String>,
    pub agent: Option<String>,
    pub openclaw_file: Option<String>,
    pub replay: bool,
    pub status: bool,
    pub replay_run: Option<String>,
    pub replay_latest: bool,
    pub replay_offset: usize,
    pub status_filter: Option<String>,
    pub file_filter: Option<String>,
    pub dry_run: bool,
    pub apply: bool,
    pub source: String,
    pub limit: usize,
    pub cortex: bool,
    pub claude_code: Option<String>,
    pub cursor: Option<String>,
}

pub async fn run(ctx: &crate::context::CommandContext<'_>, args: &IngestArgs) -> Result<()> {
    let config = ctx.config;
    let ag = resolve_agent(config, args.agent.as_deref())?;
    let conn = open_agent_db(config, &ag.name)?;

    if args.status {
        let req = op_request(
            &ag.name,
            "ingest.status",
            serde_json::json!({
                "source": args.source.clone(),
                "status": args.status_filter.clone(),
                "file_path_like": args.file_filter.clone(),
                "limit": args.limit
            }),
        );
        let resp = mp_core::operations::execute(&conn, &req)?;
        let rows = resp.data.as_array().cloned().unwrap_or_default();
        ui::blank();
        if rows.is_empty() {
            ui::info("No ingest runs found.");
        } else {
            ui::table_header(&[("RUN_ID", 36), ("SOURCE", 10), ("STATUS", 22), ("PROC", 8), ("INS", 8), ("DEDUP", 8), ("PROJ", 8), ("ERR", 8)]);
            for r in rows {
                println!(
                    "  {:36} {:10} {:22} {:8} {:8} {:8} {:8} {:8}",
                    r["id"].as_str().unwrap_or("-"),
                    r["source"].as_str().unwrap_or("-"),
                    r["status"].as_str().unwrap_or("-"),
                    r["processed_count"].as_i64().unwrap_or(0),
                    r["inserted_count"].as_i64().unwrap_or(0),
                    r["deduped_count"].as_i64().unwrap_or(0),
                    r["projected_count"].as_i64().unwrap_or(0),
                    r["error_count"].as_i64().unwrap_or(0),
                );
            }
        }
        ui::blank();
    } else if args.replay_run.is_some() || args.replay_latest {
        let selected_run_id = if let Some(run_id) = args.replay_run.clone() {
            run_id
        } else {
            let status_req = op_request(
                &ag.name,
                "ingest.status",
                serde_json::json!({
                    "source": args.source.clone(),
                    "status": args.status_filter.clone(),
                    "file_path_like": args.file_filter.clone(),
                    "limit": args.limit
                }),
            );
            let status_resp = mp_core::operations::execute(&conn, &status_req)?;
            let rows = status_resp.data.as_array().cloned().unwrap_or_default();
            let selected = rows.get(args.replay_offset).cloned().ok_or_else(|| {
                anyhow::anyhow!(
                    "no ingest run available at replay offset {} (after filters)",
                    args.replay_offset
                )
            })?;
            selected["id"]
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| anyhow::anyhow!("selected ingest run has no id"))?
        };

        let effective_dry_run = if args.dry_run { true } else { !args.apply };
        let req = op_request(
            &ag.name,
            "ingest.replay",
            serde_json::json!({
                "run_id": selected_run_id,
                "dry_run": effective_dry_run
            }),
        );
        let resp = mp_core::operations::execute(&conn, &req)?;
        if !resp.ok {
            anyhow::bail!("replay denied: {}", resp.message);
        }
        if effective_dry_run {
            ui::info(format!(
                "Replay preview {}: processed={}, would_insert={}, would_dedupe={}, parse_errors={}, lines={}..{} (use --apply to execute)",
                resp.data["run_id"].as_str().unwrap_or("-"),
                resp.data["processed_count"].as_i64().unwrap_or(0),
                resp.data["would_insert_count"].as_i64().unwrap_or(0),
                resp.data["would_dedupe_count"].as_i64().unwrap_or(0),
                resp.data["parse_error_count"].as_i64().unwrap_or(0),
                resp.data["from_line"].as_i64().unwrap_or(0),
                resp.data["to_line"].as_i64().unwrap_or(0),
            ));
        } else {
            ui::success(format!(
                "Replay run {}: processed={}, inserted={}, deduped={}, projected={}, errors={}",
                resp.data["run_id"].as_str().unwrap_or("-"),
                resp.data["processed_count"].as_i64().unwrap_or(0),
                resp.data["inserted_count"].as_i64().unwrap_or(0),
                resp.data["deduped_count"].as_i64().unwrap_or(0),
                resp.data["projected_count"].as_i64().unwrap_or(0),
                resp.data["error_count"].as_i64().unwrap_or(0),
            ));
        }
    } else if let Some(file) = &args.openclaw_file {
        let req = op_request(
            &ag.name,
            "ingest.events",
            serde_json::json!({
                "source": args.source.clone(),
                "file_path": file.clone(),
                "replay": args.replay,
            }),
        );
        let resp = mp_core::operations::execute(&conn, &req)?;
        if !resp.ok {
            anyhow::bail!("external ingest denied: {}", resp.message);
        }
        ui::success(format!(
            "Ingest run {}: processed={}, inserted={}, deduped={}, projected={}, errors={}",
            resp.data["run_id"].as_str().unwrap_or("-"),
            resp.data["processed_count"].as_i64().unwrap_or(0),
            resp.data["inserted_count"].as_i64().unwrap_or(0),
            resp.data["deduped_count"].as_i64().unwrap_or(0),
            resp.data["projected_count"].as_i64().unwrap_or(0),
            resp.data["error_count"].as_i64().unwrap_or(0),
        ));
    } else if args.cortex {
        let sessions = mp_core::ingest::discover_cortex_sessions();
        if sessions.is_empty() {
            ui::info("No Cortex Code conversations found in ~/.snowflake/cortex/conversations/");
            return Ok(());
        }
        ui::info(format!("Found {} Cortex Code session(s)", sessions.len()));
        ui::blank();
        let mut total_inserted = 0i64;
        let mut total_deduped = 0i64;
        let mut total_errors = 0i64;
        for session_path in &sessions {
            let lines = match mp_core::ingest::convert_cortex_session(session_path) {
                Ok(l) => l,
                Err(e) => {
                    ui::warn(format!(
                        "Skipping {:?}: {}",
                        session_path.file_name().unwrap_or_default(),
                        e
                    ));
                    total_errors += 1;
                    continue;
                }
            };
            if lines.is_empty() {
                continue;
            }
            let tmp = mp_core::ingest::write_temp_jsonl(&lines, "cortex")?;
            let summary =
                mp_core::ingest::ingest_jsonl_file(&conn, "cortex", &tmp, args.replay, &ag.name)?;
            let fname = session_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy();
            ui::info(format!(
                "{}: inserted={}, deduped={}, projected={}, errors={}",
                fname,
                summary.inserted_count,
                summary.deduped_count,
                summary.projected_count,
                summary.error_count,
            ));
            total_inserted += summary.inserted_count;
            total_deduped += summary.deduped_count;
            total_errors += summary.error_count;
            let _ = std::fs::remove_file(&tmp);
        }
        ui::blank();
        ui::dim(format!(
            "Total: {} sessions, {} inserted, {} deduped, {} errors",
            sessions.len(),
            total_inserted,
            total_deduped,
            total_errors
        ));
    } else if args.claude_code.is_some() {
        let slug = args.claude_code.as_deref().filter(|s| !s.is_empty());
        let sessions = mp_core::ingest::discover_claude_code_sessions(slug);
        if sessions.is_empty() {
            if let Some(s) = slug {
                ui::info(format!("No Claude Code sessions found for project slug: {s}"));
            } else {
                ui::info("No Claude Code sessions found in ~/.claude/projects/");
            }
            return Ok(());
        }
        ui::info(format!("Found {} Claude Code session(s)", sessions.len()));
        ui::blank();
        let mut total_inserted = 0i64;
        let mut total_deduped = 0i64;
        let mut total_errors = 0i64;
        for session_path in &sessions {
            let lines = match mp_core::ingest::convert_claude_code_session(session_path) {
                Ok(l) => l,
                Err(e) => {
                    ui::warn(format!(
                        "Skipping {:?}: {}",
                        session_path.file_name().unwrap_or_default(),
                        e
                    ));
                    total_errors += 1;
                    continue;
                }
            };
            if lines.is_empty() {
                continue;
            }
            let tmp = mp_core::ingest::write_temp_jsonl(&lines, "claude-code")?;
            let summary =
                mp_core::ingest::ingest_jsonl_file(&conn, "claude-code", &tmp, args.replay, &ag.name)?;
            let fname = session_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy();
            ui::info(format!(
                "{}: inserted={}, deduped={}, projected={}, errors={}",
                fname,
                summary.inserted_count,
                summary.deduped_count,
                summary.projected_count,
                summary.error_count,
            ));
            total_inserted += summary.inserted_count;
            total_deduped += summary.deduped_count;
            total_errors += summary.error_count;
            let _ = std::fs::remove_file(&tmp);
        }
        ui::blank();
        ui::dim(format!(
            "Total: {} sessions, {} inserted, {} deduped, {} errors",
            sessions.len(),
            total_inserted,
            total_deduped,
            total_errors
        ));
    } else if args.cursor.is_some() {
        let slug = args.cursor.as_deref().filter(|s| !s.is_empty());
        let sessions = mp_core::ingest::discover_cursor_sessions(slug);
        if sessions.is_empty() {
            if let Some(s) = slug {
                ui::info(format!("No Cursor sessions found for project slug: {s}"));
            } else {
                ui::info("No Cursor sessions found in ~/.cursor/projects/");
            }
            return Ok(());
        }
        ui::info(format!("Found {} Cursor session(s)", sessions.len()));
        ui::blank();
        let mut total_inserted = 0i64;
        let mut total_deduped = 0i64;
        let mut total_errors = 0i64;
        for session_path in &sessions {
            let lines = match mp_core::ingest::convert_cursor_session(session_path) {
                Ok(l) => l,
                Err(e) => {
                    ui::warn(format!(
                        "Skipping {:?}: {}",
                        session_path.file_name().unwrap_or_default(),
                        e
                    ));
                    total_errors += 1;
                    continue;
                }
            };
            if lines.is_empty() {
                continue;
            }
            let tmp = mp_core::ingest::write_temp_jsonl(&lines, "cursor")?;
            let summary =
                mp_core::ingest::ingest_jsonl_file(&conn, "cursor", &tmp, args.replay, &ag.name)?;
            let fname = session_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy();
            ui::info(format!(
                "{}: inserted={}, deduped={}, projected={}, errors={}",
                fname,
                summary.inserted_count,
                summary.deduped_count,
                summary.projected_count,
                summary.error_count,
            ));
            total_inserted += summary.inserted_count;
            total_deduped += summary.deduped_count;
            total_errors += summary.error_count;
            let _ = std::fs::remove_file(&tmp);
        }
        ui::blank();
        ui::dim(format!(
            "Total: {} sessions, {} inserted, {} deduped, {} errors",
            sessions.len(),
            total_inserted,
            total_deduped,
            total_errors
        ));
    } else if let Some(p) = &args.path {
        let content = std::fs::read_to_string(&p)?;
        let title = Path::new(&p)
            .file_name()
            .map(|n| n.to_string_lossy().to_string());
        let req = op_request(
            &ag.name,
            "knowledge.ingest",
            serde_json::json!({
                "path": p.clone(),
                "title": title,
                "content": content,
            }),
        );
        let resp = mp_core::operations::execute(&conn, &req)?;
        if !resp.ok {
            anyhow::bail!("ingest denied: {}", resp.message);
        }
        let doc_id = resp.data["document_id"].as_str().unwrap_or("-");
        let chunks = resp.data["chunks_created"].as_u64().unwrap_or(0);
        ui::success(format!("Ingested {p}: {chunks} chunks (doc {doc_id})"));
    } else if let Some(u) = &args.url {
        ui::info(format!("Ingesting URL {u} …"));

        let req = op_request(
            &ag.name,
            "knowledge.ingest",
            serde_json::json!({
                "path": u,
            }),
        );
        let resp = mp_core::operations::execute(&conn, &req)?;
        if !resp.ok {
            anyhow::bail!("ingest denied: {}", resp.message);
        }
        let doc_id = resp.data["document_id"].as_str().unwrap_or("-");
        let chunks = resp.data["chunks_created"].as_u64().unwrap_or(0);
        ui::success(format!("Ingested {u}: {chunks} chunks (doc {doc_id})"));
    } else {
        anyhow::bail!("Provide a path, --openclaw-file, or --url to ingest.");
    }
    Ok(())
}
