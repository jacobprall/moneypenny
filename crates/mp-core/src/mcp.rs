use rusqlite::{Connection, params};
/// Pure-Rust MCP (Model Context Protocol) stdio client.
///
/// Implements the 2024-11-05 protocol version over a subprocess stdin/stdout
/// pipe.  One `McpClient` wraps one running server process.  The client is
/// synchronous — it writes a JSON-RPC request and reads lines until the
/// matching response arrives, skipping any intervening notifications.
///
/// # Naming convention
///
/// Tools are stored in the `skills` table with a compound name:
/// `{server_name}__{mcp_tool_name}` and a `tool_id` of
/// `mcp:{server_name}:{mcp_tool_name}`.  The full server configuration is
/// serialised into the `content` column so that dispatch can re-connect to
/// the correct server without any global state.
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Stdio};

use crate::config::McpServerConfig;
use crate::tools::registry::ToolResult;

// =========================================================================
// MCP tool descriptor
// =========================================================================

#[derive(Debug, Clone)]
pub struct McpTool {
    /// Tool name as reported by the MCP server.
    pub name: String,
    pub description: String,
    /// JSON Schema for the tool's input parameters (as raw JSON string).
    pub input_schema: String,
}

/// Persisted alongside each MCP tool in the `skills.content` column so that
/// dispatch can reconnect without touching the config.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct StoredMcpDef {
    server_command: String,
    server_args: Vec<String>,
    server_env: std::collections::HashMap<String, String>,
    /// The original tool name on the MCP server (without the namespace prefix).
    mcp_tool_name: String,
    input_schema: serde_json::Value,
}

// =========================================================================
// Client
// =========================================================================

pub struct McpClient {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<std::process::ChildStdout>,
    next_id: u64,
}

impl McpClient {
    /// Spawn an MCP server process and complete the initialize handshake.
    pub fn connect(cfg: &McpServerConfig) -> anyhow::Result<Self> {
        let mut cmd = std::process::Command::new(&cfg.command);
        cmd.args(&cfg.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        for (k, v) in &cfg.env {
            cmd.env(k, v);
        }

        let mut child = cmd
            .spawn()
            .map_err(|e| anyhow::anyhow!("failed to spawn MCP server '{}': {e}", cfg.command))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow::anyhow!("MCP server has no stdin pipe"))?;
        let stdout = BufReader::new(
            child
                .stdout
                .take()
                .ok_or_else(|| anyhow::anyhow!("MCP server has no stdout pipe"))?,
        );

        let mut client = McpClient {
            child,
            stdin,
            stdout,
            next_id: 1,
        };
        client.initialize()?;
        Ok(client)
    }

    // -----------------------------------------------------------------------
    // JSON-RPC transport
    // -----------------------------------------------------------------------

    fn send_request(
        &mut self,
        method: &str,
        params: serde_json::Value,
    ) -> anyhow::Result<serde_json::Value> {
        let id = self.next_id;
        self.next_id += 1;

        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        let line = format!("{}\n", serde_json::to_string(&req)?);
        self.stdin.write_all(line.as_bytes())?;
        self.stdin.flush()?;

        // Read lines until we find the response that carries our request id.
        // Notifications (no "id") are silently discarded.
        loop {
            let mut buf = String::new();
            let n = self.stdout.read_line(&mut buf)?;
            if n == 0 {
                anyhow::bail!("MCP server closed stdout while waiting for response to '{method}'");
            }
            let buf = buf.trim();
            if buf.is_empty() {
                continue;
            }
            let resp: serde_json::Value = serde_json::from_str(buf)
                .map_err(|e| anyhow::anyhow!("MCP server sent non-JSON: {e}\nline: {buf}"))?;

            // Skip notifications (they have a "method" but no "id")
            if resp.get("id").is_none() {
                continue;
            }
            if resp["id"].as_u64() != Some(id) {
                continue; // response for a different in-flight request
            }

            if let Some(err) = resp.get("error") {
                anyhow::bail!("MCP error response for '{method}': {err}");
            }
            return Ok(resp["result"].clone());
        }
    }

    fn send_notification(&mut self, method: &str, params: serde_json::Value) -> anyhow::Result<()> {
        let notif = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        let line = format!("{}\n", serde_json::to_string(&notif)?);
        self.stdin.write_all(line.as_bytes())?;
        self.stdin.flush()?;
        Ok(())
    }

