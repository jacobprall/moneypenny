use crate::provider::{LlmProvider, StreamResult};
use crate::types::*;
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use tokio_stream::StreamExt;

const ANTHROPIC_API_VERSION: &str = "2023-06-01";
const DEFAULT_MAX_TOKENS: u32 = 8192;

pub struct AnthropicProvider {
    client: Client,
    api_base: String,
    api_key: Option<String>,
    model: String,
}

impl AnthropicProvider {
    pub fn new(
        api_base: impl Into<String>,
        api_key: Option<String>,
        model: impl Into<String>,
    ) -> Self {
        Self {
            client: Client::new(),
            api_base: api_base.into(),
            api_key,
            model: model.into(),
        }
    }

    pub fn from_config(
        api_base: Option<&str>,
        api_key: Option<&str>,
        model: Option<&str>,
    ) -> Self {
        Self::new(
            api_base.unwrap_or("https://api.anthropic.com"),
            api_key.map(String::from),
            model.unwrap_or("claude-sonnet-4-20250514"),
        )
    }
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    async fn generate(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
        config: &GenerateConfig,
    ) -> anyhow::Result<GenerateResponse> {
        let body = build_request(&self.model, messages, tools, config, false);

        let mut req = self.client
            .post(format!("{}/v1/messages", self.api_base))
            .header("anthropic-version", ANTHROPIC_API_VERSION)
            .header("content-type", "application/json")
            .json(&body);

        if let Some(key) = &self.api_key {
            req = req.header("x-api-key", key);
        }

        let resp = req.send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Anthropic API error {status}: {body}");
        }

        let api_resp: ApiResponse = resp.json().await?;
        parse_response(api_resp)
    }

    async fn generate_stream(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
        config: &GenerateConfig,
    ) -> anyhow::Result<StreamResult> {
        let body = build_request(&self.model, messages, tools, config, true);

        let mut req = self.client
            .post(format!("{}/v1/messages", self.api_base))
            .header("anthropic-version", ANTHROPIC_API_VERSION)
            .header("content-type", "application/json")
            .json(&body);

        if let Some(key) = &self.api_key {
            req = req.header("x-api-key", key);
        }

        let resp = req.send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Anthropic API error {status}: {body}");
        }

        let byte_stream = resp.bytes_stream();
        let stream = parse_sse_stream(byte_stream);
        Ok(Box::pin(stream))
    }

    async fn embed(&self, _text: &str) -> anyhow::Result<Vec<f32>> {
        anyhow::bail!(
            "Anthropic does not provide an embeddings API. \
             Configure a separate embedding provider (e.g. provider = \"http\" with an OpenAI-compatible endpoint) \
             or use local embeddings via sqlite-ai."
        )
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    fn name(&self) -> &str {
        "anthropic"
    }
}

// ---------------------------------------------------------------------------
// Request building
// ---------------------------------------------------------------------------

