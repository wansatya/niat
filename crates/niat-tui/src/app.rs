//! TUI application state.

use crate::agent_client::AgentClient;
use niat_common::config::NiatConfig;
use niat_common::types::{SafetyLevel, SystemStatus, NIAT_VERSION};
use std::path::PathBuf;
use std::time::SystemTime;

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
    // Kept for future transcript export; the lean UI shows no timestamps.
    #[allow(dead_code)]
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
    pub config_source: Option<PathBuf>,
    pub config_mtime: Option<SystemTime>,
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
        let (config, config_source) = Self::initial_config();

        let mut app = Self {
            input: String::new(),
            input_mode: InputMode::Editing,
            messages: Vec::new(),
            status: None,
            config,
            config_source,
            config_modal: None,
            pending_confirm: None,
            agent_client: None,
            scroll_offset: 0,
            connected: false,
            should_quit: false,
            should_reboot: false,
            config_mtime: None,
        };
        app.track_config_mtime();

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
            content: "Quick commands: :config (Model & API settings) · :reboot (Restart) · :quit (Reboot) · F2 (Shell)".into(),
            timestamp: now(),
        });
        app.messages.push(Message {
            sender: MessageSender::System,
            content: "Made in Indonesia @ 2026 - Niat Baik Initiative".into(),
            timestamp: now(),
        });

        app
    }

    #[cfg(not(test))]
    fn initial_config() -> (NiatConfig, Option<PathBuf>) {
        NiatConfig::load_persistent()
    }

    #[cfg(test)]
    fn initial_config() -> (NiatConfig, Option<PathBuf>) {
        // Unit tests must never read or create real config files; tests that
        // need on-disk config point `config_source` at scratch files instead.
        (NiatConfig::default(), None)
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

    /// Persist the current config: save back to the file it was loaded from
    /// when possible, otherwise to the first writable persistent location
    /// (`~/.niat/config.toml` first). Remembers the path written so later
    /// saves keep going to the same file.
    pub fn persist_config(&mut self) -> anyhow::Result<PathBuf> {
        let path = self.config.save_persistent(self.config_source.as_deref())?;
        self.config_source = Some(path.clone());
        self.track_config_mtime();
        Ok(path)
    }

    /// Reload configuration from disk and apply it when it differs from the
    /// live state, syncing the agent kernel. Manual refreshes (F5) always
    /// report; automatic ones stay silent unless something changed.
    pub async fn refresh_config(&mut self, manual: bool) {
        let (config, source) = self.reload_active_config();
        let changed = config.model != self.config.model;
        self.config = config;
        self.config_source = source.clone();
        self.track_config_mtime();
        if changed {
            if let Some(client) = &mut self.agent_client {
                let _ = client
                    .send_update_config(
                        Some(self.config.model.base_url.clone()),
                        Some(self.config.model.model.clone()),
                        Some(self.config.model.api_key.clone().unwrap_or_default()),
                    )
                    .await;
            }
            self.messages.push(Message {
                sender: MessageSender::Success,
                content: format!(
                    "Configuration reloaded from {}: model='{}', url='{}', key={}.",
                    config_source_label(&source),
                    self.config.model.model,
                    self.config.model.base_url,
                    if self.config.model.api_key.is_some() { "set" } else { "none" },
                ),
                timestamp: now(),
            });
        } else if manual {
            self.messages.push(Message {
                sender: MessageSender::System,
                content: format!(
                    "Configuration already up to date ({}).",
                    config_source_label(&source)
                ),
                timestamp: now(),
            });
        }
    }

    /// Re-read config from disk into the open modal's fields (F5 in the
    /// modal). The agent sync still happens on save, as usual.
    pub fn refresh_modal_from_disk(&mut self) {
        let (config, source) = self.reload_active_config();
        if let Some(modal) = &mut self.config_modal {
            modal.base_url = config.model.base_url.clone();
            modal.model = config.model.model.clone();
            modal.api_key = config.model.api_key.clone().unwrap_or_default();
        }
        self.messages.push(Message {
            sender: MessageSender::System,
            content: format!("Modal fields reloaded from {}.", config_source_label(&source)),
            timestamp: now(),
        });
    }

    /// Called every event-loop iteration: adopt external edits to the config
    /// file. Skipped while the modal holds its own snapshot, and silent
    /// unless values actually changed (so the agent kernel's own save-back
    /// after our updates causes no feedback loop).
    pub async fn poll_config_file(&mut self) {
        if self.input_mode == InputMode::ConfigModal {
            return;
        }
        let current = self
            .config_source
            .as_ref()
            .and_then(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok());
        let adopted = match (self.config_mtime, current) {
            (Some(tracked), Some(seen)) => seen > tracked,
            (None, Some(_)) => true,
            _ => false,
        };
        if adopted {
            self.refresh_config(false).await;
        }
    }

    /// Re-read the config file we loaded (when it still exists and parses),
    /// else fall back to the standard candidates.
    fn reload_active_config(&self) -> (NiatConfig, Option<PathBuf>) {
        if let Some(p) = &self.config_source {
            if p.exists() {
                if let Ok(cfg) = NiatConfig::load(p) {
                    return (cfg, Some(p.clone()));
                }
            }
        }
        NiatConfig::load_persistent()
    }

    fn track_config_mtime(&mut self) {
        self.config_mtime = self
            .config_source
            .as_ref()
            .and_then(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok());
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

            // Save to disk (persistent across restarts); report failures loudly.
            let save_note = match self.persist_config() {
                Ok(path) => format!("Saved to {}.", path.display()),
                Err(e) => {
                    self.messages.push(Message {
                        sender: MessageSender::Error,
                        content: format!(
                            "Could not save configuration to disk: {}. Settings apply until restart.",
                            e
                        ),
                        timestamp: now(),
                    });
                    "Disk save failed.".to_string()
                }
            };

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
                    "Configuration updated: Model = '{}', API URL = '{}', API Key = {}. {}",
                    self.config.model.model,
                    self.config.model.base_url,
                    if self.config.model.api_key.is_some() { "[Set]" } else { "[None]" },
                    save_note
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

    /// Insert bracketed-paste text (e.g. from terminal Ctrl+Shift+V) into the
    /// active editable field. Newlines/tabs become spaces and other control
    /// characters are dropped so a multi-line paste stays on one line.
    pub fn paste_text(&mut self, text: &str) {
        let cleaned: String = text
            .chars()
            .filter_map(|c| {
                if c == '\n' || c == '\r' || c == '\t' {
                    Some(' ')
                } else if c.is_control() {
                    None
                } else {
                    Some(c)
                }
            })
            .collect();
        if cleaned.is_empty() {
            return;
        }
        match self.input_mode {
            InputMode::Editing => self.input.push_str(&cleaned),
            InputMode::ConfigModal => {
                if let Some(modal) = &mut self.config_modal {
                    match modal.active_field {
                        ConfigField::BaseUrl => modal.base_url.push_str(&cleaned),
                        ConfigField::Model => modal.model.push_str(&cleaned),
                        ConfigField::ApiKey => modal.api_key.push_str(&cleaned),
                        ConfigField::SaveButton | ConfigField::CancelButton => {}
                    }
                }
            }
            InputMode::Normal => {}
        }
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
                            let save_note = match self.persist_config() {
                                Ok(path) => format!("Saved to {}.", path.display()),
                                Err(e) => format!("Disk save failed: {}.", e),
                            };
                            if let Some(client) = &mut self.agent_client {
                                let _ = client.send_update_config(Some(url.clone()), None, None).await;
                            }
                            self.messages.push(Message {
                                sender: MessageSender::Success,
                                content: format!("API Base URL set to: {}. {}", url, save_note),
                                timestamp: now(),
                            });
                        }
                        "model" => {
                            let model_name = parts[3..].join(" ");
                            self.config.model.model = model_name.clone();
                            let save_note = match self.persist_config() {
                                Ok(path) => format!("Saved to {}.", path.display()),
                                Err(e) => format!("Disk save failed: {}.", e),
                            };
                            if let Some(client) = &mut self.agent_client {
                                let _ = client.send_update_config(None, Some(model_name.clone()), None).await;
                            }
                            self.messages.push(Message {
                                sender: MessageSender::Success,
                                content: format!("Model name set to: {}. {}", model_name, save_note),
                                timestamp: now(),
                            });
                        }
                        "key" | "api_key" => {
                            let key = parts[3..].join(" ");
                            self.config.model.api_key = Some(key.clone());
                            let save_note = match self.persist_config() {
                                Ok(path) => format!("Saved to {}.", path.display()),
                                Err(e) => format!("Disk save failed: {}.", e),
                            };
                            if let Some(client) = &mut self.agent_client {
                                let _ = client.send_update_config(None, None, Some(key)).await;
                            }
                            self.messages.push(Message {
                                sender: MessageSender::Success,
                                content: format!("API key updated successfully. {}", save_note),
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
                            "Current Configuration:\n  • API URL: {}\n  • Model: {}\n  • API Key: {}\n  • Source: {}",
                            self.config.model.base_url,
                            self.config.model.model,
                            key_status,
                            config_source_label(&self.config_source)
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
                        "  F5                 Reload configuration from disk",
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

fn config_source_label(source: &Option<PathBuf>) -> String {
    source
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "defaults".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paste_appends_to_editing_input() {
        let mut app = App::new();
        app.input_mode = InputMode::Editing;
        app.input = "check ".into();
        app.paste_text("disk usage");
        assert_eq!(app.input, "check disk usage");
    }

    #[test]
    fn paste_sanitizes_newlines_and_control_chars() {
        let mut app = App::new();
        app.input_mode = InputMode::Editing;
        app.paste_text("line1\r\nline2\tline3\x1b[0m");
        assert_eq!(app.input, "line1  line2 line3[0m");
    }

    #[test]
    fn paste_targets_active_config_field() {
        let mut app = App::new();
        app.open_config_modal();
        let modal = app.config_modal.as_mut().unwrap();
        modal.active_field = ConfigField::ApiKey;
        modal.api_key.clear();
        app.paste_text("sk-test-key");
        assert_eq!(app.config_modal.as_ref().unwrap().api_key, "sk-test-key");
        assert!(app.input.is_empty());
    }

    #[test]
    fn paste_ignored_in_normal_mode_and_on_buttons() {
        let mut app = App::new();
        app.input_mode = InputMode::Normal;
        app.paste_text("hello");
        assert!(app.input.is_empty());

        app.open_config_modal();
        app.config_modal.as_mut().unwrap().active_field = ConfigField::SaveButton;
        app.paste_text("hello");
        assert!(app.input.is_empty());
    }

    #[test]
    fn persist_config_saves_back_to_loaded_path() {
        let mut dir = std::env::temp_dir();
        dir.push(format!("niat-app-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");

        let mut app = App::new();
        app.config.model.model = "persisted-model".into();
        // A scratch preferred path wins first, so no real location is touched.
        app.config_source = Some(path.clone());
        let saved = app.persist_config().unwrap();
        assert_eq!(saved, path);
        assert_eq!(app.config_source.as_ref(), Some(&path));
        let reloaded = NiatConfig::load(&path).unwrap();
        assert_eq!(reloaded.model.model, "persisted-model");
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn write_refresh_fixture(path: &std::path::Path, model: &str, key: Option<&str>) {
        let mut cfg = NiatConfig::default();
        cfg.model.model = model.into();
        cfg.model.api_key = key.map(|k| k.into());
        cfg.save(path).unwrap();
    }

    fn refresh_scratch(tag: &str) -> std::path::PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!("niat-refresh-test-{}-{}", std::process::id(), tag));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("config.toml")
    }

    #[tokio::test]
    async fn refresh_applies_external_config_change() {
        let path = refresh_scratch("apply");
        write_refresh_fixture(&path, "external-model", Some("sk-ext"));

        let mut app = App::new();
        app.config_source = Some(path.clone());
        app.config.model.model = "stale-model".into();
        let before = app.messages.len();
        app.refresh_config(false).await;
        assert_eq!(app.config.model.model, "external-model");
        assert_eq!(app.config.model.api_key.as_deref(), Some("sk-ext"));
        assert_eq!(app.messages.len(), before + 1);
        assert!(app.messages.last().unwrap().content.contains("reloaded"));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[tokio::test]
    async fn refresh_reports_up_to_date_when_manual_and_unchanged() {
        let path = refresh_scratch("noop");
        write_refresh_fixture(&path, "same-model", None);

        let mut app = App::new();
        app.config_source = Some(path.clone());
        app.config.model = NiatConfig::load(&path).unwrap().model;
        let before = app.messages.len();
        app.refresh_config(false).await;
        assert_eq!(app.messages.len(), before);
        app.refresh_config(true).await;
        assert_eq!(app.messages.len(), before + 1);
        assert!(app.messages.last().unwrap().content.contains("up to date"));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[tokio::test]
    async fn poll_config_file_picks_up_external_edit() {
        let path = refresh_scratch("poll");
        write_refresh_fixture(&path, "v1-model", None);

        let mut app = App::new();
        app.config_source = Some(path.clone());
        app.config.model = NiatConfig::load(&path).unwrap().model;
        app.track_config_mtime();
        let before = app.messages.len();
        app.poll_config_file().await;
        assert_eq!(app.messages.len(), before);

        // External edit, with a forced-old tracked mtime to avoid tick flakiness.
        write_refresh_fixture(&path, "v2-model", None);
        app.config_mtime = Some(std::time::SystemTime::UNIX_EPOCH);
        app.poll_config_file().await;
        assert_eq!(app.config.model.model, "v2-model");
        assert_eq!(app.messages.len(), before + 1);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn modal_refresh_reloads_fields_from_disk() {
        let path = refresh_scratch("modal");
        write_refresh_fixture(&path, "modal-model", Some("sk-modal"));

        let mut app = App::new();
        app.config_source = Some(path.clone());
        app.open_config_modal();
        let modal = app.config_modal.as_mut().unwrap();
        modal.base_url.clear();
        modal.model.clear();
        modal.api_key.clear();
        app.refresh_modal_from_disk();
        let modal = app.config_modal.as_ref().unwrap();
        assert_eq!(modal.model, "modal-model");
        assert_eq!(modal.api_key, "sk-modal");
        assert!(app.messages.last().unwrap().content.contains("Modal fields reloaded"));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
