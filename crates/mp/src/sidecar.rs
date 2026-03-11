use crate::domain_tools;
use crate::helpers::{
    build_embedding_provider, build_sidecar_request, embedding_model_id, open_agent_db,
    resolve_agent, sidecar_error_response,
};
use anyhow::Result;
use mp_core::config::Config;
use mp_llm::provider::EmbeddingProvider;
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

fn mcp_tools_list_result() -> serde_json::Value {
    domain_tools::tools_list()
}

fn jsonrpc_result(id: Option<serde_json::Value>, result: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id.unwrap_or(serde_json::Value::Null),
        "result": result
    })
}

fn jsonrpc_error(
    id: Option<serde_json::Value>,
    code: i64,
    message: impl Into<String>,
) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id.unwrap_or(serde_json::Value::Null),
        "error": {
            "code": code,
            "message": message.into()
        }
    })
}

fn request_id_from_jsonrpc(input: &serde_json::Value) -> Option<String> {
    let id = input.get("id")?;
    match id {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        serde_json::Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct SidecarToolStats {
    selection_count: u64,
    success_count: u64,
    error_count: u64,
    fallback_count: u64,
    invalid_action_count: u64,
}

static SIDECAR_TOOL_STATS: LazyLock<Mutex<HashMap<String, SidecarToolStats>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn record_sidecar_tool_event(tool: &str, success: bool, fallback: bool, invalid_action: bool) {
    if let Ok(mut guard) = SIDECAR_TOOL_STATS.lock() {
        let entry = guard.entry(tool.to_string()).or_default();
        entry.selection_count += 1;
        if success {
            entry.success_count += 1;
        } else {
            entry.error_count += 1;
        }
        if fallback {
            entry.fallback_count += 1;
        }
        if invalid_action {
            entry.invalid_action_count += 1;
        }
    }
}

fn sidecar_tool_stats_snapshot() -> serde_json::Value {
    if let Ok(guard) = SIDECAR_TOOL_STATS.lock() {
        let mut rows = Vec::new();
        for (tool, s) in guard.iter() {
            let selection = s.selection_count.max(1) as f64;
            rows.push(serde_json::json!({
                "tool": tool,
                "selection_rate": s.selection_count,
                "success_rate": (s.success_count as f64) / selection,
                "fallback_rate": (s.fallback_count as f64) / selection,
                "invalid_action_rate": (s.invalid_action_count as f64) / selection,
                "errors": s.error_count
            }));
        }
        serde_json::json!(rows)
    } else {
        serde_json::json!([])
    }
}

enum ParsedMcpToolCall {
    Operation {
        request: mp_core::operations::OperationRequest,
        tool: String,
        action: String,
        fallback: bool,
    },
    DirectResponse {
        payload: serde_json::Value,
        tool: String,
    },
    MpqQuery {
        expression: String,
        dry_run: bool,
        agent_id: String,
        channel: Option<String>,
        session_id: Option<String>,
        trace_id: Option<String>,
    },
}

fn build_sidecar_request_from_mcp_call(
    input: &serde_json::Value,
    default_agent_id: &str,
) -> anyhow::Result<ParsedMcpToolCall> {
    let params = input
        .get("params")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| anyhow::anyhow!("missing object params for tools/call"))?;
    let tool_name = params
        .get("name")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("missing params.name"))?;
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    let request_id = params
        .get("request_id")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .or_else(|| request_id_from_jsonrpc(input));

    let tool_specific = if tool_name.starts_with("moneypenny.") || tool_name.starts_with("moneypenny_") {
        Some(domain_tools::route_tool_call(tool_name, &arguments)?)
    } else {
        None
    };

    let (op, args, tool_label, action, fallback) = match tool_specific {
        Some(domain_tools::RoutedToolCall::MpqQuery {
            expression,
            dry_run,
        }) => {
            let agent_id = params
                .get("agent_id")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(default_agent_id)
                .to_string();
            let channel = params
                .get("channel")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string);
            let session_id = params
                .get("session_id")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string);
            let trace_id = params
                .get("trace_id")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
                .or_else(|| request_id.clone());
            return Ok(ParsedMcpToolCall::MpqQuery {
                expression,
                dry_run,
                agent_id,
                channel,
                session_id,
                trace_id,
            });
        }
        Some(domain_tools::RoutedToolCall::Capabilities { payload }) => {
            return Ok(ParsedMcpToolCall::DirectResponse {
                payload,
                tool: tool_name.to_string(),
            });
        }
        Some(domain_tools::RoutedToolCall::Operation {
            domain_tool,
            action,
            op,
            args,
            execute_fallback,
        }) => (op, args, domain_tool, action, execute_fallback),
        None => (
            tool_name.to_string(),
            arguments,
            tool_name.to_string(),
            "legacy".to_string(),
            false,
        ),
    };
    let request_id = params
        .get("request_id")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .or(request_id);

    let request = build_sidecar_request(
        serde_json::json!({
            "op": op,
            "request_id": request_id,
            "idempotency_key": params.get("idempotency_key").cloned(),
            "agent_id": params.get("agent_id").cloned(),
            "tenant_id": params.get("tenant_id").cloned(),
            "user_id": params.get("user_id").cloned(),
            "channel": params.get("channel").cloned(),
            "session_id": params.get("session_id").cloned(),
            "trace_id": params.get("trace_id").cloned(),
            "args": args
        }),
        default_agent_id,
    )?;

    Ok(ParsedMcpToolCall::Operation {
        request,
        tool: tool_label,
        action,
        fallback,
    })
}

