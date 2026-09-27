//! TUI application state.

use crate::agent_client::AgentClient;
use niat_common::config::NiatConfig;
use niat_common::types::{SafetyLevel, SystemStatus, NIAT_VERSION};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Editing,
    ConfigModal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigField {
    BaseUrl,
    Model,
    ApiKey,
    SaveButton,
    CancelButton,
}

pub struct ConfigModalState {
    pub base_url: String,
    pub model: String,
    pub api_key: String,
    pub active_field: ConfigField,
    pub show_api_key: bool,
}

pub struct Message {
    pub sender: MessageSender,
    pub content: String,
    pub timestamp: String,
}

pub enum MessageSender {
    User,
    Agent,
    System,
    Success,
    Error,
    Tool(String),
}

pub struct PendingConfirmation {
    pub request_id: String,
    pub tool_name: String,
    pub description: String,
    pub params: serde_json::Value,
    pub safety_level: SafetyLevel,
}

pub struct App {
    pub input: String,
    pub input_mode: InputMode,
    pub messages: Vec<Message>,
    pub status: Option<SystemStatus>,
    pub config: NiatConfig,
    pub config_modal: Option<ConfigModalState>,
    pub pending_confirm: Option<PendingConfirmation>,
    pub agent_client: Option<AgentClient>,
    pub scroll_offset: u16,
    pub connected: bool,
    pub should_quit: bool,
    pub should_reboot: bool,
}

impl App {
    pub fn new() -> Self {
        let config = NiatConfig::load(NiatConfig::default_path())
            .or_else(|_| NiatConfig::load(std::path::Path::new("/tmp/niat_config.toml")))
            .unwrap_or_default();

        let mut app = Self {
            input: String::new(),
            input_mode: InputMode::Editing,
            messages: Vec::new(),
            status: None,
            config,
            config_modal: None,
            pending_confirm: None,
            agent_client: None,
            scroll_offset: 0,
            connected: false,
            should_quit: false,
            should_reboot: false,
        };

        app.messages.push(Message {
            sender: MessageSender::System,
            content: format!("NIAT: The Intent-Driven Operating System (v{})", NIAT_VERSION),
            timestamp: now(),
        });
        app.messages.push(Message {
            sender: MessageSender::System,
            content: "Welcome! Type your intent in natural language or enter :help for available commands.".into(),
            timestamp: now(),
        });
        app.messages.push(Message {
            sender: MessageSender::System,
            content: "Quick commands: :config (Model & API settings) │ :reboot (Restart) │ :quit (Reboot) │ F2 (Shell)".into(),
            timestamp: now(),
        });

        app
    }

    pub async fn connect_agent(&mut self) {
        match AgentClient::connect().await {
            Ok(client) => {
                self.agent_client = Some(client);
                self.connected = true;
                self.messages.push(Message {
                    sender: MessageSender::Success,
                    content: "Connected to NIAT Agent Kernel daemon".into(),
                    timestamp: now(),
                });

                // Request initial status
                if let Some(c) = &mut self.agent_client {
                    let _ = c.send_status_request().await;
                }
            }
            Err(e) => {
                self.messages.push(Message {
                    sender: MessageSender::System,
                    content: format!("Agent Kernel not connected: {}. Running in standalone mode.", e),
                    timestamp: now(),
                });
            }
        }
    }

    pub fn open_config_modal(&mut self) {
        let current_key = self.config.model.api_key.clone().unwrap_or_default();
        self.config_modal = Some(ConfigModalState {
            base_url: self.config.model.base_url.clone(),
            model: self.config.model.model.clone(),
            api_key: current_key,
            active_field: ConfigField::BaseUrl,
            show_api_key: false,
        });
        self.input_mode = InputMode::ConfigModal;
    }

    pub fn close_config_modal(&mut self) {
        self.config_modal = None;
        self.input_mode = InputMode::Editing;
    }

    pub async fn save_config_modal(&mut self) {
        if let Some(modal) = self.config_modal.take() {
            let base_url = modal.base_url.trim().to_string();
            let model = modal.model.trim().to_string();
            let key = modal.api_key.trim().to_string();

            self.config.model.base_url = if base_url.is_empty() {
                "https://api.openai.com/v1".into()
            } else {
                base_url.clone()
            };
            self.config.model.model = if model.is_empty() {
                "gpt-4o".into()
            } else {
                model.clone()
            };
            self.config.model.api_key = if key.is_empty() {
                None
            } else {
                Some(key.clone())
            };

            // Save to disk
            let _ = self.config.save(NiatConfig::default_path());
            let _ = self.config.save(std::path::Path::new("/tmp/niat_config.toml"));

            // Sync with agent kernel
            if let Some(client) = &mut self.agent_client {
                let _ = client.send_update_config(
                    Some(self.config.model.base_url.clone()),
                    Some(self.config.model.model.clone()),
                    self.config.model.api_key.clone(),
                ).await;
            }

            self.messages.push(Message {
                sender: MessageSender::Success,
                content: format!(
                    "Configuration updated: Model = '{}', API URL = '{}', API Key = {}",
                    self.config.model.model,
                    self.config.model.base_url,
                    if self.config.model.api_key.is_some() { "[Set]" } else { "[None]" }
                ),
                timestamp: now(),
            });
        }
        self.input_mode = InputMode::Editing;
    }

