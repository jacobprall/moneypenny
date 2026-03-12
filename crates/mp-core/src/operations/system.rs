//! System operations for dashboard: db.stats, sync.status.
//! config.get is handled in the HTTP layer (op_dispatch) since it needs Config.

use rusqlite::Connection;
use std::path::Path;

use super::{fail_response, OperationRequest, OperationResponse};

pub(super) fn op_db_stats(
    conn: &Connection,
    req: &OperationRequest,
) -> anyhow::Result<OperationResponse> {
    let data_dir = req.args["data_dir"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("db.stats requires data_dir in args (injected by server)"))?;
    let agent_id = &req.actor.agent_id;
    let db_path = Path::new(data_dir).join(format!("{agent_id}.db"));

    let file_size_bytes: i64 = std::fs::metadata(&db_path)
        .map(|m| m.len() as i64)
        .unwrap_or(0);

    let schema_version: i64 = conn
        .query_row(
            "SELECT version FROM schema_version ORDER BY version DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let mut tables: Vec<serde_json::Value> = Vec::new();
    let mut stmt = conn.prepare(
        "SELECT name FROM sqlite_master
         WHERE type = 'table' AND name NOT LIKE 'sqlite_%'
         ORDER BY name",
    )?;
    let table_names: Vec<String> = stmt
        .query_map([], |r| r.get(0))?
        .collect::<Result<Vec<_>, _>>()?;

    for name in &table_names {
        // Table names from sqlite_master are trusted; quote for safety.
        let quoted = name.replace('"', "\"\"");
        let row_count: i64 = conn
            .query_row(&format!("SELECT COUNT(*) FROM \"{quoted}\""), [], |r| r.get(0))
            .unwrap_or(0);
        tables.push(serde_json::json!({
            "name": name,
            "row_count": row_count,
        }));
    }

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: "db stats".into(),
        data: serde_json::json!({
            "file_size_bytes": file_size_bytes,
            "schema_version": schema_version,
            "tables": tables,
        }),
        policy: None,
        audit: super::AuditMeta { recorded: false },
    })
}

pub(super) fn op_sync_status(
    conn: &Connection,
    req: &OperationRequest,
) -> anyhow::Result<OperationResponse> {
    let tables: Vec<String> = req.args["tables"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let tables_ref: Vec<&str> = tables.iter().map(String::as_str).collect();

    match crate::sync::status(conn, &tables_ref) {
        Ok(status) => Ok(OperationResponse {
            ok: true,
            code: "ok".into(),
            message: "sync status".into(),
            data: serde_json::json!({
                "site_id": status.site_id,
                "db_version": status.db_version,
                "tables": status
                    .tables
                    .iter()
                    .map(|t| serde_json::json!({
                        "name": t.table,
                        "enabled": t.enabled,
                    }))
                    .collect::<Vec<_>>(),
            }),
            policy: None,
            audit: super::AuditMeta { recorded: false },
        }),
        Err(e) => Ok(fail_response(
            "sync_status_error",
            format!("failed to get sync status: {e}"),
        )),
    }
}
