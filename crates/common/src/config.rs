//! NIAT system configuration types and loader.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

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

    /// Legacy live-session path. Still read so an existing session's key is
    /// picked up, but never written to (it does not survive reboot).
    pub fn legacy_tmp_path() -> &'static Path {
        Path::new("/tmp/niat_config.toml")
    }

    /// Per-user persistent path: `~/.niat/config.toml`.
    /// `None` when no home directory is known.
    pub fn user_path() -> Option<PathBuf> {
        std::env::var("HOME")
            .ok()
            .filter(|h| !h.trim().is_empty())
            .map(|home| PathBuf::from(home).join(".niat/config.toml"))
    }

    /// Persistent on-disk state path for system services.
    pub fn state_path() -> &'static Path {
        Path::new("/var/lib/niat/config.toml")
    }

    /// Ordered load candidates: user config first, then system, state,
    /// and finally the legacy tmp path for migration.
    pub fn load_candidates() -> Vec<PathBuf> {
        Self::load_candidates_for(Self::user_path())
    }

    fn load_candidates_for(user: Option<PathBuf>) -> Vec<PathBuf> {
        let mut out = Vec::with_capacity(4);
        if let Some(p) = user {
            out.push(p);
        }
        out.push(Self::default_path().to_path_buf());
        out.push(Self::state_path().to_path_buf());
        out.push(Self::legacy_tmp_path().to_path_buf());
        out
    }

    /// Load the first candidate that exists and parses, else defaults.
    /// Returns the config plus the path it was loaded from, if any.
    pub fn load_persistent() -> (Self, Option<PathBuf>) {
        Self::load_from_first(&Self::load_candidates())
    }

    fn load_from_first(paths: &[PathBuf]) -> (Self, Option<PathBuf>) {
        for path in paths {
            if !path.exists() {
                continue;
            }
            match Self::load(path) {
                Ok(cfg) => {
                    tracing::info!("Loaded config from {}", path.display());
                    return (cfg, Some(path.clone()));
                }
                Err(e) => {
                    tracing::warn!("Skipping unreadable config at {}: {}", path.display(), e);
                }
            }
        }
        (Self::default(), None)
    }

    /// Ordered save targets: `preferred` first (unless it is the legacy tmp
    /// path, which migrates to a persistent location instead), then the
    /// persistent load candidates. The tmp path is never a save target.
    pub fn save_targets(preferred: Option<&Path>) -> Vec<PathBuf> {
        Self::save_targets_for(Self::user_path(), preferred)
    }

    fn save_targets_for(user: Option<PathBuf>, preferred: Option<&Path>) -> Vec<PathBuf> {
        let mut targets = Vec::with_capacity(4);
        if let Some(p) = preferred {
            if p != Self::legacy_tmp_path() && !targets.contains(&p.to_path_buf()) {
                targets.push(p.to_path_buf());
            }
        }
        for p in Self::load_candidates_for(user) {
            if p != Self::legacy_tmp_path() && !targets.contains(&p) {
                targets.push(p);
            }
        }
        targets
    }

    /// Save to the first writable target and return the path written.
    /// Fails only when no candidate location is writable.
    pub fn save_persistent(&self, preferred: Option<&Path>) -> anyhow::Result<PathBuf> {
        self.save_to_first_writable(&Self::save_targets(preferred))
    }

    fn save_to_first_writable(&self, targets: &[PathBuf]) -> anyhow::Result<PathBuf> {
        let mut last_err = None;
        for target in targets {
            match self.save(target) {
                Ok(()) => {
                    tracing::info!("Saved config to {}", target.display());
                    return Ok(target.clone());
                }
                Err(e) => {
                    tracing::warn!("Cannot write config to {}: {}", target.display(), e);
                    last_err = Some(e);
                }
            }
        }
        Err(last_err.unwrap_or_else(|| anyhow::anyhow!("no writable config path found")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_dir(tag: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!("niat-config-test-{}-{}", std::process::id(), tag));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn user_path_lives_under_home_niat() {
        let path = NiatConfig::user_path().expect("HOME is set when running tests");
        assert!(
            path.ends_with(".niat/config.toml"),
            "unexpected user path: {}",
            path.display()
        );
    }

    #[test]
    fn candidates_prefer_user_then_system_state_legacy() {
        let user = PathBuf::from("/home/tester/.niat/config.toml");
        let got = NiatConfig::load_candidates_for(Some(user.clone()));
        assert_eq!(
            got,
            vec![
                user,
                NiatConfig::default_path().to_path_buf(),
                NiatConfig::state_path().to_path_buf(),
                NiatConfig::legacy_tmp_path().to_path_buf(),
            ]
        );
    }

    #[test]
    fn save_targets_prefer_persistent_and_never_tmp() {
        let user = PathBuf::from("/home/tester/.niat/config.toml");
        // A legacy preferred path migrates to persistent candidates instead.
        let got = NiatConfig::save_targets_for(Some(user.clone()), Some(NiatConfig::legacy_tmp_path()));
        assert_eq!(got[0], user);
        assert!(!got.contains(&NiatConfig::legacy_tmp_path().to_path_buf()));

        // A persistent preferred path stays first with no duplicates.
        let custom = PathBuf::from("/etc/niat/custom.toml");
        let got = NiatConfig::save_targets_for(Some(user.clone()), Some(&custom));
        assert_eq!(got[0], custom);
        let mut dedup = got.clone();
        dedup.sort();
        dedup.dedup();
        assert_eq!(dedup.len(), got.len());
    }

    #[test]
    fn load_from_first_skips_missing_and_corrupt() {
        let dir = scratch_dir("load");
        let missing = dir.join("missing.toml");
        let corrupt = dir.join("corrupt.toml");
        let valid = dir.join("valid.toml");
        std::fs::write(&corrupt, "not = [valid toml").unwrap();
        let mut cfg = NiatConfig::default();
        cfg.model.model = "loaded-model".into();
        cfg.save(&valid).unwrap();

        let (loaded, source) =
            NiatConfig::load_from_first(&[missing, corrupt, valid.clone()]);
        assert_eq!(loaded.model.model, "loaded-model");
        assert_eq!(source, Some(valid));

        let (fallback, source) =
            NiatConfig::load_from_first(&[dir.join("nope.toml")]);
        assert_eq!(fallback.model.model, NiatConfig::default().model.model);
        assert_eq!(source, None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_skips_unwritable_and_round_trips_key() {
        let dir = scratch_dir("save");
        // A path beneath a regular file can never be created: forces fallback.
        let blocker = dir.join("blocker");
        std::fs::write(&blocker, "x").unwrap();
        let unwritable = blocker.join("config.toml");
        let writable = dir.join("nested/niat/config.toml");

        let mut cfg = NiatConfig::default();
        cfg.model.model = "roundtrip-model".into();
        cfg.model.api_key = Some("sk-roundtrip".into());
        let written = cfg
            .save_to_first_writable(&[unwritable, writable.clone()])
            .unwrap();
        assert_eq!(written, writable);

        let loaded = NiatConfig::load(&writable).unwrap();
        assert_eq!(loaded.model.model, "roundtrip-model");
        assert_eq!(loaded.model.api_key.as_deref(), Some("sk-roundtrip"));

        assert!(cfg.save_to_first_writable(&[]).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
