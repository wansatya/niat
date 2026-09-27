//! Agent kernel client — connects to agent-kernel over Unix socket.

use crate::app::PendingConfirmation;
use anyhow::Result;
use niat_common::types::{SafetyLevel, SystemStatus};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

const SOCKET_PATH: &str = "/var/run/niat/agent.sock";

pub enum ParsedResponse {
    Text(String),
    ToolResult { name: String, output: String, success: bool },
    Confirm(PendingConfirmation),
    Status(SystemStatus),
    ConfigUpdated {
        base_url: String,
        model: String,
        has_key: bool,
        online: bool,
    },
    Error(String),
}

pub struct AgentClient {
    reader: BufReader<tokio::net::unix::OwnedReadHalf>,
    writer: tokio::net::unix::OwnedWriteHalf,
}

impl AgentClient {
    pub async fn connect() -> Result<Self> {
        let stream = UnixStream::connect(SOCKET_PATH).await?;
        let (read_half, write_half) = stream.into_split();
        Ok(Self {
            reader: BufReader::new(read_half),
            writer: write_half,
        })
    }

    pub async fn send_intent(&mut self, text: &str) -> Result<()> {
        let msg = serde_json::json!({
            "type": "intent",
            "text": text
        });
        let line = serde_json::to_string(&msg)? + "\n";
        self.writer.write_all(line.as_bytes()).await?;
        self.writer.flush().await?;
        Ok(())
    }

    pub async fn send_approve(&mut self, request_id: &str) -> Result<()> {
        let msg = serde_json::json!({
            "type": "approve",
            "request_id": request_id
        });
        let line = serde_json::to_string(&msg)? + "\n";
        self.writer.write_all(line.as_bytes()).await?;
        self.writer.flush().await?;
        Ok(())
    }

    pub async fn send_deny(&mut self, request_id: &str) -> Result<()> {
        let msg = serde_json::json!({
            "type": "deny",
            "request_id": request_id
        });
        let line = serde_json::to_string(&msg)? + "\n";
        self.writer.write_all(line.as_bytes()).await?;
        self.writer.flush().await?;
        Ok(())
    }

    pub async fn send_update_config(
        &mut self,
        base_url: Option<String>,
        model: Option<String>,
        api_key: Option<String>,
    ) -> Result<()> {
        let msg = serde_json::json!({
            "type": "update_config",
            "base_url": base_url,
            "model": model,
            "api_key": api_key,
        });
        let line = serde_json::to_string(&msg)? + "\n";
        self.writer.write_all(line.as_bytes()).await?;
        self.writer.flush().await?;
        Ok(())
    }

    pub async fn send_reboot(&mut self) -> Result<()> {
        let msg = serde_json::json!({ "type": "reboot" });
        let line = serde_json::to_string(&msg)? + "\n";
        self.writer.write_all(line.as_bytes()).await?;
        self.writer.flush().await?;
        Ok(())
    }

    pub async fn send_status_request(&mut self) -> Result<()> {
        let msg = serde_json::json!({ "type": "status" });
        let line = serde_json::to_string(&msg)? + "\n";
        self.writer.write_all(line.as_bytes()).await?;
        self.writer.flush().await?;
        Ok(())
    }

    /// Non-blocking receive — returns None if no data available.
    pub async fn try_recv(&mut self) -> Option<ParsedResponse> {
        let mut line = String::new();

        // Use tokio timeout for non-blocking check
        match tokio::time::timeout(
            std::time::Duration::from_millis(10),
            self.reader.read_line(&mut line),
        ).await {
            Ok(Ok(0)) => None, // Connection closed
            Ok(Ok(_)) => self.parse_response(&line),
            Ok(Err(_)) => None,
            Err(_) => None, // Timeout — no data
        }
    }

    fn parse_response(&self, line: &str) -> Option<ParsedResponse> {
        let v: serde_json::Value = serde_json::from_str(line.trim()).ok()?;

        match v["type"].as_str()? {
            "text" => Some(ParsedResponse::Text(
                v["content"].as_str().unwrap_or("").to_string(),
            )),
            "delta" => Some(ParsedResponse::Text(
                v["content"].as_str().unwrap_or("").to_string(),
            )),
            "tool_result" => Some(ParsedResponse::ToolResult {
                name: v["tool_name"].as_str().unwrap_or("").to_string(),
                output: v["result"]["output"].as_str().unwrap_or("").to_string(),
                success: v["result"]["success"].as_bool().unwrap_or(false),
            }),
            "confirm" => {
                let safety = match v["safety_level"].as_str().unwrap_or("CONFIRM") {
                    "SAFE" => SafetyLevel::Safe,
                    "DANGEROUS" => SafetyLevel::Dangerous,
                    _ => SafetyLevel::Confirm,
                };
                Some(ParsedResponse::Confirm(PendingConfirmation {
                    request_id: v["request_id"].as_str().unwrap_or("").to_string(),
                    tool_name: v["tool_name"].as_str().unwrap_or("").to_string(),
                    description: v["description"].as_str().unwrap_or("").to_string(),
                    params: v["params"].clone(),
                    safety_level: safety,
                }))
            }
            "status" => {
                let status: SystemStatus = serde_json::from_value(v.clone()).ok()?;
                Some(ParsedResponse::Status(status))
            }
            "config_updated" => Some(ParsedResponse::ConfigUpdated {
                base_url: v["base_url"].as_str().unwrap_or("").to_string(),
                model: v["model"].as_str().unwrap_or("").to_string(),
                has_key: v["has_key"].as_bool().unwrap_or(false),
                online: v["online"].as_bool().unwrap_or(false),
            }),
            "error" => Some(ParsedResponse::Error(
                v["message"].as_str().unwrap_or("Unknown error").to_string(),
            )),
            _ => None,
        }
    }
}
