//! NIAT error types.

use thiserror::Error;

#[derive(Error, Debug)]
pub enum NiatError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Network error: {0}")]
    Network(String),

    #[error("Model provider error: {0}")]
    ModelProvider(String),

    #[error("Tool execution error: {0}")]
    ToolExecution(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("MCP protocol error: {0}")]
    Mcp(String),

    #[error("Installer error: {0}")]
    Installer(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(String),
}