    fn initialize(&mut self) -> anyhow::Result<()> {
        self.send_request(
            "initialize",
            serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": "moneypenny",
                    "version": env!("CARGO_PKG_VERSION"),
                },
            }),
        )?;
        self.send_notification("notifications/initialized", serde_json::json!({}))?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // High-level API
    // -----------------------------------------------------------------------

    /// List all tools advertised by this MCP server.
    pub fn list_tools(&mut self) -> anyhow::Result<Vec<McpTool>> {
        let result = self.send_request("tools/list", serde_json::json!({}))?;
        let tools = result["tools"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("MCP tools/list: 'tools' array missing"))?;

        let mut out = Vec::new();
        for t in tools {
            let name = t["name"].as_str().unwrap_or("").to_string();
            let description = t["description"].as_str().unwrap_or("").to_string();
            let schema = serde_json::to_string(
                &t.get("inputSchema")
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({"type":"object","properties":{}})),
            )?;
            if !name.is_empty() {
                out.push(McpTool {
                    name,
                    description,
                    input_schema: schema,
                });
            }
        }
        Ok(out)
    }

    /// Call a tool on this MCP server and return the text output.
    pub fn call_tool(
        &mut self,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> anyhow::Result<String> {
        let result = self.send_request(
            "tools/call",
            serde_json::json!({
                "name": tool_name,
                "arguments": arguments,
            }),
        )?;

        // MCP content is an array of {type, text} items
        let content = result["content"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| item["text"].as_str())
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_else(|| result.to_string());

        if result["isError"].as_bool() == Some(true) {
            anyhow::bail!("MCP tool '{tool_name}' returned an error: {content}");
        }
        Ok(content)
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

// =========================================================================
// Discovery and registration
// =========================================================================

/// Tool name as stored in `skills`: `{server_name}__{mcp_tool_name}`.
fn registered_name(server_name: &str, mcp_tool_name: &str) -> String {
    format!("{server_name}__{mcp_tool_name}")
}

/// `tool_id` value stored in `skills`: `mcp:{server_name}:{mcp_tool_name}`.
fn tool_id(server_name: &str, mcp_tool_name: &str) -> String {
    format!("mcp:{server_name}:{mcp_tool_name}")
}

/// Connect to each configured MCP server, list its tools, and register them
/// in the agent's `skills` table.  Returns the total number of tools registered.
///
/// Existing entries for the same `tool_id` are replaced (idempotent on restart).
/// Servers that fail to connect are logged as warnings and skipped.
pub fn discover_and_register(
    conn: &Connection,
    servers: &[McpServerConfig],
) -> anyhow::Result<usize> {
    let mut total = 0usize;

    for server in servers {
        let mut client = match McpClient::connect(server) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(server = %server.name, "MCP server connection failed: {e}");
                continue;
            }
        };

        let tools = match client.list_tools() {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!(server = %server.name, "MCP tools/list failed: {e}");
                continue;
            }
        };

        for tool in &tools {
            if let Err(e) = register_tool(conn, server, tool) {
                tracing::warn!(
                    server = %server.name, tool = %tool.name,
                    "failed to register MCP tool: {e}"
                );
                continue;
            }
            total += 1;
        }

        tracing::info!(server = %server.name, count = tools.len(), "MCP tools discovered");
    }

    Ok(total)
}

fn register_tool(
    conn: &Connection,
    server: &McpServerConfig,
    tool: &McpTool,
) -> anyhow::Result<()> {
    let def = StoredMcpDef {
        server_command: server.command.clone(),
        server_args: server.args.clone(),
        server_env: server.env.clone(),
        mcp_tool_name: tool.name.clone(),
        input_schema: serde_json::from_str(&tool.input_schema).unwrap_or(serde_json::json!({})),
    };
    let content = serde_json::to_string(&def)?;
    let name = registered_name(&server.name, &tool.name);
    let tid = tool_id(&server.name, &tool.name);
    let now = chrono::Utc::now().timestamp();

    conn.execute(
        "INSERT OR REPLACE INTO skills
         (id, name, description, content, tool_id, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            uuid::Uuid::new_v4().to_string(),
            name,
            tool.description,
            content,
            tid,
            now,
            now,
        ],
    )?;
    Ok(())
}

// =========================================================================
// Dispatch
// =========================================================================

/// Returns true if the skill with this name was registered by an MCP server.
pub fn is_mcp_tool(conn: &Connection, tool_name: &str) -> bool {
    conn.query_row(
        "SELECT tool_id FROM skills WHERE name = ?1",
        [tool_name],
        |r| r.get::<_, Option<String>>(0),
    )
    .ok()
    .flatten()
    .map(|tid| tid.starts_with("mcp:"))
    .unwrap_or(false)
}

