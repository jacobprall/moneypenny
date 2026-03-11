//! Experience store — curated learned priors from events.
//!
//! Failure patterns, command outcome priors, budget/performance priors.
//! Fingerprint = SHA-256 of type||tool||error_signature for dedup.
//! Confidence decays with time (default 30-day half-life).

use rusqlite::{Connection, params};
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// Default half-life for confidence decay (days). Used by effective_confidence.
pub const DEFAULT_DECAY_HALF_LIFE_DAYS: f64 = 30.0;

#[derive(Debug, Clone)]
pub struct ExperienceCase {
    pub id: String,
    pub brain_id: String,
    pub case_type: String,
    pub fingerprint: String,
    pub tool: Option<String>,
    pub command: Option<String>,
    pub error_signature: Option<String>,
    pub context: String,
    pub outcome: String,
    pub confidence: f64,
    pub status: String,
    pub hit_count: i64,
    pub last_hit_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone)]
pub struct ExperienceFix {
    pub id: String,
    pub case_id: String,
    pub fix_text: String,
    pub fix_type: String,
    pub applied_count: i64,
    pub success_rate: Option<f64>,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
pub struct RecordInput {
    pub brain_id: String,
    pub case_type: String,
    pub tool: Option<String>,
    pub command: Option<String>,
    pub error_signature: Option<String>,
    pub context: String,
    pub outcome: String,
    pub confidence: Option<f64>,
}

/// Compute fingerprint for dedup: SHA-256 of type||tool||error_signature
pub fn fingerprint(case_type: &str, tool: Option<&str>, error_signature: Option<&str>) -> String {
    let mut hasher = Sha256::new();
    hasher.update(case_type.as_bytes());
    hasher.update(b"||");
    hasher.update(tool.unwrap_or("").as_bytes());
    hasher.update(b"||");
    hasher.update(error_signature.unwrap_or("").as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Record a new experience case or increment hit_count if fingerprint exists.
pub fn record(conn: &Connection, input: &RecordInput) -> anyhow::Result<String> {
    let fp = fingerprint(
        &input.case_type,
        input.tool.as_deref(),
        input.error_signature.as_deref(),
    );
    let now = chrono::Utc::now().timestamp();
    let confidence = input.confidence.unwrap_or(1.0);

    let existing: Option<String> = conn
        .query_row(
            "SELECT id FROM experience_cases WHERE brain_id = ?1 AND fingerprint = ?2 AND status = 'open'",
            params![input.brain_id, fp],
            |r| r.get(0),
        )
        .ok();

    if let Some(id) = existing {
        conn.execute(
            "UPDATE experience_cases SET hit_count = hit_count + 1, last_hit_at = ?1, updated_at = ?1 WHERE id = ?2",
            params![now, id],
        )?;
        Ok(id)
    } else {
        let id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO experience_cases (id, brain_id, type, fingerprint, tool, command, error_signature, context, outcome, confidence, status, hit_count, last_hit_at, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'open', 1, ?11, ?11, ?11)",
            params![
                id,
                input.brain_id,
                input.case_type,
                fp,
                input.tool,
                input.command,
                input.error_signature,
                input.context,
                input.outcome,
                confidence,
                now,
            ],
        )?;
        Ok(id)
    }
}

/// Hot-path pre-action lookup: find matching experience priors.
pub fn r#match(
    conn: &Connection,
    brain_id: &str,
    case_type: Option<&str>,
    tool: Option<&str>,
    command: Option<&str>,
    error: Option<&str>,
    limit: Option<usize>,
) -> anyhow::Result<Vec<ExperienceCase>> {
    let limit = limit.unwrap_or(10).min(50) as i64;

    let (sql, params): (String, Vec<Box<dyn rusqlite::ToSql + '_>>) = {
        let mut sql = String::from(
            "SELECT id, brain_id, type, fingerprint, tool, command, error_signature, context, outcome, confidence, status, hit_count, last_hit_at, created_at, updated_at
             FROM experience_cases
             WHERE brain_id = ?1 AND status = 'open'",
        );
        let mut p: Vec<Box<dyn rusqlite::ToSql + '_>> = vec![Box::new(brain_id)];

        if let Some(t) = case_type {
            sql.push_str(" AND type = ?");
            p.push(Box::new(t));
        }
        if let Some(t) = tool {
            sql.push_str(" AND (tool IS NULL OR tool = ?)");
            p.push(Box::new(t));
        }
        if let Some(c) = command {
            sql.push_str(" AND (command IS NULL OR command LIKE ?)");
            p.push(Box::new(format!("%{c}%")));
        }
        if let Some(e) = error {
            sql.push_str(" AND (error_signature IS NULL OR error_signature LIKE ?)");
            p.push(Box::new(format!("%{e}%")));
        }
        sql.push_str(" ORDER BY hit_count DESC, last_hit_at DESC LIMIT ?");
        p.push(Box::new(limit));
        (sql, p)
    };

    let mut stmt = conn.prepare(&sql)?;
    let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|b| b.as_ref()).collect();
    let cases = stmt
        .query_map(rusqlite::params_from_iter(param_refs), |r| {
            Ok(ExperienceCase {
                id: r.get(0)?,
                brain_id: r.get(1)?,
                case_type: r.get(2)?,
                fingerprint: r.get(3)?,
                tool: r.get(4)?,
                command: r.get(5)?,
                error_signature: r.get(6)?,
                context: r.get(7)?,
                outcome: r.get(8)?,
                confidence: r.get(9)?,
                status: r.get(10)?,
                hit_count: r.get(11)?,
                last_hit_at: r.get(12)?,
                created_at: r.get(13)?,
                updated_at: r.get(14)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(cases)
}

/// Mark as resolved and record the fix.
pub fn resolve(
    conn: &Connection,
    case_id: &str,
    fix_text: &str,
    fix_type: &str,
) -> anyhow::Result<()> {
    let now = chrono::Utc::now().timestamp();
    let fix_id = Uuid::new_v4().to_string();

    conn.execute(
        "UPDATE experience_cases SET status = 'resolved', updated_at = ?1 WHERE id = ?2",
        params![now, case_id],
    )?;

    conn.execute(
        "INSERT INTO experience_fixes (id, case_id, fix_text, fix_type, applied_count, success_rate, created_at)
         VALUES (?1, ?2, ?3, ?4, 0, 1.0, ?5)",
        params![fix_id, case_id, fix_text, fix_type, now],
    )?;

    Ok(())
}

/// Suppress noisy prior.
pub fn ignore(conn: &Connection, case_id: &str, reason: &str) -> anyhow::Result<()> {
    let now = chrono::Utc::now().timestamp();
    conn.execute(
        "UPDATE experience_cases SET status = 'ignored', context = context || ' [ignored: ' || ?1 || ']', updated_at = ?2 WHERE id = ?3",
        params![reason, now, case_id],
    )?;
    Ok(())
}

/// Free-text search across context and outcome. FTS returns rowids; we filter by brain_id.
pub fn search(
    conn: &Connection,
    brain_id: &str,
    query: &str,
    limit: Option<usize>,
) -> anyhow::Result<Vec<ExperienceCase>> {
    let limit = limit.unwrap_or(20).min(50);

    let rowids: Vec<i64> = conn
        .prepare(
            "SELECT rowid FROM experience_cases_fts WHERE experience_cases_fts MATCH ?1 LIMIT ?2",
        )?
        .query_map(params![query, limit as i64], |r| r.get(0))?
        .collect::<Result<Vec<_>, _>>()?;

    if rowids.is_empty() {
        return Ok(Vec::new());
    }

    let placeholders = rowids
        .iter()
        .enumerate()
        .map(|(i, _)| format!("?{}", i + 1))
        .collect::<Vec<_>>()
        .join(", ");

    let sql = format!(
        "SELECT id, brain_id, type, fingerprint, tool, command, error_signature, context, outcome, confidence, status, hit_count, last_hit_at, created_at, updated_at
         FROM experience_cases
         WHERE rowid IN ({placeholders}) AND brain_id = ?{}",
        rowids.len() + 1
    );

    let mut stmt = conn.prepare(&sql)?;
    let mut query_params: Vec<Box<dyn rusqlite::ToSql>> = rowids
        .iter()
        .map(|r| Box::new(*r) as Box<dyn rusqlite::ToSql>)
        .collect();
    query_params.push(Box::new(brain_id.to_string()));

    let param_refs: Vec<&dyn rusqlite::ToSql> = query_params.iter().map(|b| b.as_ref()).collect();
    let cases = stmt
        .query_map(rusqlite::params_from_iter(param_refs), |r| {
            Ok(ExperienceCase {
                id: r.get(0)?,
                brain_id: r.get(1)?,
                case_type: r.get(2)?,
                fingerprint: r.get(3)?,
                tool: r.get(4)?,
                command: r.get(5)?,
                error_signature: r.get(6)?,
                context: r.get(7)?,
                outcome: r.get(8)?,
                confidence: r.get(9)?,
                status: r.get(10)?,
                hit_count: r.get(11)?,
                last_hit_at: r.get(12)?,
                created_at: r.get(13)?,
                updated_at: r.get(14)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(cases)
}

/// Aggregate stats: count by type, status, over window.
pub fn stats(
    conn: &Connection,
    brain_id: &str,
    window_days: Option<i64>,
    case_type: Option<&str>,
) -> anyhow::Result<serde_json::Value> {
    let now = chrono::Utc::now().timestamp();
    let cutoff = window_days.map(|d| now - d * 24 * 3600);

    let mut sql = String::from(
        "SELECT type, status, COUNT(*), COALESCE(SUM(hit_count), 0) FROM experience_cases WHERE brain_id = ?1",
    );
    let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(brain_id.to_string())];

    if let Some(c) = cutoff {
        sql.push_str(" AND created_at >= ?");
        params_vec.push(Box::new(c));
    }
    if let Some(t) = case_type {
        sql.push_str(" AND type = ?");
        params_vec.push(Box::new(t.to_string()));
    }
    sql.push_str(" GROUP BY type, status");

    let mut stmt = conn.prepare(&sql)?;
    let param_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|b| b.as_ref()).collect();
    let rows: Vec<(String, String, i64, i64)> = stmt
        .query_map(rusqlite::params_from_iter(param_refs), |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let by_type_status: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|(t, s, count, hits)| {
            serde_json::json!({
                "type": t,
                "status": s,
                "count": count,
                "total_hits": hits,
            })
        })
        .collect();

    Ok(serde_json::json!({
        "by_type_status": by_type_status,
        "window_days": window_days,
    }))
}

/// Decay/merge: delete low-confidence or old cases, optionally merge duplicates.
pub fn compact(
    conn: &Connection,
    brain_id: &str,
    min_confidence: Option<f64>,
    older_than_days: Option<i64>,
) -> anyhow::Result<Vec<String>> {
    let now = chrono::Utc::now().timestamp();
    let mut deleted = Vec::new();

    if let Some(days) = older_than_days {
        let cutoff = now - days * 24 * 3600;
        let ids: Vec<String> = conn
            .prepare(
                "SELECT id FROM experience_cases WHERE brain_id = ?1 AND (last_hit_at IS NULL OR last_hit_at < ?2) AND status = 'open'",
            )?
            .query_map(params![brain_id, cutoff], |r| r.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
        for id in &ids {
            conn.execute("DELETE FROM experience_cases WHERE id = ?1", params![id])?;
            deleted.push(id.clone());
        }
    }

    if let Some(min_conf) = min_confidence {
        let ids: Vec<String> = conn
            .prepare(
                "SELECT id FROM experience_cases WHERE brain_id = ?1 AND confidence < ?2 AND status = 'open'",
            )?
            .query_map(params![brain_id, min_conf], |r| r.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
        for id in &ids {
            if !deleted.contains(id) {
                conn.execute("DELETE FROM experience_cases WHERE id = ?1", params![id])?;
                deleted.push(id.clone());
            }
        }
    }

    Ok(deleted)
}

/// Effective confidence with exponential decay: confidence * 0.5^(age_days / half_life)
pub fn effective_confidence(
    base_confidence: f64,
    created_at: i64,
    half_life_days: f64,
) -> f64 {
    let now = chrono::Utc::now().timestamp();
    let age_secs = (now - created_at).max(0) as f64;
    let age_days = age_secs / (24.0 * 3600.0);
    let decay = 0.5_f64.powf(age_days / half_life_days);
    base_confidence * decay
}
