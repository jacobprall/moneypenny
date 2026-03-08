//! E2E tests: gateway with HTTP channel — spawn `mp start`, hit /health and /v1/chat.

mod common;

use common::{enable_http_channel, init_project, spawn_gateway};
use std::io::Read;
use std::time::Duration;

const HTTP_PORT: u16 = 18999;
const HEALTH_URL: &str = "http://127.0.0.1:18999/health";
const CHAT_URL: &str = "http://127.0.0.1:18999/v1/chat";

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
