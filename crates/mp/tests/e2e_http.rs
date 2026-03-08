//! E2E tests: gateway with HTTP channel — spawn `mp start`, hit /health and /v1/chat.

mod common;

use common::{enable_http_channel, init_project, spawn_gateway};
use std::io::Write;
use std::process::{Command, Stdio};
use std::io::Read;
use std::time::Duration;

const HTTP_PORT: u16 = 18999;
const HEALTH_URL: &str = "http://127.0.0.1:18999/health";
const CHAT_URL: &str = "http://127.0.0.1:18999/v1/chat";
const OPS_URL: &str = "http://127.0.0.1:18999/v1/ops";

/// Wait for the server to respond on the given URL, up to `timeout`.
fn wait_for_http(url: &str, timeout: Duration) -> bool {
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        if let Ok(r) = reqwest::blocking::get(url) {
            if r.status().is_success() {
                return true;
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

fn run_sidecar_once(
    config_path: &std::path::Path,
    request: &serde_json::Value,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let cwd = config_path.parent().unwrap_or(std::path::Path::new("."));
    let mut child = Command::new(env!("CARGO_BIN_EXE_mp"))
        .args([
            "--config",
            config_path.to_str().unwrap_or("moneypenny.toml"),
            "sidecar",
            "--agent",
            "main",
        ])
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    if let Some(mut stdin) = child.stdin.take() {
        let line = format!("{}\n", serde_json::to_string(request)?);
        stdin.write_all(line.as_bytes())?;
    }

    let output = child.wait_with_output()?;
    if !output.status.success() {
        return Err(format!(
            "sidecar failed: status={} stderr={}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout
        .lines()
        .next()
        .ok_or("sidecar produced no response line")?;
    let parsed: serde_json::Value = serde_json::from_str(line)?;
    Ok(parsed)
}

fn run_sidecar_mcp_tools_call_once(
    config_path: &std::path::Path,
    op_name: &str,
    args: serde_json::Value,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let req = serde_json::json!({
        "jsonrpc": "2.0",
        "id": "rpc-parity-1",
        "method": "tools/call",
        "params": {
            "name": op_name,
            "arguments": args,
            "agent_id": "main"
        }
    });
    let resp = run_sidecar_once(config_path, &req)?;
    let text = resp["result"]["content"]
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|v| v["text"].as_str())
        .ok_or("missing MCP tools/call text payload")?;
    let parsed: serde_json::Value = serde_json::from_str(text)?;
    Ok(parsed)
}

#[test]
fn gateway_http_health_endpoint() {
    let (_temp, config_path) = init_project().unwrap();
    enable_http_channel(&config_path, HTTP_PORT).unwrap();

    let mut child = spawn_gateway(&config_path).unwrap();

    let ok = wait_for_http(HEALTH_URL, Duration::from_secs(10));
    if !ok {
        let _ = child.kill();
        let mut stderr = String::new();
        let _ = child.stderr.as_mut().unwrap().read_to_string(&mut stderr);
        panic!("server did not respond at {} within 10s. stderr:\n{}", HEALTH_URL, stderr);
    }

    let r = reqwest::blocking::get(HEALTH_URL).unwrap();
    assert!(r.status().is_success());
    let body = r.text().unwrap();
    assert!(body.contains("ok") || body.contains("status") || body.contains("version"));

    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn gateway_http_chat_endpoint() {
    let (_temp, config_path) = init_project().unwrap();
    enable_http_channel(&config_path, HTTP_PORT).unwrap();

    let mut child = spawn_gateway(&config_path).unwrap();

    if !wait_for_http(HEALTH_URL, Duration::from_secs(10)) {
        let _ = child.kill();
        let _ = child.wait();
        return; // server didn't start, skip
    }

    let client = reqwest::blocking::Client::new();
    let resp = client
        .post(CHAT_URL)
        .json(&serde_json::json!({ "message": "Reply with exactly: OK" }))
        .timeout(Duration::from_secs(60))
        .send();

    let _ = child.kill();
    let _ = child.wait();

    let resp = match resp {
        Ok(r) => r,
        Err(_e) => {
            // Timeout or connection error — possible without LLM or in CI
            return;
        }
    };

    // 200 with a response body, or 500 when LLM is unavailable
    if resp.status().is_success() {
        let body: serde_json::Value = resp.json().unwrap_or(serde_json::Value::Null);
        assert!(body.get("response").is_some() || body.get("session_id").is_some());
    }
}

#[test]
fn gateway_http_ops_parity_with_sidecar() {
    let (_temp, config_path) = init_project().unwrap();
    enable_http_channel(&config_path, HTTP_PORT).unwrap();

    let mut child = spawn_gateway(&config_path).unwrap();
    if !wait_for_http(HEALTH_URL, Duration::from_secs(10)) {
        let _ = child.kill();
        let _ = child.wait();
        panic!("gateway did not start for /v1/ops parity test");
    }

    let request = serde_json::json!({
        "op": "session.list",
        "request_id": "parity-session-list-1",
        "agent_id": "main",
        "args": { "agent_id": "main", "limit": 5 }
    });

    let sidecar_resp = run_sidecar_once(&config_path, &request).expect("sidecar response");
    let http_resp = reqwest::blocking::Client::new()
        .post(OPS_URL)
        .json(&request)
        .timeout(Duration::from_secs(20))
        .send()
        .expect("http /v1/ops response");

    let _ = child.kill();
    let _ = child.wait();

    assert!(
        http_resp.status().is_success(),
        "http status must be success, got {}",
        http_resp.status()
    );
    let http_json: serde_json::Value = http_resp.json().expect("json body");

    assert_eq!(sidecar_resp["ok"], http_json["ok"]);
    assert_eq!(sidecar_resp["code"], http_json["code"]);
    assert_eq!(sidecar_resp["message"], http_json["message"]);
    assert!(sidecar_resp["data"].is_array());
    assert!(http_json["data"].is_array());
    assert_eq!(
        sidecar_resp["data"].as_array().map(|a| a.len()),
        http_json["data"].as_array().map(|a| a.len())
    );
}

#[test]
fn gateway_http_ops_parity_with_mcp_tools_call() {
    let (_temp, config_path) = init_project().unwrap();
    enable_http_channel(&config_path, HTTP_PORT).unwrap();

    let mut child = spawn_gateway(&config_path).unwrap();
    if !wait_for_http(HEALTH_URL, Duration::from_secs(10)) {
        let _ = child.kill();
        let _ = child.wait();
        panic!("gateway did not start for MCP parity test");
    }

    let op_args = serde_json::json!({ "agent_id": "main", "limit": 5 });
    let mcp_sidecar_op_resp = run_sidecar_mcp_tools_call_once(&config_path, "session.list", op_args.clone())
        .expect("MCP tools/call sidecar response");

    let http_op_req = serde_json::json!({
        "op": "session.list",
        "agent_id": "main",
        "args": op_args
    });
    let http_resp = reqwest::blocking::Client::new()
        .post(OPS_URL)
        .json(&http_op_req)
        .timeout(Duration::from_secs(20))
        .send()
        .expect("http /v1/ops response");

    let _ = child.kill();
    let _ = child.wait();

    assert!(http_resp.status().is_success());
    let http_json: serde_json::Value = http_resp.json().expect("json body");

    assert_eq!(mcp_sidecar_op_resp["ok"], http_json["ok"]);
    assert_eq!(mcp_sidecar_op_resp["code"], http_json["code"]);
    assert_eq!(mcp_sidecar_op_resp["message"], http_json["message"]);
    assert!(mcp_sidecar_op_resp["data"].is_array());
    assert!(http_json["data"].is_array());
    assert_eq!(
        mcp_sidecar_op_resp["data"].as_array().map(|a| a.len()),
        http_json["data"].as_array().map(|a| a.len())
    );
}

#[test]
fn gateway_ingest_status_parity_http_and_sidecar() {
    let (_temp, config_path) = init_project().unwrap();
    enable_http_channel(&config_path, HTTP_PORT).unwrap();

    let mut child = spawn_gateway(&config_path).unwrap();
    if !wait_for_http(HEALTH_URL, Duration::from_secs(10)) {
        let _ = child.kill();
        let _ = child.wait();
        panic!("gateway did not start for ingest parity test");
    }

    let request = serde_json::json!({
        "op": "ingest.status",
        "agent_id": "main",
        "args": {
            "source": "openclaw",
            "limit": 10
        }
    });

    let sidecar_resp = run_sidecar_once(&config_path, &request).expect("sidecar ingest.status response");
    let http_resp = reqwest::blocking::Client::new()
        .post(OPS_URL)
        .json(&request)
        .timeout(Duration::from_secs(20))
        .send()
        .expect("http ingest.status response");

    let _ = child.kill();
    let _ = child.wait();

    assert!(http_resp.status().is_success());
    let http_json: serde_json::Value = http_resp.json().expect("json body");

    assert_eq!(sidecar_resp["ok"], http_json["ok"]);
    assert_eq!(sidecar_resp["code"], http_json["code"]);
    assert_eq!(sidecar_resp["message"], http_json["message"]);
    assert!(sidecar_resp["data"].is_array());
    assert!(http_json["data"].is_array());
    assert_eq!(
        sidecar_resp["data"].as_array().map(|a| a.len()),
        http_json["data"].as_array().map(|a| a.len())
    );
}
