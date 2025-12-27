//! Configuration management with XDG paths
//!
//! ~/.config/codish/config.json - API key, preferences (0600)
//! ~/.cache/codish/models.json  - Cached model list
//! ~/.local/state/codish/       - Session logs

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

const APP_NAME: &str = "hyle";

/// Get config directory (~/.config/codish/)
pub fn config_dir() -> Result<PathBuf> {
    let base = dirs::config_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join(".config")))
        .context("Could not determine config directory")?;
    Ok(base.join(APP_NAME))
}

/// Get cache directory (~/.cache/codish/)
pub fn cache_dir() -> Result<PathBuf> {
    let base = dirs::cache_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join(".cache")))
        .context("Could not determine cache directory")?;
    Ok(base.join(APP_NAME))
}

/// Get state directory (~/.local/state/codish/)
pub fn state_dir() -> Result<PathBuf> {
    let base = dirs::state_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join(".local/state")))
        .context("Could not determine state directory")?;
    Ok(base.join(APP_NAME))
}

/// Get config file path
pub fn config_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("config.json"))
}

/// Ensure all directories exist
pub fn ensure_dirs() -> Result<()> {
    fs::create_dir_all(config_dir()?)?;
    fs::create_dir_all(cache_dir()?)?;
    fs::create_dir_all(state_dir()?)?;
    Ok(())
}

/// Main configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    /// OpenRouter API key
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,

    /// Default model to use
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_model: Option<String>,

    /// Show only free models by default
    #[serde(default)]
    pub free_only: bool,

    /// Telemetry sample rate (Hz)
    #[serde(default = "default_sample_rate")]
    pub telemetry_hz: u32,

    /// Auto-throttle on pressure
    #[serde(default = "default_true")]
    pub auto_throttle: bool,
}

fn default_sample_rate() -> u32 { 4 }
fn default_true() -> bool { true }

impl Config {
    /// Load config from disk, or return defaults
    pub fn load() -> Result<Self> {
        ensure_dirs()?;
        let path = config_path()?;

        if path.exists() {
            let content = fs::read_to_string(&path)
                .with_context(|| format!("Failed to read {}", path.display()))?;
            let config: Config = serde_json::from_str(&content)
                .with_context(|| format!("Failed to parse {}", path.display()))?;
            Ok(config)
        } else {
            Ok(Config::default())
        }
    }

    /// Save config to disk with secure permissions
    pub fn save(&self) -> Result<()> {
        ensure_dirs()?;
        let path = config_path()?;

        let content = serde_json::to_string_pretty(self)?;
        fs::write(&path, &content)
            .with_context(|| format!("Failed to write {}", path.display()))?;

        // Set permissions to 0600 (owner read/write only) for API key security
        let mut perms = fs::metadata(&path)?.permissions();
        perms.set_mode(0o600);
        fs::set_permissions(&path, perms)?;

        Ok(())
    }
}

/// Get API key from config or environment
pub fn get_api_key() -> Result<String> {
    // Environment variable takes precedence
    if let Ok(key) = std::env::var("OPENROUTER_API_KEY") {
        if !key.is_empty() {
            return Ok(key);
        }
    }

    // Otherwise, check config
    let cfg = Config::load()?;
    cfg.api_key.context("No API key configured. Set OPENROUTER_API_KEY or run: codish config set key <your-key>")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let cfg = Config {
            auto_throttle: default_true(),
            telemetry_hz: default_sample_rate(),
            ..Default::default()
        };
        assert!(cfg.api_key.is_none());
        assert!(cfg.auto_throttle);
        assert_eq!(cfg.telemetry_hz, 4);
    }

    #[test]
    fn test_config_serialize() {
        let cfg = Config {
            api_key: Some("test-key".to_string()),
            default_model: Some("test/model".to_string()),
            ..Default::default()
        };

        let json = serde_json::to_string(&cfg).unwrap();
        assert!(json.contains("test-key"));
        assert!(json.contains("test/model"));
    }
}