    pub async fn execute_reboot(&mut self) {
        self.messages.push(Message {
            sender: MessageSender::System,
            content: "Rebooting system now...".into(),
            timestamp: now(),
        });

        if let Some(client) = &mut self.agent_client {
            let _ = client.send_reboot().await;
        }

        self.should_reboot = true;
    }

    pub async fn submit_input(&mut self) {
        let text = self.input.drain(..).collect::<String>();
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return;
        }

        // Handle slash or colon commands
        if trimmed.starts_with(':') || trimmed.starts_with('/') {
            let cmd = &trimmed[1..].trim();
            self.handle_command(cmd).await;
            self.input_mode = InputMode::Editing;
            return;
        }

        self.messages.push(Message {
            sender: MessageSender::User,
            content: text.clone(),
            timestamp: now(),
        });

        if let Some(client) = &mut self.agent_client {
            match client.send_intent(&text).await {
                Ok(()) => {}
                Err(e) => {
                    self.messages.push(Message {
                        sender: MessageSender::Error,
                        content: format!("Failed to send intent: {}", e),
                        timestamp: now(),
                    });
                }
            }
        } else {
            self.messages.push(Message {
                sender: MessageSender::System,
                content: "Agent Kernel not connected. Use F2 for direct shell access, or :config to setup API.".into(),
                timestamp: now(),
            });
        }