pub async fn handle_sidecar_mcp_request(
    conn: &rusqlite::Connection,
    input: &serde_json::Value,
    default_agent_id: &str,
    embed_provider: Option<&dyn EmbeddingProvider>,
    embedding_model_id: &str,
) -> anyhow::Result<Option<serde_json::Value>> {
    let method = match input.get("method").and_then(serde_json::Value::as_str) {
        Some(m) => m,
        None => return Ok(None),
    };
    let id = input.get("id").cloned();
    if id.is_none() {
        return Ok(None);
    }

    let response = match method {
        "initialize" => jsonrpc_result(
            id,
            serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": { "listChanged": false }
                },
                "serverInfo": {
                    "name": "moneypenny-sidecar",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }),
        ),
        "tools/list" => jsonrpc_result(id, mcp_tools_list_result()),
        "tools/call" => {
            let parsed_call = match build_sidecar_request_from_mcp_call(input, default_agent_id) {
                Ok(r) => r,
                Err(e) => {
                    record_sidecar_tool_event("moneypenny.invalid", false, false, true);
                    return Ok(Some(jsonrpc_error(
                        id,
                        -32602,
                        format!("invalid tools/call params: {e}"),
                    )));
                }
            };
            match parsed_call {
                ParsedMcpToolCall::MpqQuery {
                    expression,
                    dry_run,
                    agent_id,
                    channel,
                    session_id,
                    trace_id,
                } => {
                    let ctx = mp_core::dsl::ExecuteContext {
                        agent_id,
                        channel,
                        session_id,
                        trace_id,
                    };
                    let resp = mp_core::dsl::run(conn, &expression, dry_run, &ctx);
                    record_sidecar_tool_event(domain_tools::TOOL_QUERY, resp.ok, false, false);
                    let text = serde_json::to_string(&serde_json::json!({
                        "ok": resp.ok,
                        "code": resp.code,
                        "message": resp.message,
                        "data": resp.data,
                    }))
                    .unwrap_or_else(|_| "{}".to_string());
                    jsonrpc_result(
                        id,
                        serde_json::json!({
                            "content": [{
                                "type": "text",
                                "text": text
                            }],
                            "isError": !resp.ok
                        }),
                    )
                }
                ParsedMcpToolCall::DirectResponse { payload, tool } => {
                    let payload = if tool == domain_tools::TOOL_CAPABILITIES {
                        let mut p = payload;
                        if let Some(obj) = p.as_object_mut() {
                            obj.insert("telemetry".to_string(), sidecar_tool_stats_snapshot());
                        }
                        p
                    } else {
                        payload
                    };
                    record_sidecar_tool_event(&tool, true, false, false);
                    jsonrpc_result(
                        id,
                        serde_json::json!({
                            "content": [{
                                "type": "text",
                                "text": serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string())
                            }],
                            "isError": false
                        }),
                    )
                }
                ParsedMcpToolCall::Operation {
                    request,
                    tool,
                    action,
                    fallback,
                } => {
                    if fallback && domain_tools::covered_ops().contains(&request.op.as_str()) {
                        record_sidecar_tool_event(&tool, false, true, false);
                    }

                    let op_resp = match execute_sidecar_operation(
                        conn,
                        &request,
                        embed_provider,
                        embedding_model_id,
                    )
                    .await
                    {
                        Ok(mut resp) => {
                            if let Some(obj) = resp.data.as_object_mut() {
                                obj.insert(
                                    "next_actions".to_string(),
                                    serde_json::Value::Array(domain_tools::next_actions(
                                        &tool, &action,
                                    )),
                                );
                            }
                            record_sidecar_tool_event(&tool, resp.ok, fallback, false);
                            resp
                        }
                        Err(e) => {
                            record_sidecar_tool_event(&tool, false, fallback, false);
                            let err =
                                sidecar_error_response("sidecar_execute_error", e.to_string());
                            return Ok(Some(jsonrpc_result(
                                id,
                                serde_json::json!({
                                    "content": [{ "type": "text", "text": err.to_string() }],
                                    "isError": true
                                }),
                            )));
                        }
                    };

                    jsonrpc_result(
                        id,
                        serde_json::json!({
                            "content": [{
                                "type": "text",
                                "text": serde_json::to_string(&op_resp).unwrap_or_else(|_| "{}".to_string())
                            }],
                            "isError": !op_resp.ok
                        }),
                    )
                }
            }
        }
        unknown if unknown.starts_with("notifications/") && id.is_none() => return Ok(None),
        _ => jsonrpc_error(id, -32601, format!("method not found: {method}")),
    };

    Ok(Some(response))
}

