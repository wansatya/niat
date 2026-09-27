//! Intent planner — routes user intent through model → tools → response.

use crate::ipc::{AgentResponse, TuiRequest};
use crate::model::ModelClient;
use niat_common::config::NiatConfig;
use niat_common::types::{ChatMessage, ChatRole};
use std::sync::Arc;
use tokio::sync::RwLock;
use tool_runtime::ToolExecutor;
use tracing::{info, warn};
use uuid::Uuid;

struct PlannerState {
    config: NiatConfig,
    model: ModelClient,
    tool_executor: Arc<ToolExecutor>,
    system_prompt: String,
}

#[derive(Clone)]
pub struct Planner {
    state: Arc<RwLock<PlannerState>>,
}

impl Planner {
    pub fn new(config: NiatConfig, model: ModelClient, tool_executor: ToolExecutor) -> Self {
        let system_prompt = config.agent.system_prompt.clone();
        Self {
            state: Arc::new(RwLock::new(PlannerState {
                config,
                model,
                tool_executor: Arc::new(tool_executor),
                system_prompt,
            })),
        }
    }

    /// Handle a request from the TUI.
    pub async fn handle_request(&self, request: TuiRequest) -> AgentResponse {
        match request {
            TuiRequest::Intent { text } => self.handle_intent(&text).await,
            TuiRequest::Approve { request_id } => {
                AgentResponse::Text {
                    content: format!("Approved request {}", request_id),
                }
            }
            TuiRequest::Deny { request_id } => {
                AgentResponse::Text {
                    content: format!("Denied request {}", request_id),
                }
            }
            TuiRequest::Status => {
                let sys_mgr = system_manager::SystemManager::new();
                let mut status = sys_mgr.status();
                status.agent_ready = true;
                let state = self.state.read().await;
                status.model_online = state.model.health_check().await;
                AgentResponse::Status(status)
            }
            TuiRequest::UpdateConfig { base_url, model, api_key } => {
                let mut state = self.state.write().await;
                if let Some(url) = base_url {
                    if !url.trim().is_empty() {
                        state.config.model.base_url = url.trim().to_string();
                    }
                }
                if let Some(m) = model {
                    if !m.trim().is_empty() {
                        state.config.model.model = m.trim().to_string();
                    }
                }
                if let Some(key) = api_key {
                    let trimmed = key.trim();
                    if trimmed.is_empty() {
                        state.config.model.api_key = None;
                    } else {
                        state.config.model.api_key = Some(trimmed.to_string());
                    }
                }

                // Persist to the first writable location; never fail the update.
                if let Err(e) = state.config.save_persistent(None) {
                    tracing::warn!("Failed to persist updated config: {}", e);
                }

                // Reinitialize model client
                state.model = ModelClient::new(&state.config.model);
                let online = state.model.health_check().await;

                info!(
                    model = %state.config.model.model,
                    base_url = %state.config.model.base_url,
                    has_key = state.config.model.api_key.is_some(),
                    online = online,
                    "Updated model configuration"
                );

                AgentResponse::ConfigUpdated {
                    base_url: state.config.model.base_url.clone(),
                    model: state.config.model.model.clone(),
                    has_key: state.config.model.api_key.is_some(),
                    online,
                }
            }
            TuiRequest::Shutdown => {
                info!("Shutdown requested by TUI");
                AgentResponse::Text {
                    content: "Agent kernel shutting down...".into(),
                }
            }
            TuiRequest::Reboot => {
                info!("System reboot requested by TUI");
                tokio::spawn(async {
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                    let _ = std::process::Command::new("/sbin/reboot").spawn();
                    let _ = std::process::Command::new("reboot").spawn();
                });
                AgentResponse::Text {
                    content: "Initiating system reboot...".into(),
                }
            }
        }
    }

    async fn handle_intent(&self, user_text: &str) -> AgentResponse {
        info!(intent = %user_text, "Processing user intent");

        let (model, tool_executor, system_prompt) = {
            let state = self.state.read().await;
            (state.model.clone(), state.tool_executor.clone(), state.system_prompt.clone())
        };

        // Build message history
        let messages = vec![
            ChatMessage {
                role: ChatRole::System,
                content: system_prompt,
                tool_calls: None,
            },
            ChatMessage {
                role: ChatRole::User,
                content: user_text.to_string(),
                tool_calls: None,
            },
        ];

        // Build tool definitions for the model
        let tools_json = self.build_tools_json(&tool_executor);

        // Call the model
        match model.chat(&messages, Some(tools_json)).await {
            Ok((content, tool_requests)) => {
                if !tool_requests.is_empty() {
                    let first = &tool_requests[0];
                    let safety = tool_executor.safety_level(&first.name);

                    if tool_executor.needs_confirmation(&first.name) {
                        let request_id = Uuid::new_v4().to_string();
                        AgentResponse::ConfirmTool {
                            request_id,
                            tool_name: first.name.clone(),
                            description: content.clone(),
                            params: first.params.clone(),
                            safety_level: safety,
                        }
                    } else {
                        let result = tool_executor.execute(first);
                        AgentResponse::ToolResult {
                            tool_name: first.name.clone(),
                            result,
                        }
                    }
                } else {
                    AgentResponse::Text { content }
                }
            }
            Err(e) => {
                warn!(error = %e, "Model request failed");
                AgentResponse::Error {
                    message: format!("Model error: {}", e),
                }
            }
        }
    }

    fn build_tools_json(&self, tool_executor: &ToolExecutor) -> Vec<serde_json::Value> {
        tool_executor.list_tools().iter().map(|(name, desc, _safety)| {
            serde_json::json!({
                "type": "function",
                "function": {
                    "name": name,
                    "description": desc,
                    "parameters": {
                        "type": "object",
                        "properties": {}
                    }
                }
            })
        }).collect()
    }
}
