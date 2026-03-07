use crate::provider::{LlmProvider, StreamResult};
use crate::types::*;
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use tokio_stream::StreamExt;

pub struct HttpProvider {
    client: Client,
    api_base: String,
    api_key: Option<String>,
    model: String,
}

impl HttpProvider {
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
}

#[async_trait]
impl LlmProvider for HttpProvider {
    async fn generate(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
        config: &GenerateConfig,
    ) -> anyhow::Result<GenerateResponse> {
        let body = build_chat_request(&self.model, messages, tools, config, false);

        let mut req = self.client
            .post(format!("{}/chat/completions", self.api_base))
            .json(&body);

        if let Some(key) = &self.api_key {
            req = req.bearer_auth(key);
        }

        let resp = req.send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("LLM API error {status}: {body}");
        }

        let api_resp: ChatCompletionResponse = resp.json().await?;
        parse_chat_response(api_resp)
    }

    async fn generate_stream(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
        config: &GenerateConfig,
    ) -> anyhow::Result<StreamResult> {
        let body = build_chat_request(&self.model, messages, tools, config, true);

        let mut req = self.client
            .post(format!("{}/chat/completions", self.api_base))
            .json(&body);

        if let Some(key) = &self.api_key {
            req = req.bearer_auth(key);
        }

        let resp = req.send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("LLM API error {status}: {body}");
        }

        let byte_stream = resp.bytes_stream();
        let stream = parse_sse_stream(byte_stream);
        Ok(Box::pin(stream))
    }

    async fn embed(&self, _text: &str) -> anyhow::Result<Vec<f32>> {
        anyhow::bail!(
            "Use the dedicated EmbeddingProvider for embeddings. \
             HttpProvider is for generation only. Configure [embedding] in moneypenny.toml."
        )
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    fn name(&self) -> &str {
        "http"
    }
}

// ---------------------------------------------------------------------------
// Request building
// ---------------------------------------------------------------------------

fn build_chat_request(
    model: &str,
    messages: &[Message],
    tools: &[ToolDef],
    config: &GenerateConfig,
    stream: bool,
) -> serde_json::Value {
    let api_messages: Vec<serde_json::Value> = messages
        .iter()
        .map(|m| {
            let mut msg = serde_json::json!({
                "role": m.role,
                "content": m.content,
            });
            if let Some(id) = &m.tool_call_id {
                msg["tool_call_id"] = serde_json::json!(id);
            }
            if !m.tool_calls.is_empty() {
                msg["tool_calls"] = serde_json::json!(
                    m.tool_calls.iter().map(|tc| serde_json::json!({
                        "id": tc.id,
                        "type": "function",
                        "function": {
                            "name": tc.name,
                            "arguments": tc.arguments,
                        }
                    })).collect::<Vec<_>>()
                );
            }
            msg
        })
        .collect();

    let mut body = serde_json::json!({
        "model": model,
        "messages": api_messages,
        "stream": stream,
    });

    if let Some(temp) = config.temperature {
        body["temperature"] = serde_json::json!(temp);
    }
    if let Some(max) = config.max_tokens {
        body["max_tokens"] = serde_json::json!(max);
    }
    if !config.stop.is_empty() {
        body["stop"] = serde_json::json!(config.stop);
    }
    if !tools.is_empty() {
        let api_tools: Vec<serde_json::Value> = tools
            .iter()
            .map(|t| {
                serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.parameters,
                    }
                })
            })
            .collect();
        body["tools"] = serde_json::json!(api_tools);
    }

    body
}

// ---------------------------------------------------------------------------
// Response parsing (OpenAI-compatible format)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
    #[serde(default)]
    usage: Option<ApiUsage>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

#[derive(Debug, Deserialize)]
struct ChatMessage {
    content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<ApiToolCall>,
}

#[derive(Debug, Deserialize)]
struct ApiToolCall {
    id: String,
    function: ApiFunction,
}

#[derive(Debug, Deserialize)]
struct ApiFunction {
    name: String,
    arguments: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ApiUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

fn parse_chat_response(resp: ChatCompletionResponse) -> anyhow::Result<GenerateResponse> {
    let choice = resp
        .choices
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("No choices in response"))?;

    let tool_calls = choice
        .message
        .tool_calls
        .into_iter()
        .map(|tc| ToolCall {
            id: tc.id,
            name: tc.function.name,
            arguments: tc.function.arguments,
        })
        .collect();