pub async fn execute_sidecar_operation(
    conn: &rusqlite::Connection,
    req: &mp_core::operations::OperationRequest,
    embed_provider: Option<&dyn EmbeddingProvider>,
    embedding_model_id: &str,
) -> anyhow::Result<mp_core::operations::OperationResponse> {
    if req.op == "embedding.process" || req.op == "embedding.backfill.process" {
        return execute_embedding_process_operation(conn, req, embed_provider, embedding_model_id)
            .await;
    }
    let maybe_enriched = enrich_memory_search_request_with_embedding(req, embed_provider).await;
    mp_core::operations::execute(conn, &maybe_enriched)
}

async fn enrich_memory_search_request_with_embedding(
    req: &mp_core::operations::OperationRequest,
    embed_provider: Option<&dyn EmbeddingProvider>,
) -> mp_core::operations::OperationRequest {
    if req.op != "memory.search" {
        return req.clone();
    }
    let Some(embedder) = embed_provider else {
        return req.clone();
    };
    if req.args.get("__query_embedding").is_some() || req.args.get("query_embedding").is_some() {
        return req.clone();
    }

    let mut enriched = req.clone();
    let Some(query) = enriched
        .args
        .get("query")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|q| !q.is_empty())
    else {
        return req.clone();
    };

    match embedder.embed(query).await {
        Ok(vec) => {
            let embedding = vec
                .into_iter()
                .map(|v| serde_json::Value::from(v as f64))
                .collect::<Vec<_>>();
            if let Some(obj) = enriched.args.as_object_mut() {
                obj.insert(
                    "__query_embedding".to_string(),
                    serde_json::Value::Array(embedding),
                );
            }
            enriched
        }
        Err(e) => {
            tracing::debug!("sidecar memory.search embedding generation failed: {e}");
            req.clone()
        }
    }
}

async fn execute_embedding_process_operation(
    conn: &rusqlite::Connection,
    req: &mp_core::operations::OperationRequest,
    embed_provider: Option<&dyn EmbeddingProvider>,
    default_model_id: &str,
) -> anyhow::Result<mp_core::operations::OperationResponse> {
    let Some(embed) = embed_provider else {
        return Ok(mp_core::operations::OperationResponse {
            ok: false,
            code: "embedding_provider_unavailable".into(),
            message: "embedding provider is not configured or failed to initialize".into(),
            data: serde_json::json!({}),
            policy: None,
            audit: mp_core::operations::AuditMeta { recorded: false },
        });
    };

    let agent_id = req.args["agent_id"]
        .as_str()
        .unwrap_or(&req.actor.agent_id)
        .to_string();
    let model_id = req.args["model_id"]
        .as_str()
        .unwrap_or(default_model_id)
        .to_string();
    let limit_per_target = req.args["limit"].as_u64().unwrap_or(10_000) as usize;
    let max_batches = req.args["max_batches"].as_u64().unwrap_or(200) as usize;
    let batch_size = req.args["batch_size"].as_u64().unwrap_or(128) as usize;
    let retry_base_seconds = req.args["retry_base_seconds"].as_i64().unwrap_or(5);
    let max_attempts = req.args["max_attempts"].as_i64().unwrap_or(8);
    let enqueue_drift = req.op == "embedding.backfill.process"
        || req.args["enqueue_drift"].as_bool().unwrap_or(false);

    let mut total_queued = 0usize;
    if enqueue_drift {
        total_queued = mp_core::store::embedding::enqueue_drift_jobs(
            conn,
            &agent_id,
            &model_id,
            limit_per_target,
        )?;
    }

    let mut total_claimed = 0usize;
    let mut total_embedded = 0usize;
    let mut total_failed = 0usize;
    let mut total_skipped = 0usize;
    let mut rounds = 0usize;

    loop {
        rounds += 1;
        let stats = mp_core::store::embedding::process_embedding_jobs(
            conn,
            &agent_id,
            &model_id,
            batch_size.max(1),
            retry_base_seconds.max(1),
            max_attempts.max(1),
            |content| async move {
                let vec = embed.embed(&content).await?;
                Ok::<Vec<u8>, anyhow::Error>(mp_llm::f32_slice_to_blob(&vec))
            },
        )
        .await?;
        total_claimed += stats.claimed;
        total_embedded += stats.embedded;
        total_failed += stats.failed;
        total_skipped += stats.skipped;

        if stats.claimed == 0 || rounds >= max_batches.max(1) {
            break;
        }
    }

    let queue = mp_core::store::embedding::queue_stats(conn)?;
    Ok(mp_core::operations::OperationResponse {
        ok: true,
        code: "ok".into(),
        message: "embedding queue processed".into(),
        data: serde_json::json!({
            "agent_id": agent_id,
            "model_id": model_id,
            "enqueue_drift": enqueue_drift,
            "queued": total_queued,
            "rounds": rounds,
            "claimed": total_claimed,
            "embedded": total_embedded,
            "failed": total_failed,
            "skipped": total_skipped,
            "queue": {
                "total": queue.total,
                "pending": queue.pending,
                "retry": queue.retry,
                "processing": queue.processing,
                "dead": queue.dead,
            }
        }),
        policy: None,
        audit: mp_core::operations::AuditMeta { recorded: true },
    })
}