fn build_request(
    model: &str,
    messages: &[Message],
    tools: &[ToolDef],
    config: &GenerateConfig,
    stream: bool,
) -> serde_json::Value {
    let mut system_parts: Vec<String> = Vec::new();
    let mut api_messages: Vec<serde_json::Value> = Vec::new();

    for msg in messages {
        match msg.role {
            Role::System => {
                system_parts.push(msg.content.clone());
            }
            Role::User => {
                api_messages.push(serde_json::json!({
                    "role": "user",
                    "content": msg.content,
                }));
            }
            Role::Assistant => {
                let mut content_blocks: Vec<serde_json::Value> = Vec::new();

                if !msg.content.is_empty() {
                    content_blocks.push(serde_json::json!({
                        "type": "text",
                        "text": msg.content,
                    }));
                }

                for tc in &msg.tool_calls {
                    let input: serde_json::Value = serde_json::from_str(&tc.arguments)
                        .unwrap_or(serde_json::json!({}));
                    content_blocks.push(serde_json::json!({
                        "type": "tool_use",
                        "id": tc.id,
                        "name": tc.name,
                        "input": input,
                    }));
                }

                if content_blocks.is_empty() {
                    content_blocks.push(serde_json::json!({
                        "type": "text",
                        "text": "",
                    }));
                }

                api_messages.push(serde_json::json!({
                    "role": "assistant",
                    "content": content_blocks,
                }));
            }
            Role::Tool => {
                let tool_result = serde_json::json!({
                    "type": "tool_result",
                    "tool_use_id": msg.tool_call_id.as_deref().unwrap_or(""),
                    "content": msg.content,
                });

                // Anthropic requires tool results in a "user" message.
                // Merge consecutive tool results into one user message.
                if let Some(last) = api_messages.last_mut() {
                    if last.get("role").and_then(|r| r.as_str()) == Some("user") {
                        if let Some(content) = last.get_mut("content") {
                            if let Some(arr) = content.as_array_mut() {
                                if arr.iter().all(|b| b.get("type").and_then(|t| t.as_str()) == Some("tool_result")) {
                                    arr.push(tool_result);
                                    continue;
                                }
                            }
                        }
                    }
                }

                api_messages.push(serde_json::json!({
                    "role": "user",
                    "content": [tool_result],
                }));
            }
        }
    }

    let max_tokens = config.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS);

    let mut body = serde_json::json!({
        "model": model,
        "max_tokens": max_tokens,
        "messages": api_messages,
    });

    if !system_parts.is_empty() {
        body["system"] = serde_json::json!(system_parts.join("\n\n"));
    }

    if let Some(temp) = config.temperature {
        body["temperature"] = serde_json::json!(temp);
    }

    if !config.stop.is_empty() {
        body["stop_sequences"] = serde_json::json!(config.stop);
    }

    if !tools.is_empty() {
        let api_tools: Vec<serde_json::Value> = tools
            .iter()
            .map(|t| {
                serde_json::json!({
                    "name": t.name,
                    "description": t.description,
                    "input_schema": t.parameters,
                })
            })
            .collect();
        body["tools"] = serde_json::json!(api_tools);
    }

    if stream {
        body["stream"] = serde_json::json!(true);
    }

    body
}

// ---------------------------------------------------------------------------
// Response parsing
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ApiResponse {
    content: Vec<ContentBlock>,
    #[serde(default)]
    usage: Option<ApiUsage>,
    #[allow(dead_code)]
    #[serde(default)]
    stop_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
}

#[derive(Debug, Clone, Deserialize)]
struct ApiUsage {
    #[serde(default)]
    input_tokens: u32,
    #[serde(default)]
    output_tokens: u32,
}

fn parse_response(resp: ApiResponse) -> anyhow::Result<GenerateResponse> {
    let mut text_parts: Vec<String> = Vec::new();
    let mut tool_calls: Vec<ToolCall> = Vec::new();

    for block in resp.content {
        match block {
            ContentBlock::Text { text } => {
                if !text.is_empty() {
                    text_parts.push(text);
                }
            }
            ContentBlock::ToolUse { id, name, input } => {
                tool_calls.push(ToolCall {
                    id,
                    name,
                    arguments: serde_json::to_string(&input).unwrap_or_else(|_| "{}".into()),
                });
            }
        }
    }

    let content = if text_parts.is_empty() {
        None
    } else {
        Some(text_parts.join(""))
    };

    let usage = resp.usage.map(|u| Usage {
        prompt_tokens: u.input_tokens,
        completion_tokens: u.output_tokens,
        total_tokens: u.input_tokens + u.output_tokens,
    }).unwrap_or_default();

    Ok(GenerateResponse {
        content,
        tool_calls,
        usage,
    })
}

// ---------------------------------------------------------------------------
// SSE stream parsing
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct StreamEvent_ {
    #[serde(rename = "type")]
    event_type: String,

    #[serde(default)]
    index: Option<usize>,

    #[serde(default)]
    content_block: Option<StreamContentBlock>,

    #[serde(default)]
    delta: Option<StreamDelta>,

    #[serde(default)]
    usage: Option<ApiUsage>,

    #[serde(default)]
    message: Option<StreamMessage>,
}

