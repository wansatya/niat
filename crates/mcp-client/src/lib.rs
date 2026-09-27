//! NIAT MCP Client
//! Model Context Protocol client engine for stdio-based MCP servers.

use anyhow::Result;
use niat_common::types::{McpConfig, McpServerConfig, ToolRequest, ToolResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tracing::{info, warn};

/// An active MCP server connection over stdio.
struct McpConnection {
    #[allow(dead_code)]
    name: String,
    child: Child,
    #[allow(dead_code)]
    tools: Vec<McpToolDef>,
}

/// Tool definition reported by an MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolDef {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub input_schema: serde_json::Value,
}

/// JSON-RPC message for MCP protocol.
#[derive(Debug, Serialize, Deserialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    id: u64,
    method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize, Deserialize)]
struct JsonRpcError {
    #[allow(dead_code)]
    code: i64,
    message: String,
}

/// MCP client manager — spawns and communicates with MCP servers.
pub struct McpClient {
    connections: HashMap<String, McpConnection>,
    next_id: u64,
}

impl McpClient {
    pub fn new() -> Self {
        Self {
            connections: HashMap::new(),
            next_id: 1,
        }
    }

    /// Initialize MCP servers from config.
    pub async fn init_from_config(&mut self, config: &McpConfig) -> Result<()> {
        for (name, server_config) in &config.mcp_servers {
            match self.spawn_server(name, server_config).await {
                Ok(()) => info!(server = %name, "MCP server started"),
                Err(e) => warn!(server = %name, error = %e, "Failed to start MCP server"),
            }
        }
        Ok(())
    }

    async fn spawn_server(&mut self, name: &str, config: &McpServerConfig) -> Result<()> {
        let mut cmd = Command::new(&config.command);
        cmd.args(&config.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        for (k, v) in &config.env {
            cmd.env(k, v);
        }

        let child = cmd.spawn()?;

        let conn = McpConnection {
            name: name.to_string(),
            child,
            tools: Vec::new(),
        };

        self.connections.insert(name.to_string(), conn);
        Ok(())
    }

    /// List all tools from all connected MCP servers.
    pub fn list_tools(&self) -> Vec<(String, McpToolDef)> {
        let mut tools = Vec::new();
        for (server_name, conn) in &self.connections {
            for tool in &conn.tools {
                tools.push((server_name.clone(), tool.clone()));
            }
        }
        tools
    }

    /// Execute a tool on a specific MCP server.
    pub async fn call_tool(&mut self, server_name: &str, request: &ToolRequest) -> Result<ToolResult> {
        let conn = self.connections.get_mut(server_name)
            .ok_or_else(|| anyhow::anyhow!("MCP server '{}' not found", server_name))?;

        let id = self.next_id;
        self.next_id += 1;

        let rpc_request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id,
            method: "tools/call".to_string(),
            params: Some(serde_json::json!({
                "name": request.name,
                "arguments": request.params,
            })),
        };

        let stdin = conn.child.stdin.as_mut()
            .ok_or_else(|| anyhow::anyhow!("MCP server stdin not available"))?;

        let msg = serde_json::to_string(&rpc_request)? + "\n";
        stdin.write_all(msg.as_bytes()).await?;
        stdin.flush().await?;

        let stdout = conn.child.stdout.as_mut()
            .ok_or_else(|| anyhow::anyhow!("MCP server stdout not available"))?;

        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        reader.read_line(&mut line).await?;

        let response: JsonRpcResponse = serde_json::from_str(&line)?;

        if let Some(error) = response.error {
            Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(error.message),
            })
        } else {
            Ok(ToolResult {
                success: true,
                output: response.result
                    .map(|v| serde_json::to_string_pretty(&v).unwrap_or_default())
                    .unwrap_or_default(),
                error: None,
            })
        }
    }

    /// Shut down all MCP server connections.
    pub async fn shutdown(&mut self) {
        for (name, mut conn) in self.connections.drain() {
            info!(server = %name, "Shutting down MCP server");
            let _ = conn.child.kill().await;
        }
    }
}

impl Default for McpClient {
    fn default() -> Self {
        Self::new()
    }
}
