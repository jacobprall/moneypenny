use super::registry::ToolResult;

/// Dispatch a built-in tool call by name.
pub fn dispatch(tool_name: &str, arguments: &str) -> anyhow::Result<ToolResult> {
    match tool_name {
        "file_read" => file_read(arguments),
        "file_write" => file_write(arguments),
        "shell_exec" => shell_exec(arguments),
        "http_request" => http_request(arguments),
        "sql_query" => sql_query(arguments),
        _ => anyhow::bail!("unknown built-in tool: {tool_name}"),
    }
}

fn file_read(arguments: &str) -> anyhow::Result<ToolResult> {
    let args: serde_json::Value = serde_json::from_str(arguments)?;
    let path = args["path"].as_str().ok_or_else(|| anyhow::anyhow!("missing 'path'"))?;

    match std::fs::read_to_string(path) {
        Ok(content) => Ok(ToolResult { output: content, success: true, duration_ms: 0 }),
        Err(e) => Ok(ToolResult { output: format!("Error: {e}"), success: false, duration_ms: 0 }),
    }
}

fn file_write(arguments: &str) -> anyhow::Result<ToolResult> {
    let args: serde_json::Value = serde_json::from_str(arguments)?;
    let path = args["path"].as_str().ok_or_else(|| anyhow::anyhow!("missing 'path'"))?;
    let content = args["content"].as_str().ok_or_else(|| anyhow::anyhow!("missing 'content'"))?;

    match std::fs::write(path, content) {
        Ok(()) => Ok(ToolResult { output: format!("Wrote {} bytes to {path}", content.len()), success: true, duration_ms: 0 }),
        Err(e) => Ok(ToolResult { output: format!("Error: {e}"), success: false, duration_ms: 0 }),
    }
}

fn shell_exec(arguments: &str) -> anyhow::Result<ToolResult> {
    let args: serde_json::Value = serde_json::from_str(arguments)?;
    let command = args["command"].as_str().ok_or_else(|| anyhow::anyhow!("missing 'command'"))?;
    let _timeout_ms = args["timeout_ms"].as_u64().unwrap_or(30_000);

    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg(command)
        .output();

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            let combined = if stderr.is_empty() {
                stdout.to_string()
            } else {
                format!("{stdout}\n--- stderr ---\n{stderr}")
            };
            Ok(ToolResult {
                output: combined,
                success: out.status.success(),
                duration_ms: 0,
            })
        }
        Err(e) => Ok(ToolResult { output: format!("Error: {e}"), success: false, duration_ms: 0 }),
    }
}

fn http_request(_arguments: &str) -> anyhow::Result<ToolResult> {
    Ok(ToolResult {
        output: "HTTP tool requires async runtime; not yet implemented in sync context".into(),
        success: false,
        duration_ms: 0,
    })
}

fn sql_query(_arguments: &str) -> anyhow::Result<ToolResult> {
    Ok(ToolResult {
        output: "SQL tool requires database connection; use execute() with connection context".into(),
        success: false,
        duration_ms: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatch_unknown_tool_fails() {
        let result = dispatch("nonexistent", "{}");
        assert!(result.is_err());
    }

    #[test]
    fn dispatch_routes_to_file_read() {
        let dir = std::env::temp_dir().join("mp_test_builtins");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test_read.txt");
        std::fs::write(&path, "hello world").unwrap();

        let args = format!(r#"{{"path": "{}"}}"#, path.to_string_lossy());
        let result = dispatch("file_read", &args).unwrap();
        assert!(result.success);
        assert_eq!(result.output, "hello world");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn file_read_nonexistent_returns_error() {
        let result = dispatch("file_read", r#"{"path": "/tmp/mp_nonexistent_12345.txt"}"#).unwrap();
        assert!(!result.success);
        assert!(result.output.contains("Error"));
    }

    #[test]
    fn dispatch_routes_to_file_write() {
        let path = std::env::temp_dir().join("mp_test_write.txt");
        let args = format!(r#"{{"path": "{}", "content": "test data"}}"#, path.to_string_lossy());
        let result = dispatch("file_write", &args).unwrap();
        assert!(result.success);
        assert!(result.output.contains("Wrote 9 bytes"));

        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "test data");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn dispatch_routes_to_shell_exec() {
        let result = dispatch("shell_exec", r#"{"command": "echo hello"}"#).unwrap();
        assert!(result.success);
        assert!(result.output.trim().contains("hello"));
    }

    #[test]
    fn shell_exec_captures_failure() {
        let result = dispatch("shell_exec", r#"{"command": "false"}"#).unwrap();
        assert!(!result.success);
    }

    #[test]
    fn file_read_missing_path_arg() {
        let result = dispatch("file_read", r#"{}"#);
        assert!(result.is_err());
    }

    #[test]
    fn file_write_missing_args() {
        assert!(dispatch("file_write", r#"{}"#).is_err());
        assert!(dispatch("file_write", r#"{"path": "/tmp/x"}"#).is_err());
    }

    #[test]
    fn shell_exec_missing_command() {
        assert!(dispatch("shell_exec", r#"{}"#).is_err());
    }
}