/// Call a registered MCP tool.  Looks up the stored `StoredMcpDef`, spawns
/// a fresh server process, calls the tool, and returns the result.
pub fn dispatch(conn: &Connection, tool_name: &str, arguments: &str) -> anyhow::Result<ToolResult> {
    let start = std::time::Instant::now();

    let content: String = conn
        .query_row(
            "SELECT content FROM skills WHERE name = ?1",
            [tool_name],
            |r| r.get(0),
        )
        .map_err(|_| anyhow::anyhow!("MCP tool '{tool_name}' not found in skills"))?;

    let def: StoredMcpDef = serde_json::from_str(&content)
        .map_err(|e| anyhow::anyhow!("corrupt MCP tool definition for '{tool_name}': {e}"))?;

    let cfg = McpServerConfig {
        name: String::new(), // not needed for dispatch
        command: def.server_command,
        args: def.server_args,
        env: def.server_env,
    };

    let mut client = McpClient::connect(&cfg)?;
    let args: serde_json::Value = serde_json::from_str(arguments).unwrap_or(serde_json::json!({}));
    let output = client.call_tool(&def.mcp_tool_name, args)?;
    let duration_ms = start.elapsed().as_millis() as u64;

    Ok(ToolResult {
        output,
        success: true,
        duration_ms,
    })
}

/// Load all registered MCP tools as `mp_llm::types::ToolDef` for passing to
/// the LLM.  Called during `agent_turn` to expose dynamically-discovered tools.
pub fn load_tool_defs(conn: &Connection) -> Vec<(String, String, serde_json::Value)> {
    let mut stmt = match conn
        .prepare("SELECT name, description, content FROM skills WHERE tool_id LIKE 'mcp:%'")
    {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })
    .ok()
    .map(|rows| {
        rows.flatten()
            .filter_map(|(name, desc, content)| {
                let def: StoredMcpDef = serde_json::from_str(&content).ok()?;
                Some((name, desc, def.input_schema))
            })
            .collect()
    })
    .unwrap_or_default()
}

// =========================================================================
// Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db, schema};

    fn setup() -> Connection {
        let conn = db::open_memory().unwrap();
        schema::init_agent_db(&conn).unwrap();
        conn
    }

    #[test]
    fn registered_name_format() {
        assert_eq!(
            registered_name("filesystem", "read_file"),
            "filesystem__read_file"
        );
    }

    #[test]
    fn tool_id_format() {
        assert_eq!(
            tool_id("filesystem", "read_file"),
            "mcp:filesystem:read_file"
        );
    }

    #[test]
    fn register_and_detect_mcp_tool() {
        let conn = setup();
        let server = McpServerConfig {
            name: "test_server".into(),
            command: "echo".into(),
            args: vec![],
            env: Default::default(),
        };
        let tool = McpTool {
            name: "greet".into(),
            description: "Greet someone".into(),
            input_schema: r#"{"type":"object","properties":{"name":{"type":"string"}}}"#.into(),
        };

        register_tool(&conn, &server, &tool).unwrap();

        assert!(is_mcp_tool(&conn, "test_server__greet"));
        assert!(!is_mcp_tool(&conn, "file_read")); // builtin, not mcp
    }

    #[test]
    fn load_tool_defs_returns_registered_tools() {
        let conn = setup();
        let server = McpServerConfig {
            name: "srv".into(),
            command: "true".into(),
            args: vec![],
            env: Default::default(),
        };
        let tool = McpTool {
            name: "ping".into(),
            description: "Ping the server".into(),
            input_schema: r#"{"type":"object","properties":{}}"#.into(),
        };

        register_tool(&conn, &server, &tool).unwrap();

        let defs = load_tool_defs(&conn);
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].0, "srv__ping");
        assert_eq!(defs[0].1, "Ping the server");
    }

    #[test]
    fn discover_and_register_skips_unreachable_servers() {
        let conn = setup();
        let servers = vec![McpServerConfig {
            name: "nonexistent".into(),
            command: "this-binary-does-not-exist-xyz".into(),
            args: vec![],
            env: Default::default(),
        }];

        // Should not panic; unreachable servers are skipped
        let count = discover_and_register(&conn, &servers).unwrap();
        assert_eq!(count, 0);
    }
}
