//! NIAT shared types for inter-component communication.

use serde::{Deserialize, Serialize};

/// Safety level for tool operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SafetyLevel {
    Safe,
    Confirm,
    Dangerous,
}

/// A tool invocation request from the Agent Kernel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolRequest {
    pub name: String,
    pub params: serde_json::Value,
}

/// Result of a tool execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub success: bool,
    pub output: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// A message in the chat history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolRequest>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatRole {
    System,
    User,
    Assistant,
    Tool,
}

/// A streaming chunk from the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatChunk {
    pub delta: String,
    pub finish_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call: Option<ToolRequest>,
}

/// System status information displayed in the TUI header.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemStatus {
    pub kernel_version: String,
    pub network_interface: Option<String>,
    pub network_ip: Option<String>,
    pub agent_ready: bool,
    pub model_online: bool,
    pub uptime_secs: u64,
    pub mem_used_mb: u64,
    pub mem_total_mb: u64,
    pub disk_used_gb: u64,
    pub disk_total_gb: u64,
}

/// MCP server configuration entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
}

/// Top-level MCP config file structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpConfig {
    #[serde(rename = "mcpServers", default)]
    pub mcp_servers: std::collections::HashMap<String, McpServerConfig>,
}

impl McpConfig {
    pub fn load(path: &std::path::Path) -> anyhow::Result<Self> {
        if path.exists() {
            let content = std::fs::read_to_string(path)?;
            Ok(serde_json::from_str(&content)?)
        } else {
            Ok(Self {
                mcp_servers: std::collections::HashMap::new(),
            })
        }
    }
}

/// Version constant.
pub const NIAT_VERSION: &str = "0.1.0";
pub const NIAT_NAME: &str = "NIAT: The Intent-Driven Operating System";