    let usage = resp.usage.map(|u| Usage {
        prompt_tokens: u.prompt_tokens,
        completion_tokens: u.completion_tokens,
        total_tokens: u.total_tokens,
    }).unwrap_or_default();

    Ok(GenerateResponse {
        content: choice.message.content,
        tool_calls,
        usage,
    })
}

// ---------------------------------------------------------------------------
// SSE stream parsing
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct StreamChunk {
    choices: Vec<StreamChoice>,
    #[serde(default)]
    usage: Option<ApiUsage>,
}

#[derive(Debug, Deserialize)]
struct StreamChoice {
    delta: StreamDelta,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct StreamDelta {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<StreamToolCall>,
}

#[derive(Debug, Deserialize)]
struct StreamToolCall {
    #[serde(default)]
    index: usize,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    function: Option<StreamFunction>,
}

#[derive(Debug, Deserialize)]
struct StreamFunction {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

fn parse_sse_stream(
    byte_stream: impl futures_core::Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send + 'static,
) -> impl tokio_stream::Stream<Item = anyhow::Result<StreamEvent>> + Send {
    let mut tool_accumulators: Vec<(String, String, String)> = Vec::new(); // (id, name, args)
    let mut buf = String::new();

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

                if line.is_empty() || line == "data: [DONE]" {
                    if line == "data: [DONE]" {
                        for (id, name, args) in tool_accumulators.drain(..) {
                            yield Ok(StreamEvent::ToolCall(ToolCall { id, name, arguments: args }));
                        }
                        yield Ok(StreamEvent::Done(Usage::default()));
                    }
                    continue;
                }

                let json_str = if let Some(stripped) = line.strip_prefix("data: ") {
                    stripped
                } else {
                    continue;
                };

                let chunk: StreamChunk = match serde_json::from_str(json_str) {
                    Ok(c) => c,
                    Err(_) => continue,
                };

                for choice in chunk.choices {
                    if let Some(text) = choice.delta.content {
                        if !text.is_empty() {
                            yield Ok(StreamEvent::Delta(text));
                        }
                    }

                    for tc in choice.delta.tool_calls {
                        let idx = tc.index;
                        while tool_accumulators.len() <= idx {
                            tool_accumulators.push((String::new(), String::new(), String::new()));
                        }
                        if let Some(id) = tc.id {
                            tool_accumulators[idx].0 = id;
                        }
                        if let Some(f) = tc.function {
                            if let Some(name) = f.name {
                                tool_accumulators[idx].1 = name;
                            }
                            if let Some(args) = f.arguments {
                                tool_accumulators[idx].2.push_str(&args);
                            }
                        }
                    }

                    if choice.finish_reason.is_some() {
                        for (id, name, args) in tool_accumulators.drain(..) {
                            if !name.is_empty() {
                                yield Ok(StreamEvent::ToolCall(ToolCall { id, name, arguments: args }));
                            }
                        }
                        let usage = chunk.usage.clone().map(|u| Usage {
                            prompt_tokens: u.prompt_tokens,
                            completion_tokens: u.completion_tokens,
                            total_tokens: u.total_tokens,
                        }).unwrap_or_default();
                        yield Ok(StreamEvent::Done(usage));
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers for building from config
// ---------------------------------------------------------------------------

impl HttpProvider {
    pub fn from_config(
        api_base: Option<&str>,
        api_key: Option<&str>,
        model: Option<&str>,
    ) -> Self {
        Self::new(
            api_base.unwrap_or("https://api.openai.com/v1"),
            api_key.map(String::from),
            model.unwrap_or("gpt-4o-mini"),
        )
    }
}

/// Test helpers — expose internal parsing functions for unit tests.
#[cfg(test)]
pub mod tests {
    use super::*;

    pub fn build_chat_request_test(
        model: &str,
        messages: &[Message],
        tools: &[ToolDef],
        config: &GenerateConfig,
        stream: bool,
    ) -> serde_json::Value {
        build_chat_request(model, messages, tools, config, stream)
    }

    pub fn parse_response_test(raw: serde_json::Value) -> GenerateResponse {
        let resp: ChatCompletionResponse = serde_json::from_value(raw).unwrap();
        parse_chat_response(resp).unwrap()
    }

    pub fn try_parse_response_test(raw: serde_json::Value) -> anyhow::Result<GenerateResponse> {
        let resp: ChatCompletionResponse = serde_json::from_value(raw)?;
        parse_chat_response(resp)
    }
}
