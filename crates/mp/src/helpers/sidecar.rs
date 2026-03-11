use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct SidecarOperationInput {
    pub op: String,
    #[serde(default)]
    pub op_version: Option<String>,
    #[serde(default)]
    pub request_id: Option<String>,
    #[serde(default)]
    pub idempotency_key: Option<String>,
    #[serde(default)]
    pub actor: Option<mp_core::operations::ActorContext>,
    #[serde(default)]
    pub context: Option<mp_core::operations::OperationContext>,
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub tenant_id: Option<String>,
    #[serde(default)]
    pub user_id: Option<String>,
    #[serde(default)]
    pub channel: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub trace_id: Option<String>,
    #[serde(default = "default_sidecar_args")]
    pub args: serde_json::Value,
}

fn default_sidecar_args() -> serde_json::Value {
    serde_json::json!({})
}

pub fn sidecar_error_response(code: &str, message: impl Into<String>) -> serde_json::Value {
    serde_json::json!({
        "ok": false,
        "code": code,
        "message": message.into(),
        "data": {},
        "policy": null,
        "audit": { "recorded": false }
    })
}

pub fn build_sidecar_request(
    input: serde_json::Value,
    default_agent_id: &str,
) -> anyhow::Result<mp_core::operations::OperationRequest> {
    if let Ok(req) = serde_json::from_value::<mp_core::operations::OperationRequest>(input.clone())
    {
        return Ok(req);
    }

    let compact: SidecarOperationInput = serde_json::from_value(input)?;
    let request_id = compact
        .request_id
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let actor = compact.actor.unwrap_or(mp_core::operations::ActorContext {
        agent_id: compact
            .agent_id
            .unwrap_or_else(|| default_agent_id.to_string()),
        tenant_id: compact.tenant_id,
        user_id: compact.user_id,
        channel: compact.channel.or(Some("mcp-stdio".into())),
    });

    let mut context = compact.context.unwrap_or_default();
    if context.session_id.is_none() {
        context.session_id = compact.session_id;
    }
    if context.trace_id.is_none() {
        context.trace_id = compact.trace_id.or(Some(request_id.clone()));
    }
    if context.timestamp.is_none() {
        context.timestamp = Some(chrono::Utc::now().timestamp());
    }

    Ok(mp_core::operations::OperationRequest {
        op: compact.op,
        op_version: compact.op_version.or(Some("v1".into())),
        request_id: Some(request_id),
        idempotency_key: compact.idempotency_key,
        actor,
        context,
        args: compact.args,
    })
}

pub fn op_request(
    agent_id: &str,
    op: &str,
    args: serde_json::Value,
) -> mp_core::operations::OperationRequest {
    let request_id = uuid::Uuid::new_v4().to_string();
    mp_core::operations::OperationRequest {
        op: op.to_string(),
        op_version: Some("v1".into()),
        request_id: Some(request_id.clone()),
        idempotency_key: None,
        actor: mp_core::operations::ActorContext {
            agent_id: agent_id.to_string(),
            tenant_id: None,
            user_id: None,
            channel: Some("cli".into()),
        },
        context: mp_core::operations::OperationContext {
            session_id: None,
            trace_id: Some(request_id),
            timestamp: Some(chrono::Utc::now().timestamp()),
        },
        args,
    }
}
