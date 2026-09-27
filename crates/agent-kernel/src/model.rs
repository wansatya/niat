//! LLM model client — OpenAI-compatible API streaming.

use anyhow::Result;
use niat_common::config::ModelConfig;
use niat_common::types::{ChatMessage, ChatRole};
use reqwest::Client;
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct ModelClient {
    client: Client,
    base_url: String,
    model: String,
    api_key: Option<String>,
    #[allow(dead_code)]
    timeout_secs: u64,
    max_tokens: u32,
}

#[derive(Serialize)]
struct ApiRequest {
    model: String,
    messages: Vec<ApiMessage>,
    max_tokens: u32,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<serde_json::Value>>,
}

#[derive(Serialize, Deserialize)]
struct ApiMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ApiResponse {
    choices: Vec<ApiChoice>,
}

#[derive(Deserialize)]
struct ApiChoice {
    message: Option<ApiResponseMessage>,
    #[allow(dead_code)]
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct ApiResponseMessage {
    content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<ApiToolCall>,
}

#[derive(Deserialize)]
struct ApiToolCall {
    function: ApiFunction,
}

#[derive(Deserialize)]
struct ApiFunction {
    name: String,
    arguments: String,
}

impl ModelClient {
    pub fn new(config: &ModelConfig) -> Self {
        let api_key = config
            .api_key
            .as_ref()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .or_else(|| std::env::var(&config.api_key_env).ok());
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_seconds))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            base_url: config.base_url.trim_end_matches('/').to_string(),
            model: config.model.clone(),
            api_key,
            timeout_secs: config.timeout_seconds,
            max_tokens: config.max_tokens,
        }
    }

    /// Check if the model provider is reachable.
    pub async fn health_check(&self) -> bool {
        let url = format!("{}/models", self.base_url);
        let mut req = self.client.get(&url);
        if let Some(key) = &self.api_key {
            req = req.bearer_auth(key);
        }
        req.send().await.map(|r| r.status().is_success()).unwrap_or(false)
    }

    /// Send a non-streaming chat completion request.
    pub async fn chat(&self, messages: &[ChatMessage], tools_json: Option<Vec<serde_json::Value>>) -> Result<(String, Vec<niat_common::types::ToolRequest>)> {
        let url = format!("{}/chat/completions", self.base_url);

        let api_messages: Vec<ApiMessage> = messages.iter().map(|m| ApiMessage {
            role: match m.role {
                ChatRole::System => "system".into(),
                ChatRole::User => "user".into(),
                ChatRole::Assistant => "assistant".into(),
                ChatRole::Tool => "tool".into(),
            },
            content: m.content.clone(),
        }).collect();

        let body = ApiRequest {
            model: self.model.clone(),
            messages: api_messages,
            max_tokens: self.max_tokens,
            stream: false,
            tools: tools_json,
        };

        let mut req = self.client.post(&url).json(&body);
        if let Some(key) = &self.api_key {
            req = req.bearer_auth(key);
        }

        let resp = req.send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Model API error {}: {}", status, body);
        }

        let api_resp: ApiResponse = resp.json().await?;

        let choice = api_resp.choices.into_iter().next()
            .ok_or_else(|| anyhow::anyhow!("No response from model"))?;

        let msg = choice.message.unwrap_or(ApiResponseMessage {
            content: None,
            tool_calls: vec![],
        });

        let content = msg.content.unwrap_or_default();
        let tool_requests: Vec<niat_common::types::ToolRequest> = msg.tool_calls.into_iter()
            .filter_map(|tc| {
                let params: serde_json::Value = serde_json::from_str(&tc.function.arguments).ok()?;
                Some(niat_common::types::ToolRequest {
                    name: tc.function.name,
                    params,
                })
            })
            .collect();

        Ok((content, tool_requests))
    }
}