#[derive(Debug, Deserialize)]
struct StreamMessage {
    #[serde(default)]
    usage: Option<ApiUsage>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum StreamContentBlock {
    #[serde(rename = "text")]
    Text {
        #[allow(dead_code)]
        text: String,
    },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        #[allow(dead_code)]
        input: serde_json::Value,
    },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum StreamDelta {
    #[serde(rename = "text_delta")]
    TextDelta { text: String },
    #[serde(rename = "input_json_delta")]
    InputJsonDelta { partial_json: String },
}

struct ToolAccumulator {
    id: String,
    name: String,
    input_json: String,
}

fn parse_sse_stream(
    byte_stream: impl futures_core::Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send + 'static,
) -> impl tokio_stream::Stream<Item = anyhow::Result<StreamEvent>> + Send {
    let mut tool_accumulators: Vec<Option<ToolAccumulator>> = Vec::new();
    let mut buf = String::new();
    let mut input_usage = Usage::default();

    async_stream::stream! {
        tokio::pin!(byte_stream);
        while let Some(chunk_result) = byte_stream.next().await {
            let chunk = match chunk_result {
                Ok(c) => c,
                Err(e) => { yield Err(e.into()); return; }
            };

            buf.push_str(&String::from_utf8_lossy(&chunk));

            while let Some(line_end) = buf.find('\n') {
                let line = buf[..line_end].trim().to_string();
                buf = buf[line_end + 1..].to_string();

                let json_str = if let Some(stripped) = line.strip_prefix("data: ") {
                    stripped
                } else {
                    continue;
                };

                if json_str.is_empty() {
                    continue;
                }

                let event: StreamEvent_ = match serde_json::from_str(json_str) {
                    Ok(e) => e,
                    Err(_) => continue,
                };

                match event.event_type.as_str() {
                    "message_start" => {
                        if let Some(msg) = &event.message {
                            if let Some(u) = &msg.usage {
                                input_usage = Usage {
                                    prompt_tokens: u.input_tokens,
                                    completion_tokens: 0,
                                    total_tokens: u.input_tokens,
                                };
                            }
                        }
                    }

                    "content_block_start" => {
                        let idx = event.index.unwrap_or(0);
                        while tool_accumulators.len() <= idx {
                            tool_accumulators.push(None);
                        }

                        if let Some(block) = event.content_block {
                            match block {
                                StreamContentBlock::ToolUse { id, name, .. } => {
                                    tool_accumulators[idx] = Some(ToolAccumulator {
                                        id,
                                        name,
                                        input_json: String::new(),
                                    });
                                }
                                StreamContentBlock::Text { .. } => {
                                    tool_accumulators[idx] = None;
                                }
                            }
                        }
                    }

                    "content_block_delta" => {
                        if let Some(delta) = event.delta {
                            match delta {
                                StreamDelta::TextDelta { text } => {
                                    if !text.is_empty() {
                                        yield Ok(StreamEvent::Delta(text));
                                    }
                                }
                                StreamDelta::InputJsonDelta { partial_json } => {
                                    let idx = event.index.unwrap_or(0);
                                    if let Some(Some(acc)) = tool_accumulators.get_mut(idx) {
                                        acc.input_json.push_str(&partial_json);
                                    }
                                }
                            }
                        }
                    }

                    "content_block_stop" => {
                        let idx = event.index.unwrap_or(0);
                        if let Some(acc) = tool_accumulators.get_mut(idx).and_then(|a| a.take()) {
                            let arguments = if acc.input_json.is_empty() {
                                "{}".to_string()
                            } else {
                                acc.input_json
                            };
                            yield Ok(StreamEvent::ToolCall(ToolCall {
                                id: acc.id,
                                name: acc.name,
                                arguments,
                            }));
                        }
                    }

                    "message_delta" => {
                        if let Some(u) = &event.usage {
                            input_usage.completion_tokens = u.output_tokens;
                            input_usage.total_tokens = input_usage.prompt_tokens + u.output_tokens;
                        }
                    }

                    "message_stop" => {
                        yield Ok(StreamEvent::Done(input_usage.clone()));
                    }

                    _ => {}
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

#[cfg(test)]
pub mod tests {
    use super::*;

    pub fn build_request_test(
        model: &str,
        messages: &[Message],
        tools: &[ToolDef],
        config: &GenerateConfig,
        stream: bool,
    ) -> serde_json::Value {
        build_request(model, messages, tools, config, stream)
    }

    pub fn parse_response_test(raw: serde_json::Value) -> GenerateResponse {
        let resp: ApiResponse = serde_json::from_value(raw).unwrap();
        parse_response(resp).unwrap()
    }

    pub fn try_parse_response_test(raw: serde_json::Value) -> anyhow::Result<GenerateResponse> {
        let resp: ApiResponse = serde_json::from_value(raw)?;
        parse_response(resp)
    }
}