        self.input_mode = InputMode::Editing;
    }

    async fn handle_command(&mut self, cmd: &str) {
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        let name = parts.first().copied().unwrap_or("");

        match name {
            "q" | "quit" | "exit" | "reboot" | "restart" | "poweroff" | "shutdown" => {
                self.execute_reboot().await;
            }
            "config" | "settings" => {
                if parts.len() == 1 {
                    self.open_config_modal();
                } else if parts.len() >= 4 && parts[1] == "set" {
                    match parts[2] {
                        "url" | "base_url" => {
                            let url = parts[3..].join(" ");
                            self.config.model.base_url = url.clone();
                            let _ = self.config.save(NiatConfig::default_path());
                            let _ = self.config.save(std::path::Path::new("/tmp/niat_config.toml"));
                            if let Some(client) = &mut self.agent_client {
                                let _ = client.send_update_config(Some(url.clone()), None, None).await;
                            }
                            self.messages.push(Message {
                                sender: MessageSender::Success,
                                content: format!("API Base URL set to: {}", url),
                                timestamp: now(),
                            });
                        }
                        "model" => {
                            let model_name = parts[3..].join(" ");
                            self.config.model.model = model_name.clone();
                            let _ = self.config.save(NiatConfig::default_path());
                            let _ = self.config.save(std::path::Path::new("/tmp/niat_config.toml"));
                            if let Some(client) = &mut self.agent_client {
                                let _ = client.send_update_config(None, Some(model_name.clone()), None).await;
                            }
                            self.messages.push(Message {
                                sender: MessageSender::Success,
                                content: format!("Model name set to: {}", model_name),
                                timestamp: now(),
                            });
                        }
                        "key" | "api_key" => {
                            let key = parts[3..].join(" ");
                            self.config.model.api_key = Some(key.clone());
                            let _ = self.config.save(NiatConfig::default_path());
                            let _ = self.config.save(std::path::Path::new("/tmp/niat_config.toml"));
                            if let Some(client) = &mut self.agent_client {
                                let _ = client.send_update_config(None, None, Some(key)).await;
                            }
                            self.messages.push(Message {
                                sender: MessageSender::Success,
                                content: "API key updated successfully.".into(),
                                timestamp: now(),
                            });
                        }
                        _ => {
                            self.messages.push(Message {
                                sender: MessageSender::Error,
                                content: "Usage: :config set [url|model|key] <value>".into(),
                                timestamp: now(),
                            });
                        }
                    }
                } else if parts.len() == 2 && parts[1] == "show" {
                    let key_status = if self.config.model.api_key.is_some() { "Configured" } else { "Not set" };
                    self.messages.push(Message {
                        sender: MessageSender::System,
                        content: format!(
                            "Current Configuration:\n  • API URL: {}\n  • Model: {}\n  • API Key: {}",
                            self.config.model.base_url,
                            self.config.model.model,
                            key_status
                        ),
                        timestamp: now(),
                    });
                } else {
                    self.open_config_modal();
                }
            }
            "clear" | "cls" => {
                self.clear_messages();
            }
            "status" => {
                if let Some(client) = &mut self.agent_client {
                    let _ = client.send_status_request().await;
                }
                self.messages.push(Message {
                    sender: MessageSender::System,
                    content: format!(
                        "System Status:\n  • Agent Kernel: {}\n  • Model Provider: {}\n  • Active Model: {}",
                        if self.connected { "Connected (Unix socket)" } else { "Offline" },
                        if self.status.as_ref().map(|s| s.model_online).unwrap_or(false) { "Online" } else { "Offline / Unchecked" },
                        self.config.model.model
                    ),
                    timestamp: now(),
                });
            }
            "help" | "?" => {
                self.messages.push(Message {
                    sender: MessageSender::System,
                    content: [
                        "Available NIAT Commands:",
                        "  :config            Open interactive API & Model settings modal",
                        "  :config show       Display current model, URL, and key status",
                        "  :config set <item> Set url, model, or key directly",
                        "  :reboot / :quit    Reboot the operating system",
                        "  :clear             Clear message history",
                        "  :status            Show system and model status",
                        "  :help              Show this help reference",
                        "",
                        "Keyboard Shortcuts:",
                        "  F3                 Open API & Model Configuration modal",
                        "  F2                 Direct breakout to emergency shell",
                        "  Ctrl+L             Clear screen messages",
                        "  Ctrl+C / Ctrl+Q    Force reboot operating system",
                        "  PageUp / PageDown  Scroll message history",
                    ].join("\n"),
                    timestamp: now(),
                });
            }
            _ => {
                self.messages.push(Message {
                    sender: MessageSender::Error,
                    content: format!("Unknown command ':{}'. Type :help for a list of commands.", cmd),
                    timestamp: now(),
                });
            }
        }
    }

    pub async fn handle_approval(&mut self, approved: bool) {
        if let Some(confirm) = self.pending_confirm.take() {
            if let Some(client) = &mut self.agent_client {
                if approved {
                    self.messages.push(Message {
                        sender: MessageSender::Success,
                        content: format!("✓ Approved: {}", confirm.tool_name),
                        timestamp: now(),
                    });
                    let _ = client.send_approve(&confirm.request_id).await;
                } else {
                    self.messages.push(Message {
                        sender: MessageSender::System,
                        content: format!("✗ Denied: {}", confirm.tool_name),
                        timestamp: now(),
                    });
                    let _ = client.send_deny(&confirm.request_id).await;
                }
            }
        }
    }

    pub async fn poll_responses(&mut self) {
        if let Some(client) = &mut self.agent_client {
            while let Some(response) = client.try_recv().await {
                match response {
                    crate::agent_client::ParsedResponse::Text(content) => {
                        self.messages.push(Message {
                            sender: MessageSender::Agent,
                            content,
                            timestamp: now(),
                        });
                    }
                    crate::agent_client::ParsedResponse::ToolResult { name, output, success } => {
                        self.messages.push(Message {
                            sender: MessageSender::Tool(name),
                            content: if success {
                                output
                            } else {
                                format!("Error: {}", output)
                            },
                            timestamp: now(),
                        });
                    }
                    crate::agent_client::ParsedResponse::Confirm(confirm) => {
                        self.pending_confirm = Some(confirm);
                    }
                    crate::agent_client::ParsedResponse::Status(status) => {
                        self.status = Some(status);
                    }
                    crate::agent_client::ParsedResponse::ConfigUpdated { base_url, model, has_key, online } => {
                        self.config.model.base_url = base_url.clone();
                        self.config.model.model = model.clone();
                        self.messages.push(Message {
                            sender: MessageSender::Success,
                            content: format!(
                                "Agent Kernel updated configuration: Model = {}, URL = {}, Key = {} ({})",
                                model,
                                base_url,
                                if has_key { "Configured" } else { "None" },
                                if online { "Online" } else { "Offline" }
                            ),
                            timestamp: now(),
                        });
                    }
                    crate::agent_client::ParsedResponse::Error(msg) => {
                        self.messages.push(Message {
                            sender: MessageSender::Error,
                            content: format!("⚠ {}", msg),
                            timestamp: now(),
                        });
                    }
                }
            }
        }
    }

    pub fn clear_messages(&mut self) {
        self.messages.clear();
        self.scroll_offset = 0;
    }

    pub fn scroll_up(&mut self, amount: u16) {
        self.scroll_offset = self.scroll_offset.saturating_sub(amount);
    }

    pub fn scroll_down(&mut self, amount: u16) {
        self.scroll_offset = self.scroll_offset.saturating_add(amount);
    }
}

fn now() -> String {
    chrono::Local::now().format("%H:%M:%S").to_string()
}
