//! Brain lifecycle operations.

use rusqlite::Connection;
use serde_json::json;

use super::{fail_response, AuditMeta, OperationRequest, OperationResponse};

fn brain_id_from_req(req: &OperationRequest) -> anyhow::Result<String> {
    let from_args = req.args["brain_id"].as_str().map(String::from);
    let from_ctx = req.context.brain_id.clone();
    let from_actor = (!req.actor.agent_id.is_empty()).then(|| req.actor.agent_id.clone());

    from_args
        .or(from_ctx)
        .or(from_actor)
        .ok_or_else(|| anyhow::anyhow!("missing brain_id and no default from context/actor"))
}

pub(super) fn op_brain_create(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let name = req.args["name"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'name'"))?
        .to_string();
    let mission = req.args["mission"].as_str().map(String::from);
    let config = req.args["config"].as_str().map(String::from);
    let id = req.args["brain_id"].as_str().map(String::from);

    let brain = crate::store::brain::NewBrain {
        id,
        name,
        mission,
        config,
    };

    let brain_id = crate::store::brain::create(conn, &brain)?;

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: String::new(),
        data: json!({
            "brain_id": brain_id,
            "name": brain.name,
        }),
        policy: None,
        audit: AuditMeta { recorded: false },
    })
}

pub(super) fn op_brain_get(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let brain_id = brain_id_from_req(req)?;

    match crate::store::brain::get(conn, &brain_id)? {
        Some(b) => Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: String::new(),
        data: json!({
            "brain_id": b.id,
            "name": b.name,
            "mission": b.mission,
            "config": b.config,
            "created_at": b.created_at,
            "updated_at": b.updated_at,
        }),
        policy: None,
        audit: AuditMeta { recorded: false },
    }),
        None => Ok(fail_response(
            "not_found",
            format!("brain '{brain_id}' not found"),
        )),
    }
}

pub(super) fn op_brain_list(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let limit = req.args["limit"].as_u64().and_then(|n| n.try_into().ok());

    let brains = crate::store::brain::list(conn, limit)?;
    let items: Vec<serde_json::Value> = brains
        .into_iter()
        .map(|b| {
            json!({
                "brain_id": b.id,
                "name": b.name,
                "mission": b.mission,
                "config": b.config,
                "created_at": b.created_at,
                "updated_at": b.updated_at,
            })
        })
        .collect();

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: String::new(),
        data: json!({ "brains": items }),
        policy: None,
        audit: AuditMeta { recorded: false },
    })
}

pub(super) fn op_brain_update(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let brain_id = brain_id_from_req(req)?;
    let name = req.args["name"].as_str();
    let mission = req.args["mission"].as_str();
    let config = req.args["config"].as_str();

    crate::store::brain::update(conn, &brain_id, name, mission, config)?;

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: String::new(),
        data: json!({ "brain_id": brain_id }),
        policy: None,
        audit: AuditMeta { recorded: false },
    })
}

pub(super) fn op_brain_delete(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let brain_id = brain_id_from_req(req)?;
    let confirm = req.args["confirm"]
        .as_bool()
        .unwrap_or(false);

    if !confirm {
        return Ok(fail_response(
            "confirmation_required",
            "brain.delete requires confirm: true".to_string(),
        ));
    }

    crate::store::brain::delete(conn, &brain_id)?;

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: String::new(),
        data: json!({ "brain_id": brain_id }),
        policy: None,
        audit: AuditMeta { recorded: false },
    })
}

pub(super) fn op_brain_checkpoint(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let brain_id = brain_id_from_req(req)?;
    let name = req.args["name"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'name'"))?;
    let output_path = req.args["output_path"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'output_path'"))?;

    let path = std::path::Path::new(output_path);
    let path_abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    if let Some(parent) = path_abs.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let path_str = path_abs.to_string_lossy().to_string();

    conn.execute(&format!("VACUUM INTO '{}'", path_str.replace('\'', "''")), [])?;

    let checkpoint_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp();
    let include_domains = req.args["include"].as_str().unwrap_or("all");
    conn.execute(
        "INSERT INTO checkpoints (id, brain_id, name, path, include_domains, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![checkpoint_id, brain_id, name, path_str, include_domains, now],
    )?;

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: format!("checkpoint '{name}' written to {}", path_str),
        data: json!({
            "checkpoint_id": checkpoint_id,
            "brain_id": brain_id,
            "name": name,
            "path": path_str,
        }),
        policy: None,
        audit: AuditMeta { recorded: false },
    })
}

