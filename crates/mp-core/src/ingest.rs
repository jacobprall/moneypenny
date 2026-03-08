use rusqlite::{Connection, params};
use serde_json::Value;
use std::hash::{Hash, Hasher};
use std::io::BufRead;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct IngestSummary {
    pub run_id: String,
    pub source: String,
    pub file_path: String,
    pub from_line: i64,
    pub to_line: i64,
    pub processed_count: i64,
    pub inserted_count: i64,
    pub deduped_count: i64,
    pub projected_count: i64,
    pub error_count: i64,
}

#[derive(Debug, Clone)]
pub struct IngestRunRecord {
    pub id: String,
    pub source: String,
    pub file_path: String,
    pub from_line: i64,
    pub to_line: i64,
    pub processed_count: i64,
    pub inserted_count: i64,
    pub deduped_count: i64,
    pub projected_count: i64,
    pub error_count: i64,
    pub status: String,
    pub started_at: i64,
    pub finished_at: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct IngestPreflight {
    pub source: String,
    pub file_path: String,
    pub from_line: i64,
    pub to_line: i64,
    pub processed_count: i64,
    pub would_insert_count: i64,
    pub would_dedupe_count: i64,
    pub parse_error_count: i64,
}

pub fn ingest_jsonl_file(
    conn: &Connection,
    source: &str,
    file_path: &Path,
    replay: bool,
    agent_id: &str,
) -> anyhow::Result<IngestSummary> {
    let file_path_str = file_path.to_string_lossy().to_string();
    let from_line = if replay {
        1
    } else {
        conn.query_row(
            "SELECT COALESCE(MAX(to_line), 0) + 1
             FROM ingest_runs
             WHERE source = ?1 AND file_path = ?2 AND status = 'completed'",
            params![source, file_path_str],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(1)
    };

    let run_id = uuid::Uuid::new_v4().to_string();
    let started_at = chrono::Utc::now().timestamp();
    conn.execute(
        "INSERT INTO ingest_runs (id, source, file_path, from_line, status, started_at)
         VALUES (?1, ?2, ?3, ?4, 'running', ?5)",
        params![run_id, source, file_path_str, from_line, started_at],
    )?;

    let mut processed_count = 0i64;
    let mut inserted_count = 0i64;
    let mut deduped_count = 0i64;
    let mut projected_count = 0i64;
    let mut error_count = 0i64;
    let mut to_line = from_line.saturating_sub(1);
    let mut last_error: Option<String> = None;

    let file = std::fs::File::open(file_path)?;
    let reader = std::io::BufReader::new(file);
    for (idx, line_res) in reader.lines().enumerate() {
        let line_no = (idx as i64) + 1;
        if line_no < from_line {
            continue;
        }
        to_line = line_no;
        processed_count += 1;

        let line = match line_res {
            Ok(v) => v,
            Err(e) => {
                error_count += 1;
                last_error = Some(format!("line {line_no}: {e}"));
                continue;
            }
        };
        if line.trim().is_empty() {
            continue;
        }

        let parsed: Value = serde_json::from_str(&line).unwrap_or_else(|_| {
            serde_json::json!({ "raw_line": line })
        });

        let event_type = pick_str(&parsed, &["type", "event", "event_type", "name"])
            .unwrap_or("unknown")
            .to_string();
        let source_event_id = pick_str(&parsed, &["event_id", "eventId", "id", "uuid"])
            .map(str::to_string);
        let session_id = pick_str(
            &parsed,
            &["session_id", "sessionId", "session", "conversation_id", "conversationId"],
        )
        .map(str::to_string);
        let event_ts = pick_ts(&parsed).unwrap_or_else(|| chrono::Utc::now().timestamp());
        let payload_json = serde_json::to_string(&parsed).unwrap_or_else(|_| "{}".to_string());
        let content_hash = stable_hash_hex(&line);
        let event_key = source_event_id.clone().unwrap_or_else(|| content_hash.clone());
        let event_id = format!("ext:{source}:{event_key}");
        let normalized = normalized_projection_fields(
            &parsed,
            source,
            event_key.as_str(),
            session_id.as_deref(),
        );

        conn.execute(
            "INSERT OR IGNORE INTO external_events
             (id, source, source_event_id, event_type, event_ts, session_id, payload_json, content_hash, run_id, line_no, raw_line, projected, ingested_at,
              normalized_provider, normalized_model, normalized_input_tokens, normalized_output_tokens, normalized_total_tokens, normalized_cost_usd, normalized_correlation_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 0, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)",
            params![
                event_id,
                source,
                source_event_id,
                event_type,
                event_ts,
                session_id,
                payload_json,
                content_hash,
                run_id,
                line_no,
                line,
                chrono::Utc::now().timestamp(),
                normalized.provider,
                normalized.model,
                normalized.input_tokens,
                normalized.output_tokens,
                normalized.total_tokens,
                normalized.cost_usd,
                normalized.correlation_id
            ],
        )?;

        if conn.changes() == 0 {
            deduped_count += 1;
            continue;
        }
        inserted_count += 1;

        match project_event(conn, source, &event_key, &parsed, &event_type, session_id.as_deref(), event_ts, agent_id) {
            Ok(done) => {
                if done {
                    projected_count += 1;
                }
                conn.execute(
                    "UPDATE external_events SET projected = ?2 WHERE id = ?1",
                    params![event_id, if done { 1 } else { 0 }],
                )?;
            }
            Err(e) => {
                error_count += 1;
                let msg = e.to_string();
                last_error = Some(msg.clone());
                conn.execute(
                    "UPDATE external_events SET projected = 0, projection_error = ?2 WHERE id = ?1",
                    params![event_id, msg],
                )?;
            }
        }
    }

    let finished_at = chrono::Utc::now().timestamp();
    let status = if error_count > 0 { "completed_with_errors" } else { "completed" };
    conn.execute(
        "UPDATE ingest_runs
         SET to_line = ?2, processed_count = ?3, inserted_count = ?4, deduped_count = ?5,
             projected_count = ?6, error_count = ?7, last_error = ?8, status = ?9, finished_at = ?10
         WHERE id = ?1",
        params![
            run_id,
            to_line,
            processed_count,
            inserted_count,
            deduped_count,
            projected_count,
            error_count,
            last_error,
            status,
            finished_at
        ],
    )?;

    Ok(IngestSummary {
        run_id,
        source: source.to_string(),
        file_path: file_path_str,
        from_line,
        to_line,
        processed_count,
        inserted_count,
        deduped_count,
        projected_count,
        error_count,
    })
}

pub fn replay_run(conn: &Connection, run_id: &str, agent_id: &str) -> anyhow::Result<IngestSummary> {
    let (source, file_path): (String, String) = conn.query_row(
        "SELECT source, file_path FROM ingest_runs WHERE id = ?1",
        [run_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    ingest_jsonl_file(conn, &source, Path::new(&file_path), true, agent_id)
}

pub fn replay_run_preflight(conn: &Connection, run_id: &str) -> anyhow::Result<IngestPreflight> {
    let (source, file_path): (String, String) = conn.query_row(
        "SELECT source, file_path FROM ingest_runs WHERE id = ?1",
        [run_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    preflight_jsonl_file(conn, &source, Path::new(&file_path), true)
}

pub fn recent_runs(
    conn: &Connection,
    source: Option<&str>,
    status: Option<&str>,
    file_path_like: Option<&str>,
    limit: usize,
) -> anyhow::Result<Vec<IngestRunRecord>> {
    let lim = i64::try_from(limit).unwrap_or(20);
    let file_path_pattern = file_path_like.map(|f| format!("%{f}%"));
    let mut stmt = conn.prepare(
        "SELECT id, source, file_path, from_line, to_line, processed_count, inserted_count, deduped_count,
                projected_count, error_count, status, started_at, finished_at
         FROM ingest_runs
         WHERE (?1 IS NULL OR source = ?1)
           AND (?2 IS NULL OR status = ?2)
           AND (?3 IS NULL OR file_path LIKE ?3)
         ORDER BY started_at DESC
         LIMIT ?4",
    )?;
    let rows = stmt
        .query_map(params![source, status, file_path_pattern, lim], |r| {
            Ok(IngestRunRecord {
                id: r.get(0)?,
                source: r.get(1)?,
                file_path: r.get(2)?,
                from_line: r.get(3)?,
                to_line: r.get(4)?,
                processed_count: r.get(5)?,
                inserted_count: r.get(6)?,
                deduped_count: r.get(7)?,
                projected_count: r.get(8)?,
                error_count: r.get(9)?,
                status: r.get(10)?,
                started_at: r.get(11)?,
                finished_at: r.get(12)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn preflight_jsonl_file(
    conn: &Connection,
    source: &str,
    file_path: &Path,
    replay: bool,
) -> anyhow::Result<IngestPreflight> {
    let file_path_str = file_path.to_string_lossy().to_string();
    let from_line = if replay {
        1
    } else {
        conn.query_row(
            "SELECT COALESCE(MAX(to_line), 0) + 1
             FROM ingest_runs
             WHERE source = ?1 AND file_path = ?2 AND status = 'completed'",
            params![source, file_path_str],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(1)
    };

    let file = std::fs::File::open(file_path)?;
    let reader = std::io::BufReader::new(file);
    let mut processed_count = 0i64;
    let mut would_insert_count = 0i64;
    let mut would_dedupe_count = 0i64;
    let mut parse_error_count = 0i64;
    let mut to_line = from_line.saturating_sub(1);

    for (idx, line_res) in reader.lines().enumerate() {
        let line_no = (idx as i64) + 1;
        if line_no < from_line {
            continue;
        }
        to_line = line_no;
        processed_count += 1;

        let line = match line_res {
            Ok(v) => v,
            Err(_) => {
                parse_error_count += 1;
                continue;
            }
        };
        if line.trim().is_empty() {
            continue;
        }

        let parsed: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => {
                parse_error_count += 1;
                continue;
            }
        };
        let source_event_id = pick_str(&parsed, &["event_id", "eventId", "id", "uuid"])
            .map(str::to_string);
        let content_hash = stable_hash_hex(&line);
        let event_key = source_event_id.clone().unwrap_or_else(|| content_hash.clone());
        let event_id = format!("ext:{source}:{event_key}");
        let exists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM external_events WHERE id = ?1",
                [event_id],
                |r| r.get(0),
            )
            .unwrap_or(0);
        if exists > 0 {
            would_dedupe_count += 1;
        } else {
            would_insert_count += 1;
        }
    }

    Ok(IngestPreflight {
        source: source.to_string(),
        file_path: file_path_str,
        from_line,
        to_line,
        processed_count,
        would_insert_count,
        would_dedupe_count,
        parse_error_count,
    })
}

fn project_event(
    conn: &Connection,
    source: &str,
    event_key: &str,
    payload: &Value,
    event_type: &str,
    session_id: Option<&str>,
    ts: i64,
    agent_id: &str,
) -> anyhow::Result<bool> {
    let sid = session_id
        .map(str::to_string)
        .or_else(|| pick_str(payload, &["session_id", "session", "conversation_id"]).map(str::to_string))
        .unwrap_or_else(|| format!("ext:{source}:session:{event_key}"));
    ensure_session(conn, &sid, agent_id, ts)?;

    if event_type.starts_with("session.") {
        return Ok(true);
    }

    if event_type.starts_with("message.") {
        let msg_id = format!("ext:{source}:msg:{event_key}");
        let role = pick_str(payload, &["role"])
            .unwrap_or(if event_type.contains("queued") { "user" } else { "assistant" });
        let content = pick_str(payload, &["content", "text", "message"])
            .map(str::to_string)
            .unwrap_or_else(|| serde_json::to_string(payload).unwrap_or_else(|_| "{}".to_string()));
        conn.execute(
            "INSERT OR IGNORE INTO messages (id, session_id, role, content, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![msg_id, sid, role, content, ts],
        )?;
        if role == "assistant" || role == "system" {
            promote_imported_message_fact(conn, agent_id, &sid, &msg_id, &content);
        }
        return Ok(true);
    }

    if event_type == "model.usage" {
        let provider = pick_str(payload, &["provider"]).unwrap_or("unknown");
        let model = pick_str(payload, &["model"]).unwrap_or("unknown");
        let channel = pick_str(payload, &["channel"]).unwrap_or("external");
        let input_tokens = pick_i64(payload, &["input_tokens", "prompt_tokens"]).unwrap_or(0);
        let output_tokens = pick_i64(payload, &["output_tokens", "completion_tokens"]).unwrap_or(0);
        let total_tokens = pick_i64(payload, &["total_tokens"]).unwrap_or(input_tokens + output_tokens);
        let cost = pick_f64(payload, &["cost_usd", "cost"]).unwrap_or(0.0);
        let duration_ms = pick_i64(payload, &["duration_ms", "latency_ms"]).unwrap_or(0);
        let msg_id = format!("ext:{source}:msg:tool:{event_key}");
        let summary = format!(
            "model.usage provider={provider} model={model} channel={channel} total_tokens={total_tokens} cost_usd={cost}"
        );
        conn.execute(
            "INSERT OR IGNORE INTO messages (id, session_id, role, content, created_at)
             VALUES (?1, ?2, 'system', ?3, ?4)",
            params![msg_id, sid, summary, ts],
        )?;
        let tool_id = format!("ext:{source}:tool:{event_key}");
        conn.execute(
            "INSERT OR IGNORE INTO tool_calls (id, message_id, session_id, tool_name, arguments, result, status, policy_decision, duration_ms, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, NULL, 'external', NULL, ?6, ?7)",
            params![
                tool_id,
                msg_id,
                sid,
                format!("model.usage:{provider}/{model}"),
                serde_json::to_string(payload).unwrap_or_else(|_| "{}".to_string()),
                duration_ms,
                ts
            ],
        )?;
        return Ok(true);
    }

    if event_type.starts_with("run.") {
        let msg_id = format!("ext:{source}:msg:run:{event_key}");
        let status = pick_str(payload, &["status", "result"]).unwrap_or("external");
        let result = pick_str(payload, &["output", "message", "error"])
            .map(str::to_string)
            .unwrap_or_else(|| serde_json::to_string(payload).unwrap_or_else(|_| "{}".to_string()));
        conn.execute(
            "INSERT OR IGNORE INTO messages (id, session_id, role, content, created_at)
             VALUES (?1, ?2, 'system', ?3, ?4)",
            params![msg_id, sid, format!("{event_type} status={status}"), ts],
        )?;
        let tool_id = format!("ext:{source}:tool:{event_key}");
        conn.execute(
            "INSERT OR IGNORE INTO tool_calls (id, message_id, session_id, tool_name, arguments, result, status, policy_decision, duration_ms, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, ?8, ?9)",
            params![
                tool_id,
                msg_id,
                sid,
                event_type,
                serde_json::to_string(payload).unwrap_or_else(|_| "{}".to_string()),
                result,
                status,
                pick_i64(payload, &["duration_ms", "latency_ms"]),
                ts
            ],
        )?;
        return Ok(true);
    }

    if event_type.starts_with("webhook.") {
        let audit_id = format!("ext:{source}:audit:{event_key}");
        let effect = if event_type.ends_with(".error") { "denied" } else { "audited" };
        let actor = pick_str(payload, &["provider", "source"]).unwrap_or(source);
        let reason = pick_str(payload, &["error", "reason", "message"])
            .map(str::to_string)
            .or_else(|| Some(format!("webhook event {event_type}")));
        conn.execute(
            "INSERT OR IGNORE INTO policy_audit
             (id, policy_id, actor, action, resource, effect, reason, correlation_id, session_id, created_at)
             VALUES (?1, NULL, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                audit_id,
                format!("external:{actor}"),
                event_type,
                pick_str(payload, &["url", "endpoint"]).unwrap_or("webhook"),
                effect,
                reason,
                event_key,
                sid,
                ts
            ],
        )?;
        return Ok(true);
    }

    Ok(false)
}

fn ensure_session(conn: &Connection, session_id: &str, agent_id: &str, ts: i64) -> anyhow::Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO sessions (id, agent_id, channel, started_at)
         VALUES (?1, ?2, 'external', ?3)",
        params![session_id, agent_id, ts],
    )?;
    Ok(())
}

fn pick_str<'a>(v: &'a Value, keys: &[&str]) -> Option<&'a str> {
    for key in keys {
        if let Some(s) = v.get(*key).and_then(Value::as_str) {
            return Some(s);
        }
    }
    None
}

fn pick_ts(v: &Value) -> Option<i64> {
    for key in ["timestamp", "ts", "created_at", "time"] {
        if let Some(i) = v.get(key).and_then(Value::as_i64) {
            return Some(i);
        }
        if let Some(s) = v.get(key).and_then(Value::as_str) {
            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
                return Some(dt.timestamp());
            }
            if let Ok(i) = s.parse::<i64>() {
                return Some(i);
            }
        }
    }
    None
}

fn pick_i64(v: &Value, keys: &[&str]) -> Option<i64> {
    for key in keys {
        if let Some(i) = v.get(*key).and_then(Value::as_i64) {
            return Some(i);
        }
        if let Some(s) = v.get(*key).and_then(Value::as_str) {
            if let Ok(i) = s.parse::<i64>() {
                return Some(i);
            }
        }
    }
    None
}

fn pick_f64(v: &Value, keys: &[&str]) -> Option<f64> {
    for key in keys {
        if let Some(f) = v.get(*key).and_then(Value::as_f64) {
            return Some(f);
        }
        if let Some(s) = v.get(*key).and_then(Value::as_str) {
            if let Ok(f) = s.parse::<f64>() {
                return Some(f);
            }
        }
    }
    None
}

fn stable_hash_hex(input: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    input.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn promote_imported_message_fact(
    conn: &Connection,
    agent_id: &str,
    session_id: &str,
    source_message_id: &str,
    content: &str,
) {
    if let Some(candidate) = candidate_from_message(content) {
        let _ = crate::extraction::run_pipeline(
            conn,
            agent_id,
            session_id,
            &[candidate],
            Some(source_message_id),
        );
    }
}

fn candidate_from_message(content: &str) -> Option<crate::extraction::CandidateFact> {
    let normalized = content.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.len() < 40 {
        return None;
    }
    if normalized.starts_with('{') || normalized.starts_with('[') {
        return None;
    }

    let first_sentence = normalized
        .split_terminator(['.', '!', '?'])
        .find(|s| !s.trim().is_empty())
        .unwrap_or(&normalized)
        .trim();
    if first_sentence.split_whitespace().count() < 6 {
        return None;
    }

    let content_value = truncate_chars(first_sentence, 320);
    let summary = truncate_chars(first_sentence, 140);
    let pointer_words = first_sentence
        .split_whitespace()
        .take(6)
        .collect::<Vec<_>>()
        .join(" ");
    let pointer = format!("external: {pointer_words}");
    let keywords = extract_keywords(first_sentence);

    Some(crate::extraction::CandidateFact {
        content: content_value,
        summary,
        pointer,
        keywords,
        confidence: 0.7,
    })
}

fn truncate_chars(input: &str, max_chars: usize) -> String {
    input.chars().take(max_chars).collect::<String>().trim().to_string()
}

fn extract_keywords(input: &str) -> Option<String> {
    let mut keywords = Vec::new();
    for word in input.split_whitespace() {
        let cleaned = word
            .trim_matches(|c: char| !c.is_alphanumeric() && c != '_')
            .to_ascii_lowercase();
        if cleaned.len() < 4 {
            continue;
        }
        if !keywords.iter().any(|k| k == &cleaned) {
            keywords.push(cleaned);
        }
        if keywords.len() >= 8 {
            break;
        }
    }
    if keywords.is_empty() {
        None
    } else {
        Some(keywords.join(" "))
    }
}

#[derive(Debug)]
struct NormalizedProjectionFields {
    provider: Option<String>,
    model: Option<String>,
    input_tokens: Option<i64>,
    output_tokens: Option<i64>,
    total_tokens: Option<i64>,
    cost_usd: Option<f64>,
    correlation_id: Option<String>,
}

fn normalized_projection_fields(
    payload: &Value,
    source: &str,
    event_key: &str,
    session_id: Option<&str>,
) -> NormalizedProjectionFields {
    let provider = pick_str(payload, &["provider", "vendor", "llm_provider"]).map(str::to_string);
    let model = pick_str(payload, &["model", "model_name", "llm_model"]).map(str::to_string);
    let input_tokens = pick_i64(payload, &["input_tokens", "prompt_tokens"]);
    let output_tokens = pick_i64(payload, &["output_tokens", "completion_tokens"]);
    let total_tokens = pick_i64(payload, &["total_tokens"]).or_else(|| {
        if input_tokens.is_some() || output_tokens.is_some() {
            Some(input_tokens.unwrap_or(0) + output_tokens.unwrap_or(0))
        } else {
            None
        }
    });
    let cost_usd = pick_f64(payload, &["cost_usd", "cost"]);
    let correlation_id = pick_str(
        payload,
        &[
            "correlation_id",
            "correlationId",
            "trace_id",
            "traceId",
            "request_id",
            "requestId",
            "run_id",
            "runId",
        ],
    )
    .map(str::to_string)
    .or_else(|| session_id.map(str::to_string))
    .or_else(|| Some(format!("{source}:{event_key}")));

    NormalizedProjectionFields {
        provider,
        model,
        input_tokens,
        output_tokens,
        total_tokens,
        cost_usd,
        correlation_id,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::schema::init_agent_db(&conn).unwrap();
        conn
    }

    fn write_tmp_jsonl(lines: &[&str]) -> tempfile::NamedTempFile {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::new().unwrap();
        for line in lines {
            writeln!(f, "{line}").unwrap();
        }
        f.flush().unwrap();
        f
    }

    #[test]
    fn ingest_empty_file() {
        let conn = setup();
        let f = write_tmp_jsonl(&[]);
        let summary = ingest_jsonl_file(&conn, "test", f.path(), false, "agent1").unwrap();
        assert_eq!(summary.processed_count, 0);
        assert_eq!(summary.inserted_count, 0);
        assert_eq!(summary.error_count, 0);
    }

    #[test]
    fn ingest_single_message_event() {
        let conn = setup();
        let event = r#"{"type":"message.queued","content":"hello","session_id":"s1","id":"e1","timestamp":1700000000}"#;
        let f = write_tmp_jsonl(&[event]);
        let summary = ingest_jsonl_file(&conn, "openclaw", f.path(), false, "agent1").unwrap();
        assert_eq!(summary.processed_count, 1);
        assert_eq!(summary.inserted_count, 1);
        assert_eq!(summary.projected_count, 1);
        assert_eq!(summary.error_count, 0);

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM external_events", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);

        let msg_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM messages WHERE session_id = 's1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(msg_count, 1);
    }

    #[test]
    fn ingest_deduplication() {
        let conn = setup();
        let event = r#"{"type":"message.queued","content":"hello","id":"dup1","timestamp":1700000000}"#;
        let f = write_tmp_jsonl(&[event, event]);
        let summary = ingest_jsonl_file(&conn, "test", f.path(), false, "agent1").unwrap();
        assert_eq!(summary.inserted_count, 1);
        assert_eq!(summary.deduped_count, 1);
    }

    #[test]
    fn ingest_incremental_resume() {
        let conn = setup();
        let e1 = r#"{"type":"message.queued","content":"first","id":"inc1","timestamp":1700000000}"#;
        let e2 = r#"{"type":"message.queued","content":"second","id":"inc2","timestamp":1700000001}"#;

        // Use a persistent file path so the resume lookup matches
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("events.jsonl");

        std::fs::write(&file_path, format!("{e1}\n")).unwrap();
        let s1 = ingest_jsonl_file(&conn, "test", &file_path, false, "agent1").unwrap();
        assert_eq!(s1.inserted_count, 1);
        assert_eq!(s1.to_line, 1);

        std::fs::write(&file_path, format!("{e1}\n{e2}\n")).unwrap();
        let s2 = ingest_jsonl_file(&conn, "test", &file_path, false, "agent1").unwrap();
        assert_eq!(s2.from_line, 2);
        assert_eq!(s2.inserted_count, 1);
    }

    #[test]
    fn ingest_replay_reruns_from_start() {
        let conn = setup();
        let event = r#"{"type":"message.queued","content":"replayed","id":"rpl1","timestamp":1700000000}"#;
        let f = write_tmp_jsonl(&[event]);

        ingest_jsonl_file(&conn, "test", f.path(), false, "agent1").unwrap();
        let s2 = ingest_jsonl_file(&conn, "test", f.path(), true, "agent1").unwrap();
        assert_eq!(s2.from_line, 1);
        assert_eq!(s2.deduped_count, 1);
    }

    #[test]
    fn ingest_model_usage_projection() {
        let conn = setup();
        let event = r#"{"type":"model.usage","provider":"anthropic","model":"claude-3","input_tokens":100,"output_tokens":50,"cost_usd":0.01,"id":"mu1","timestamp":1700000000}"#;
        let f = write_tmp_jsonl(&[event]);
        let s = ingest_jsonl_file(&conn, "test", f.path(), false, "agent1").unwrap();
        assert_eq!(s.projected_count, 1);

        let tool_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM tool_calls WHERE tool_name LIKE 'model.usage%'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(tool_count, 1);
    }

    #[test]
    fn ingest_run_event_projection() {
        let conn = setup();
        let event = r#"{"type":"run.completed","status":"success","output":"done","id":"run1","timestamp":1700000000}"#;
        let f = write_tmp_jsonl(&[event]);
        let s = ingest_jsonl_file(&conn, "test", f.path(), false, "agent1").unwrap();
        assert_eq!(s.projected_count, 1);

        let tc: i64 = conn
            .query_row("SELECT COUNT(*) FROM tool_calls WHERE tool_name = 'run.completed'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(tc, 1);
    }

    #[test]
    fn ingest_webhook_event_creates_audit() {
        let conn = setup();
        let event = r#"{"type":"webhook.received","provider":"github","url":"/hook","id":"wh1","timestamp":1700000000}"#;
        let f = write_tmp_jsonl(&[event]);
        let s = ingest_jsonl_file(&conn, "test", f.path(), false, "agent1").unwrap();
        assert_eq!(s.projected_count, 1);

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM policy_audit WHERE action = 'webhook.received'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn ingest_malformed_json_counted_as_processed() {
        let conn = setup();
        let f = write_tmp_jsonl(&["not json at all"]);
        let s = ingest_jsonl_file(&conn, "test", f.path(), false, "agent1").unwrap();
        assert_eq!(s.processed_count, 1);
        assert_eq!(s.inserted_count, 1);
    }

    #[test]
    fn ingest_blank_lines_skipped() {
        let conn = setup();
        let event = r#"{"type":"message.queued","content":"hello","id":"bl1","timestamp":1700000000}"#;
        let f = write_tmp_jsonl(&["", event, "  ", ""]);
        let s = ingest_jsonl_file(&conn, "test", f.path(), false, "agent1").unwrap();
        assert_eq!(s.inserted_count, 1);
    }

    #[test]
    fn recent_runs_returns_completed() {
        let conn = setup();
        let event = r#"{"type":"session.start","id":"rr1","timestamp":1700000000}"#;
        let f = write_tmp_jsonl(&[event]);
        ingest_jsonl_file(&conn, "test", f.path(), false, "agent1").unwrap();

        let runs = recent_runs(&conn, Some("test"), None, None, 10).unwrap();
        assert_eq!(runs.len(), 1);
        assert!(runs[0].status.starts_with("completed"));
    }

    #[test]
    fn preflight_counts_inserts_and_dedupes() {
        let conn = setup();
        let e1 = r#"{"type":"message.queued","content":"a","id":"pf1","timestamp":1700000000}"#;
        let e2 = r#"{"type":"message.queued","content":"b","id":"pf2","timestamp":1700000001}"#;

        let f1 = write_tmp_jsonl(&[e1]);
        ingest_jsonl_file(&conn, "test", f1.path(), false, "agent1").unwrap();

        let f2 = write_tmp_jsonl(&[e1, e2]);
        let pf = preflight_jsonl_file(&conn, "test", f2.path(), true).unwrap();
        assert_eq!(pf.would_dedupe_count, 1);
        assert_eq!(pf.would_insert_count, 1);
    }

    #[test]
    fn normalized_fields_extracted() {
        let payload: Value = serde_json::json!({
            "provider": "openai",
            "model": "gpt-4",
            "input_tokens": 100,
            "output_tokens": 200,
            "cost_usd": 0.05,
            "correlation_id": "corr-123"
        });
        let nf = normalized_projection_fields(&payload, "test", "key1", Some("session1"));
        assert_eq!(nf.provider.as_deref(), Some("openai"));
        assert_eq!(nf.model.as_deref(), Some("gpt-4"));
        assert_eq!(nf.input_tokens, Some(100));
        assert_eq!(nf.output_tokens, Some(200));
        assert_eq!(nf.total_tokens, Some(300));
        assert_eq!(nf.cost_usd, Some(0.05));
        assert_eq!(nf.correlation_id.as_deref(), Some("corr-123"));
    }

    #[test]
    fn normalized_fields_fallback() {
        let payload: Value = serde_json::json!({"some_field": "value"});
        let nf = normalized_projection_fields(&payload, "src", "key", None);
        assert!(nf.provider.is_none());
        assert!(nf.model.is_none());
        assert_eq!(nf.correlation_id.as_deref(), Some("src:key"));
    }

    #[test]
    fn pick_str_checks_multiple_keys() {
        let v: Value = serde_json::json!({"sessionId": "abc"});
        let result = pick_str(&v, &["session_id", "sessionId"]);
        assert_eq!(result, Some("abc"));
    }

    #[test]
    fn pick_ts_parses_rfc3339() {
        let v: Value = serde_json::json!({"timestamp": "2024-01-15T10:30:00Z"});
        let ts = pick_ts(&v);
        assert!(ts.is_some());
        assert!(ts.unwrap() > 1700000000);
    }

    #[test]
    fn pick_ts_parses_integer() {
        let v: Value = serde_json::json!({"ts": 1700000000});
        assert_eq!(pick_ts(&v), Some(1700000000));
    }

    #[test]
    fn stable_hash_deterministic() {
        let h1 = stable_hash_hex("hello world");
        let h2 = stable_hash_hex("hello world");
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 16);
    }

    #[test]
    fn candidate_from_message_too_short() {
        assert!(candidate_from_message("Short text.").is_none());
    }

    #[test]
    fn candidate_from_message_json_rejected() {
        assert!(candidate_from_message(r#"{"key": "value", "another": "field that is long enough to pass length check"}"#).is_none());
    }

    #[test]
    fn candidate_from_message_valid() {
        let c = candidate_from_message(
            "The Moneypenny agent platform uses SQLite for durable storage and CRDT sync across multiple agents."
        );
        assert!(c.is_some());
        let c = c.unwrap();
        assert!(!c.content.is_empty());
        assert!(!c.pointer.is_empty());
        assert_eq!(c.confidence, 0.7);
    }

    #[test]
    fn extract_keywords_filters_short_words() {
        let kw = extract_keywords("the a is of and but longer words here");
        assert!(kw.is_some());
        let kw = kw.unwrap();
        assert!(!kw.contains("the"));
        assert!(kw.contains("longer"));
    }
}
