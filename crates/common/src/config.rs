//! NIAT system configuration types and loader.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Top-level NIAT configuration, deserialized from `/etc/niat/config.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NiatConfig {
    #[serde(default)]
    pub system: SystemConfig,
    #[serde(default)]
    pub model: ModelConfig,
    #[serde(default)]
    pub agent: AgentConfig,
    #[serde(default)]
    pub security: SecurityConfig,
    #[serde(default)]
    pub network: NetworkConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemConfig {
    #[serde(default = "default_hostname")]
    pub hostname: String,
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelConfig {
    #[serde(default = "default_base_url")]
    pub base_url: String,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_api_key_env")]
    pub api_key_env: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    #[serde(default = "default_max_iterations")]
    pub max_tool_iterations: u32,
    #[serde(default = "default_system_prompt")]
    pub system_prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    #[serde(default = "default_true")]
    pub require_confirmation: bool,
    #[serde(default)]
    pub allow_raw_shell: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    #[serde(default = "default_true")]
    pub dhcp: bool,
    #[serde(default = "default_true")]
    pub check_connectivity: bool,
}

// Defaults
fn default_hostname() -> String { "niat".into() }
fn default_log_level() -> String { "info".into() }
fn default_base_url() -> String { "https://api.openai.com/v1".into() }
fn default_model() -> String { "gpt-4o".into() }
fn default_api_key_env() -> String { "NIAT_API_KEY".into() }
fn default_timeout() -> u64 { 30 }
fn default_max_tokens() -> u32 { 4096 }
fn default_max_iterations() -> u32 { 15 }
fn default_true() -> bool { true }
fn default_system_prompt() -> String {
    "You are NIAT, an intent-driven operating system assistant. \
     You translate user intent into safe, controlled system operations. \
     Always explain what you will do before acting. \
     Never perform destructive operations without explicit user approval."
        .into()
}

impl Default for NiatConfig {
    fn default() -> Self {
        Self {
            system: SystemConfig::default(),
            model: ModelConfig::default(),
            agent: AgentConfig::default(),
            security: SecurityConfig::default(),
            network: NetworkConfig::default(),
        }
    }
}

impl Default for SystemConfig {
    fn default() -> Self {
        Self {
            hostname: default_hostname(),
            log_level: default_log_level(),
        }
    }
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            base_url: default_base_url(),
            model: default_model(),
            api_key_env: default_api_key_env(),
            api_key: None,
            timeout_seconds: default_timeout(),
            max_tokens: default_max_tokens(),
        }
    }
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_tool_iterations: default_max_iterations(),
            system_prompt: default_system_prompt(),
        }
    }
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            require_confirmation: default_true(),
            allow_raw_shell: false,
        }
    }
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            dhcp: default_true(),
            check_connectivity: default_true(),
        }
    }
}

impl NiatConfig {
    /// Load config from a TOML file path, falling back to defaults.
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        if path.exists() {
            let content = std::fs::read_to_string(path)?;
            let config: NiatConfig = toml::from_str(&content)?;
            Ok(config)
        } else {
            tracing::warn!("Config file not found at {}, using defaults", path.display());
            Ok(Self::default())
        }
    }

    /// Save config to a TOML file path.
    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let serialized = toml::to_string_pretty(self)?;
        std::fs::write(path, serialized)?;
        Ok(())
    }

    /// Standard config path on the NIAT filesystem.
    pub fn default_path() -> &'static Path {
        Path::new("/etc/niat/config.toml")
    }
}
