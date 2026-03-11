pub fn parse_duration_hours(s: &str) -> i64 {
    if let Some(d) = s.strip_suffix('d') {
        d.parse::<i64>().unwrap_or(7) * 24
    } else if let Some(h) = s.strip_suffix('h') {
        h.parse::<i64>().unwrap_or(24)
    } else {
        168
    }
}

pub fn normalize_embedding_target(value: &str) -> Option<&'static str> {
    match value.trim().to_ascii_lowercase().as_str() {
        "facts" | "fact" => Some("facts"),
        "messages" | "message" | "msg" => Some("messages"),
        "tool_calls" | "tool-calls" | "toolcalls" | "tool_call" => Some("tool_calls"),
        "policy_audit" | "policy-audit" | "policyaudit" | "policy" => Some("policy_audit"),
        "chunks" | "chunk" | "knowledge" => Some("chunks"),
        _ => None,
    }
}

pub fn toml_to_json(v: &toml::Value) -> serde_json::Value {
    match v {
        toml::Value::String(s) => serde_json::Value::String(s.clone()),
        toml::Value::Integer(i) => serde_json::json!(i),
        toml::Value::Float(f) => serde_json::json!(f),
        toml::Value::Boolean(b) => serde_json::json!(b),
        toml::Value::Datetime(d) => serde_json::Value::String(d.to_string()),
        toml::Value::Array(a) => serde_json::Value::Array(a.iter().map(toml_to_json).collect()),
        toml::Value::Table(t) => {
            let mut map = serde_json::Map::new();
            for (k, val) in t {
                map.insert(k.clone(), toml_to_json(val));
            }
            serde_json::Value::Object(map)
        }
    }
}

pub fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

pub fn sql_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

pub fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max { s } else { &s[..max] }
}

pub fn default_model_url(model_name: &str) -> Option<&'static str> {
    match model_name {
        "nomic-embed-text-v1.5" => Some(
            "https://huggingface.co/nomic-ai/nomic-embed-text-v1.5-GGUF/resolve/main/nomic-embed-text-v1.5.Q4_K_M.gguf",
        ),
        _ => None,
    }
}