pub(super) fn op_brain_restore(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let checkpoint_path: String = req.args["checkpoint_path"]
        .as_str()
        .map(String::from)
        .or_else(|| {
            req.args["checkpoint_id"]
                .as_str()
                .and_then(|id| {
                    conn.query_row(
                        "SELECT path FROM checkpoints WHERE id = ?1",
                        [id],
                        |r| r.get::<_, String>(0),
                    )
                    .ok()
                })
        })
        .ok_or_else(|| anyhow::anyhow!("missing 'checkpoint_path' or valid 'checkpoint_id'"))?;
    let agent_db_path = req.args["agent_db_path"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'agent_db_path'"))?;
    let mode = req.args["mode"].as_str().unwrap_or("replace");

    if mode != "replace" {
        return Ok(fail_response(
            "unsupported",
            "brain.restore only supports mode 'replace' in V1".to_string(),
        ));
    }

    let src = std::path::Path::new(&checkpoint_path);
    let dst = std::path::Path::new(agent_db_path);
    if !src.exists() {
        return Ok(fail_response(
            "not_found",
            format!("checkpoint file not found: {checkpoint_path}"),
        ));
    }
    std::fs::copy(src, dst)?;

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: format!("restored from {} to {}", checkpoint_path, agent_db_path),
        data: json!({
            "checkpoint_path": checkpoint_path,
            "agent_db_path": agent_db_path,
            "mode": mode,
        }),
        policy: None,
        audit: AuditMeta { recorded: false },
    })
}

pub(super) fn op_brain_export(conn: &Connection, req: &OperationRequest) -> anyhow::Result<OperationResponse> {
    let brain_id = brain_id_from_req(req)?;
    let format = req.args["format"].as_str().unwrap_or("json");
    let output_path = req.args["output_path"].as_str();

    if format != "json" {
        return Ok(fail_response(
            "unsupported",
            "brain.export only supports format 'json' in V1".to_string(),
        ));
    }

    let mut dump = serde_json::Map::new();
    let tables = ["brains", "facts", "documents", "chunks", "skills", "policies", "jobs", "experience_cases"];
    for table in tables {
        let rows: Vec<serde_json::Value> = match conn.prepare(&format!("SELECT * FROM {table}")) {
            Ok(mut stmt) => {
                let col_names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
                stmt.query_map([], |r| {
                    let mut row = serde_json::Map::new();
                    for (i, col) in col_names.iter().enumerate() {
                        let val: rusqlite::types::Value = r.get(i)?;
                        row.insert(
                            col.clone(),
                            match val {
                                rusqlite::types::Value::Integer(v) => json!(v),
                                rusqlite::types::Value::Real(v) => json!(v),
                                rusqlite::types::Value::Text(v) => json!(v),
                                rusqlite::types::Value::Blob(_) => json!("<blob>"),
                                rusqlite::types::Value::Null => json!(null),
                            },
                        );
                    }
                    Ok(serde_json::Value::Object(row))
                })?
                .collect::<Result<Vec<_>, _>>()?
            }
            Err(_) => continue,
        };
        dump.insert(table.to_string(), serde_json::Value::Array(rows));
    }

    let output = serde_json::json!({
        "brain_id": brain_id,
        "exported_at": chrono::Utc::now().to_rfc3339(),
        "tables": dump,
    });

    if let Some(path) = output_path {
        std::fs::write(path, serde_json::to_string_pretty(&output)?)?;
        Ok(OperationResponse {
            ok: true,
            code: "ok".into(),
            message: format!("exported to {}", path),
            data: json!({ "output_path": path }),
            policy: None,
            audit: AuditMeta { recorded: false },
        })
    } else {
        Ok(OperationResponse {
            ok: true,
            code: "ok".into(),
            message: "export complete".into(),
            data: output,
            policy: None,
            audit: AuditMeta { recorded: false },
        })
    }
}
