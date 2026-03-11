use rusqlite::Connection;
use super::{AuditMeta, OperationRequest, OperationResponse, denied_response, evaluate_policy_with_request_context, policy_meta};

pub(super) fn op_knowledge_ingest(
    conn: &Connection,
    req: &OperationRequest,
) -> anyhow::Result<OperationResponse> {
    let mut content = req.args["content"].as_str().map(str::to_string);
    let path = req.args["path"].as_str();
    let mut title = req.args["title"].as_str().map(str::to_string);
    let mut metadata = req.args["metadata"].as_str().map(str::to_string);
    let scope = req.args["scope"].as_str().unwrap_or("shared");

    let is_url = path.map_or(false, |p| {
        p.starts_with("http://") || p.starts_with("https://")
    });
    let knowledge_resource = if is_url {
        crate::policy::resource::knowledge(Some("url"))
    } else {
        crate::policy::resource::knowledge(None)
    };

    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "ingest",
            resource: &knowledge_resource,
            sql_content: None,
            channel: req.actor.channel.as_deref(),
            arguments: if is_url { path } else { None },
        },
        req,
    )?;
    if matches!(decision.effect, crate::policy::Effect::Deny) {
        return Ok(denied_response(&decision));
    }

    if content.is_none() {
        if let Some(url) = path.filter(|p| p.starts_with("http://") || p.starts_with("https://")) {
            let fetched = fetch_url_for_knowledge_ingest(url)?;
            if title.is_none() {
                title = fetched.title;
            }
            if metadata.is_none() {
                metadata = Some(
                    serde_json::json!({
                        "source_url": url,
                        "content_type": fetched.content_type,
                    })
                    .to_string(),
                );
            }
            content = Some(fetched.content);
        }
    }

    let content = content
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("missing 'content'"))?;
    let (doc_id, chunk_count) = crate::store::knowledge::ingest_scoped(
        conn,
        path,
        title.as_deref(),
        content,
        metadata.as_deref(),
        Some(&req.actor.agent_id),
        Some(scope),
    )?;

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: "knowledge ingested".into(),
        data: serde_json::json!({
            "document_id": doc_id,
            "chunks_created": chunk_count,
            "scope": scope
        }),
        policy: Some(policy_meta(&decision)),
        audit: AuditMeta { recorded: true },
    })
}

struct FetchedKnowledgeContent {
    content: String,
    title: Option<String>,
    content_type: Option<String>,
}

fn fetch_url_for_knowledge_ingest(url: &str) -> anyhow::Result<FetchedKnowledgeContent> {
    let mut response = ureq::get(url)
        .header("User-Agent", "Moneypenny/0.1 (https://github.com/jacobprall/moneypenny)")
        .call()
        .map_err(|e| anyhow::anyhow!("HTTP fetch failed for {url}: {e}"))?;

    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);

    let body = response
        .body_mut()
        .read_to_string()
        .map_err(|e| anyhow::anyhow!("failed to read response body from {url}: {e}"))?;

    if body.trim().is_empty() {
        anyhow::bail!("fetched URL returned empty content: {url}");
    }

    let is_html = content_type
        .as_deref()
        .map(|ct| ct.contains("text/html"))
        .unwrap_or(false)
        || crate::store::knowledge::is_probably_html_document(&body);
    let title = if is_html {
        crate::store::knowledge::extract_html_title(&body)
    } else {
        None
    }
    .or_else(|| {
        url.rsplit('/')
            .find(|s| !s.is_empty())
            .map(str::to_string)
            .or_else(|| Some(url.to_string()))
    });

    Ok(FetchedKnowledgeContent {
        content: body,
        title,
        content_type,
    })
}

pub(super) fn op_knowledge_search(
    conn: &Connection,
    req: &OperationRequest,
) -> anyhow::Result<OperationResponse> {
    let query = req.args["query"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'query'"))?;
    let limit = req.args["limit"].as_u64().unwrap_or(20) as usize;

    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "search",
            resource: crate::policy::resource::KNOWLEDGE,
            sql_content: None,
            channel: req.actor.channel.as_deref(),
            arguments: None,
        },
        req,
    )?;
    if matches!(decision.effect, crate::policy::Effect::Deny) {
        return Ok(denied_response(&decision));
    }

    let rows = crate::search::fts5_search_knowledge(conn, query, limit)?;
    let data = rows
        .into_iter()
        .map(|(id, content, score)| {
            serde_json::json!({
                "id": id,
                "content": content,
                "score": score
            })
        })
        .collect::<Vec<_>>();

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: "knowledge search completed".into(),
        data: serde_json::json!(data),
        policy: Some(policy_meta(&decision)),
        audit: AuditMeta { recorded: true },
    })
}

pub(super) fn op_knowledge_list(
    conn: &Connection,
    req: &OperationRequest,
) -> anyhow::Result<OperationResponse> {
    let decision = evaluate_policy_with_request_context(
        conn,
        &crate::policy::PolicyRequest {
            actor: &req.actor.agent_id,
            action: "list",
            resource: crate::policy::resource::KNOWLEDGE,
            sql_content: None,
            channel: req.actor.channel.as_deref(),
            arguments: None,
        },
        req,
    )?;
    if matches!(decision.effect, crate::policy::Effect::Deny) {
        return Ok(denied_response(&decision));
    }

    let docs = crate::store::knowledge::list_documents(conn)?;
    let data = docs
        .into_iter()
        .map(|d| {
            serde_json::json!({
                "id": d.id,
                "agent_id": d.agent_id,
                "scope": d.scope,
                "title": d.title,
                "path": d.path,
                "content_hash": d.content_hash,
                "created_at": d.created_at,
                "updated_at": d.updated_at
            })
        })
        .collect::<Vec<_>>();

    Ok(OperationResponse {
        ok: true,
        code: "ok".into(),
        message: "knowledge documents listed".into(),
        data: serde_json::json!(data),
        policy: Some(policy_meta(&decision)),
        audit: AuditMeta { recorded: true },
    })
}