pub async fn cmd_sidecar(config: &Config, agent: Option<String>) -> Result<()> {
    let ag = resolve_agent(config, agent.as_deref())?;
    let conn = open_agent_db(config, &ag.name)?;
    let embed_provider = build_embedding_provider(config, ag).ok();
    let sidecar_embedding_model_id = embedding_model_id(ag);

    let stdin = tokio::io::stdin();
    let reader = tokio::io::BufReader::new(stdin);
    let mut lines = tokio::io::AsyncBufReadExt::lines(reader);
    let mut stdout = tokio::io::stdout();

    while let Ok(Some(line)) = lines.next_line().await {
        let parsed: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                let err = sidecar_error_response("invalid_json", e.to_string());
                tokio::io::AsyncWriteExt::write_all(&mut stdout, format!("{err}\n").as_bytes())
                    .await?;
                tokio::io::AsyncWriteExt::flush(&mut stdout).await?;
                continue;
            }
        };

        if let Some(mcp_response) = handle_sidecar_mcp_request(
            &conn,
            &parsed,
            &ag.name,
            embed_provider.as_deref(),
            &sidecar_embedding_model_id,
        )
        .await?
        {
            tokio::io::AsyncWriteExt::write_all(
                &mut stdout,
                format!("{mcp_response}\n").as_bytes(),
            )
            .await?;
            tokio::io::AsyncWriteExt::flush(&mut stdout).await?;
            continue;
        }

        if parsed.get("method").is_some() && parsed.get("id").is_none() {
            continue;
        }

        let request = match build_sidecar_request(parsed, &ag.name) {
            Ok(r) => r,
            Err(e) => {
                let err = sidecar_error_response("invalid_request", e.to_string());
                tokio::io::AsyncWriteExt::write_all(&mut stdout, format!("{err}\n").as_bytes())
                    .await?;
                tokio::io::AsyncWriteExt::flush(&mut stdout).await?;
                continue;
            }
        };

        let response = match execute_sidecar_operation(
            &conn,
            &request,
            embed_provider.as_deref(),
            &sidecar_embedding_model_id,
        )
        .await
        {
            Ok(resp) => serde_json::to_value(resp)
                .unwrap_or_else(|e| sidecar_error_response("serialization_error", e.to_string())),
            Err(e) => sidecar_error_response("sidecar_execute_error", e.to_string()),
        };

        tokio::io::AsyncWriteExt::write_all(&mut stdout, format!("{response}\n").as_bytes())
            .await?;
        tokio::io::AsyncWriteExt::flush(&mut stdout).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        ParsedMcpToolCall, build_sidecar_request_from_mcp_call, mcp_tools_list_result,
    };
    use crate::helpers::build_sidecar_request;

    #[test]
    fn sidecar_compact_request_uses_defaults() {
        let req = build_sidecar_request(
            serde_json::json!({
                "op": "job.list",
                "args": { "agent_id": "main" }
            }),
            "default-agent",
        )
        .expect("build sidecar request");
        assert_eq!(req.op, "job.list");
        assert_eq!(req.op_version.as_deref(), Some("v1"));
        assert_eq!(req.actor.agent_id, "default-agent");
        assert_eq!(req.actor.channel.as_deref(), Some("mcp-stdio"));
        assert!(req.request_id.is_some());
        assert!(req.context.trace_id.is_some());
    }

    #[test]
    fn sidecar_full_operation_request_passes_through() {
        let req = build_sidecar_request(
            serde_json::json!({
                "op": "session.list",
                "op_version": "v1",
                "request_id": "rid-1",
                "idempotency_key": null,
                "actor": {
                    "agent_id": "main",
                    "tenant_id": null,
                    "user_id": null,
                    "channel": "cli"
                },
                "context": {
                    "session_id": null,
                    "trace_id": "trace-1",
                    "timestamp": 123
                },
                "args": { "limit": 3 }
            }),
            "default-agent",
        )
        .expect("parse full canonical request");
        assert_eq!(req.request_id.as_deref(), Some("rid-1"));
        assert_eq!(req.context.trace_id.as_deref(), Some("trace-1"));
        assert_eq!(req.actor.channel.as_deref(), Some("cli"));
        assert_eq!(req.args["limit"], 3);
    }

    #[test]
    fn sidecar_mcp_tools_call_translates_to_canonical_request() {
        let parsed = build_sidecar_request_from_mcp_call(
            &serde_json::json!({
                "jsonrpc": "2.0",
                "id": "rpc-42",
                "method": "tools/call",
                "params": {
                    "name": "moneypenny.ingest",
                    "arguments": { "action": "status", "input": { "limit": 5 } },
                    "agent_id": "main"
                }
            }),
            "default-agent",
        )
        .expect("translate tools/call to operation request");
        let req = match parsed {
            ParsedMcpToolCall::Operation { request, .. } => request,
            _ => panic!("expected operation"),
        };
        assert_eq!(req.op, "ingest.status");
        assert_eq!(req.request_id.as_deref(), Some("rpc-42"));
        assert_eq!(req.actor.agent_id, "main");
        assert_eq!(req.args["limit"], 5);
    }

    #[test]
    fn sidecar_mcp_prefixed_tool_name_maps_to_operation() {
        let parsed = build_sidecar_request_from_mcp_call(
            &serde_json::json!({
                "jsonrpc": "2.0",
                "id": "rpc-43",
                "method": "tools/call",
                "params": {
                    "name": "moneypenny.jobs",
                    "arguments": { "action": "list", "input": { "agent_id": "main" } }
                }
            }),
            "default-agent",
        )
        .expect("translate prefixed tools/call to operation request");
        let req = match parsed {
            ParsedMcpToolCall::Operation { request, .. } => request,
            _ => panic!("expected operation"),
        };
        assert_eq!(req.op, "job.list");
    }

    #[test]
    fn sidecar_mcp_tools_list_exposes_domain_tools() {
        let result = mcp_tools_list_result();
        let tools = result["tools"].as_array().cloned().unwrap_or_default();
        assert_eq!(tools.len(), 9, "MCP surface: brain + facts + knowledge + policy + activity + experience + events + focus + execute");
        assert!(tools.iter().any(|t| t["name"] == "moneypenny_brain"));
        assert!(tools.iter().any(|t| t["name"] == "moneypenny_facts"));
        assert!(tools.iter().any(|t| t["name"] == "moneypenny_knowledge"));
        assert!(tools.iter().any(|t| t["name"] == "moneypenny_policy"));
        assert!(tools.iter().any(|t| t["name"] == "moneypenny_activity"));
        assert!(tools.iter().any(|t| t["name"] == "moneypenny_experience"));
        assert!(tools.iter().any(|t| t["name"] == "moneypenny_events"));
        assert!(tools.iter().any(|t| t["name"] == "moneypenny_focus"));
        assert!(tools.iter().any(|t| t["name"] == "moneypenny_execute"));
        assert!(!tools.iter().any(|t| t["name"] == "moneypenny_query"));
    }

    #[test]
    fn sidecar_mcp_capabilities_returns_direct_payload() {
        let parsed = build_sidecar_request_from_mcp_call(
            &serde_json::json!({
                "jsonrpc": "2.0",
                "id": "rpc-44",
                "method": "tools/call",
                "params": {
                    "name": "moneypenny.capabilities",
                    "arguments": {}
                }
            }),
            "default-agent",
        )
        .expect("build capabilities call");
        match parsed {
            ParsedMcpToolCall::DirectResponse { payload, .. } => {
                assert!(payload["domains"].is_array());
            }
            _ => panic!("expected direct response"),
        }
    }
}
