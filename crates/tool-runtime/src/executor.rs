//! Tool executor with security policy enforcement.

use crate::registry::ToolRegistry;
use niat_common::types::{SafetyLevel, ToolRequest, ToolResult};
use tracing::{info, warn};

pub struct ToolExecutor {
    registry: ToolRegistry,
    require_confirmation: bool,
}

impl ToolExecutor {
    pub fn new(require_confirmation: bool) -> Self {
        Self {
            registry: ToolRegistry::new(),
            require_confirmation,
        }
    }

    /// Check if a tool call needs user confirmation before execution.
    pub fn needs_confirmation(&self, tool_name: &str) -> bool {
        if let Some(tool) = self.registry.get(tool_name) {
            match tool.safety_level() {
                SafetyLevel::Safe => self.require_confirmation,
                SafetyLevel::Confirm | SafetyLevel::Dangerous => true,
            }
        } else {
            true // Unknown tools always require confirmation
        }
    }

    /// Get the safety level for a tool.
    pub fn safety_level(&self, tool_name: &str) -> SafetyLevel {
        self.registry.get(tool_name)
            .map(|t| t.safety_level())
            .unwrap_or(SafetyLevel::Dangerous)
    }

    /// Execute a tool request (caller must have already checked confirmation).
    pub fn execute(&self, request: &ToolRequest) -> ToolResult {
        info!(tool = %request.name, "Executing tool");

        match self.registry.get(&request.name) {
            Some(tool) => {
                match tool.execute(&request.params) {
                    Ok(result) => {
                        if result.success {
                            info!(tool = %request.name, "Tool execution succeeded");
                        } else {
                            warn!(tool = %request.name, error = ?result.error, "Tool execution failed");
                        }
                        result
                    }
                    Err(e) => {
                        warn!(tool = %request.name, error = %e, "Tool execution error");
                        ToolResult {
                            success: false,
                            output: String::new(),
                            error: Some(e.to_string()),
                        }
                    }
                }
            }
            None => ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("Unknown tool: {}", request.name)),
            },
        }
    }

    /// List available tools with their descriptions and safety levels.
    pub fn list_tools(&self) -> Vec<(&str, &str, SafetyLevel)> {
        self.registry.list()
    }
}
