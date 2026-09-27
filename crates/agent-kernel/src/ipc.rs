//! IPC message types between agent-kernel and niat-tui.

use niat_common::types::{SafetyLevel, SystemStatus, ToolResult};
use serde::{Deserialize, Serialize};

/// Request from TUI to Agent Kernel.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TuiRequest {
    /// User intent / chat message.
    #[serde(rename = "intent")]
    Intent { text: String },

    /// User approved a pending tool call.
    #[serde(rename = "approve")]
    Approve { request_id: String },

    /// User denied a pending tool call.
    #[serde(rename = "deny")]
    Deny { request_id: String },

    /// Request current system status.
    #[serde(rename = "status")]
    Status,

    /// Update model configuration (base_url, model, api_key).
    #[serde(rename = "update_config")]
    UpdateConfig {
        base_url: Option<String>,
        model: Option<String>,
        api_key: Option<String>,
    },

    /// Graceful shutdown.
    #[serde(rename = "shutdown")]
    Shutdown,

    /// System reboot.
    #[serde(rename = "reboot")]
    Reboot,
}

/// Response from Agent Kernel to TUI.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AgentResponse {
    /// Text response from the agent.
    #[serde(rename = "text")]
    Text { content: String },

    /// Streaming text delta.
    #[serde(rename = "delta")]
    Delta { content: String },

    /// Tool call requiring user approval.
    #[serde(rename = "confirm")]
    ConfirmTool {
        request_id: String,
        tool_name: String,
        description: String,
        params: serde_json::Value,
        safety_level: SafetyLevel,
    },

    /// Tool execution result.
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_name: String,
        result: ToolResult,
    },

    /// System status update.
    #[serde(rename = "status")]
    Status(SystemStatus),

    /// Model / System configuration updated confirmation.
    #[serde(rename = "config_updated")]
    ConfigUpdated {
        base_url: String,
        model: String,
        has_key: bool,
        online: bool,
    },

    /// Error message.
    #[serde(rename = "error")]
    Error { message: String },
}
