//! Configuration management with XDG paths
//!
//! ~/.config/codish/config.json - API key, preferences (0600)
//! ~/.cache/codish/models.json  - Cached model list
//! ~/.local/state/codish/       - Session logs

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

// ═══════════════════════════════════════════════════════════════
// PERMISSION SYSTEM
// ═══════════════════════════════════════════════════════════════

/// Permission mode for tool operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PermissionMode {
    /// Always allow without prompting
    #[default]
    Auto,
    /// Prompt user for confirmation
    Ask,
    /// Always deny
    Deny,
}

/// Category of tool operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolCategory {
    /// File reads (read, glob, grep) - generally safe
    Read,
    /// File writes (write, patch) - can modify codebase
    Write,
    /// Shell commands (bash) - arbitrary execution
    Execute,
    /// Git operations (commit, push) - affects repository
    Git,
}

impl ToolCategory {
    /// Get category for a tool name
    pub fn from_tool(tool: &str) -> Self {
        match tool {
            "read" | "glob" | "grep" | "find" => Self::Read,
            "write" | "patch" | "edit" => Self::Write,
            "bash" | "shell" | "exec" => Self::Execute,
            "git" | "commit" | "push" | "checkout" => Self::Git,
            _ => Self::Execute, // Unknown tools are treated as execute
        }
    }

    /// Default permission mode for this category
    pub fn default_mode(&self) -> PermissionMode {
        match self {
            Self::Read => PermissionMode::Auto,   // Safe to auto-allow
            Self::Write => PermissionMode::Ask,   // Ask before modifying
            Self::Execute => PermissionMode::Ask, // Ask before running
            Self::Git => PermissionMode::Ask,     // Ask before git ops
        }
    }

    /// Description for permission prompts
    pub fn description(&self) -> &'static str {
        match self {
            Self::Read => "read files",
            Self::Write => "modify files",
            Self::Execute => "run shell commands",
            Self::Git => "perform git operations",
        }
    }
}

/// Permission settings for tool operations
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Permissions {
    /// Mode for file read operations
    #[serde(default)]
    pub read: PermissionMode,

    /// Mode for file write operations
    #[serde(default)]
    pub write: PermissionMode,

    /// Mode for shell execution
    #[serde(default)]
    pub execute: PermissionMode,

    /// Mode for git operations
    #[serde(default)]
    pub git: PermissionMode,

    /// Paths always allowed (glob patterns)
    #[serde(default, skip_serializing_if = "HashSet::is_empty")]
    pub allowed_paths: HashSet<String>,

    /// Paths always denied (glob patterns)
    #[serde(default, skip_serializing_if = "HashSet::is_empty")]
    pub denied_paths: HashSet<String>,

    /// Commands always allowed (prefix match)
    #[serde(default, skip_serializing_if = "HashSet::is_empty")]
    pub allowed_commands: HashSet<String>,

    /// Commands always denied (prefix match)
    #[serde(default, skip_serializing_if = "HashSet::is_empty")]
    pub denied_commands: HashSet<String>,
}

impl Permissions {
    /// Get permission mode for a category
    pub fn mode_for(&self, category: ToolCategory) -> PermissionMode {
        match category {
            ToolCategory::Read => self.read,
            ToolCategory::Write => self.write,
            ToolCategory::Execute => self.execute,
            ToolCategory::Git => self.git,
        }
    }

    /// Check if a path is explicitly allowed
    pub fn is_path_allowed(&self, path: &str) -> Option<bool> {
        // Check deny list first
        for pattern in &self.denied_paths {
            if path_matches(path, pattern) {
                return Some(false);
            }
        }
        // Then allow list
        for pattern in &self.allowed_paths {
            if path_matches(path, pattern) {
                return Some(true);
            }
        }
        None // No explicit rule
    }

    /// Check if a command is explicitly allowed
    pub fn is_command_allowed(&self, cmd: &str) -> Option<bool> {
        // Check deny list first
        for pattern in &self.denied_commands {
            if cmd.starts_with(pattern) || cmd == pattern {
                return Some(false);
            }
        }
        // Then allow list
        for pattern in &self.allowed_commands {
            if cmd.starts_with(pattern) || cmd == pattern {
                return Some(true);
            }
        }
        None // No explicit rule
    }

    /// Create permissive permissions (auto-allow everything)
    pub fn permissive() -> Self {
        Self {
            read: PermissionMode::Auto,
            write: PermissionMode::Auto,
            execute: PermissionMode::Auto,
            git: PermissionMode::Auto,
            ..Default::default()
        }
    }

    /// Create restrictive permissions (ask for everything)
    pub fn restrictive() -> Self {
        Self {
            read: PermissionMode::Auto, // Reads are still safe
            write: PermissionMode::Ask,
            execute: PermissionMode::Ask,
            git: PermissionMode::Ask,
            ..Default::default()
        }
    }
}

/// Result of a permission check
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionCheck {
    /// Operation is allowed
    Allowed,
    /// Operation needs user confirmation
    NeedsConfirmation { category: &'static str, description: String },
    /// Operation is denied
    Denied { reason: String },
}

impl PermissionCheck {
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allowed)
    }
}

/// Check if a tool operation is permitted
pub fn check_tool_permission(
    config: &Config,
    tool_name: &str,
    args: &serde_json::Value,
) -> PermissionCheck {
    // Trust mode bypasses all checks
    if config.trust_mode {
        return PermissionCheck::Allowed;
    }

    let category = ToolCategory::from_tool(tool_name);
    let perms = &config.permissions;

    // Check explicit path/command rules first
    match tool_name {
        "read" | "write" | "patch" | "glob" | "grep" => {
            if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
                if let Some(false) = perms.is_path_allowed(path) {
                    return PermissionCheck::Denied {
                        reason: format!("Path '{}' is in denied list", path),
                    };
                }
                if let Some(true) = perms.is_path_allowed(path) {
                    return PermissionCheck::Allowed;
                }
            }
        }
        "bash" | "shell" | "exec" => {
            if let Some(cmd) = args.get("command").and_then(|v| v.as_str()) {
                if let Some(false) = perms.is_command_allowed(cmd) {
                    return PermissionCheck::Denied {
                        reason: format!("Command '{}' is in denied list", cmd),
                    };
                }
                if let Some(true) = perms.is_command_allowed(cmd) {
                    return PermissionCheck::Allowed;
                }
            }
        }
        _ => {}
    }

    // Check category-level permission
    match perms.mode_for(category) {
        PermissionMode::Auto => PermissionCheck::Allowed,
        PermissionMode::Deny => PermissionCheck::Denied {
            reason: format!("{} operations are disabled", category.description()),
        },
        PermissionMode::Ask => {
            let desc = match tool_name {
                "bash" | "shell" => {
                    let cmd = args.get("command").and_then(|v| v.as_str()).unwrap_or("?");
                    format!("Run: {}", truncate(cmd, 60))
                }
                "write" => {
                    let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("?");
                    format!("Write to: {}", path)
                }
                "patch" => {
                    let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("?");
                    format!("Patch: {}", path)
                }
                "git" | "commit" => {
                    let msg = args.get("message").and_then(|v| v.as_str()).unwrap_or("?");
                    format!("Git commit: {}", truncate(msg, 40))
                }
                _ => format!("{}: {}", tool_name, args),
            };
            PermissionCheck::NeedsConfirmation {
                category: category.description(),
                description: desc,
            }
        }
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max - 3])
    }
}

/// Simple glob pattern matching (supports * and **)
fn path_matches(path: &str, pattern: &str) -> bool {
    // Handle ** (matches any path components including /)
    if pattern.contains("**") {
        // Split on ** and check prefix/suffix
        let parts: Vec<&str> = pattern.split("**").collect();
        if parts.len() == 2 {
            let prefix = parts[0].trim_end_matches('/');
            let suffix = parts[1].trim_start_matches('/');

            let prefix_ok = prefix.is_empty() || path.starts_with(prefix);
            let suffix_ok = suffix.is_empty() || {
                // Handle *.ext pattern in suffix
                if let Some(ext) = suffix.strip_prefix('*') {
                    path.ends_with(ext)
                } else {
                    path.ends_with(suffix)
                }
            };

            return prefix_ok && suffix_ok;
        }
    }

    // Handle single * (matches within a single path component)
    if pattern.contains('*') {
        let parts: Vec<&str> = pattern.split('*').collect();
        if parts.len() == 2 {
            return path.starts_with(parts[0]) && path.ends_with(parts[1]);
        }
    }

    // Exact match or prefix (directory match)
    path == pattern || path.starts_with(&format!("{}/", pattern))
}

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

    /// Tool permission settings
    #[serde(default)]
    pub permissions: Permissions,

    /// Trust mode: skip all permission checks (for automation)
    #[serde(default)]
    pub trust_mode: bool,
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

    /// Save config to disk with secure permissions (atomic write)
    pub fn save(&self) -> Result<()> {
        ensure_dirs()?;
        let path = config_path()?;
        let tmp_path = path.with_extension("json.tmp");

        let content = serde_json::to_string_pretty(self)?;

        // Create temp file with secure permissions from the start
        {
            use std::os::unix::fs::OpenOptionsExt;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600) // Secure from creation - no race window
                .open(&tmp_path)
                .with_context(|| "Failed to create temp file".to_string())?;
            file.write_all(content.as_bytes())?;
            file.sync_all()?; // Ensure data is on disk before rename
        }

        // Atomic rename (POSIX guarantees)
        fs::rename(&tmp_path, &path)
            .with_context(|| "Failed to rename config".to_string())?;

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

    #[test]
    fn test_tool_category_from_tool() {
        assert_eq!(ToolCategory::from_tool("read"), ToolCategory::Read);
        assert_eq!(ToolCategory::from_tool("glob"), ToolCategory::Read);
        assert_eq!(ToolCategory::from_tool("write"), ToolCategory::Write);
        assert_eq!(ToolCategory::from_tool("bash"), ToolCategory::Execute);
        assert_eq!(ToolCategory::from_tool("git"), ToolCategory::Git);
        assert_eq!(ToolCategory::from_tool("unknown"), ToolCategory::Execute);
    }

    #[test]
    fn test_permission_mode_default() {
        let perms = Permissions::default();
        assert_eq!(perms.read, PermissionMode::Auto);
        assert_eq!(perms.write, PermissionMode::Auto);
    }

    #[test]
    fn test_permissions_permissive() {
        let perms = Permissions::permissive();
        assert_eq!(perms.mode_for(ToolCategory::Write), PermissionMode::Auto);
        assert_eq!(perms.mode_for(ToolCategory::Execute), PermissionMode::Auto);
    }

    #[test]
    fn test_permissions_restrictive() {
        let perms = Permissions::restrictive();
        assert_eq!(perms.mode_for(ToolCategory::Read), PermissionMode::Auto);
        assert_eq!(perms.mode_for(ToolCategory::Write), PermissionMode::Ask);
        assert_eq!(perms.mode_for(ToolCategory::Execute), PermissionMode::Ask);
    }

    #[test]
    fn test_path_allowlist() {
        let mut perms = Permissions::default();
        perms.allowed_paths.insert("src/**".to_string());
        perms.denied_paths.insert("src/secrets/**".to_string());

        assert_eq!(perms.is_path_allowed("src/main.rs"), Some(true));
        assert_eq!(perms.is_path_allowed("src/secrets/key.txt"), Some(false));
        assert_eq!(perms.is_path_allowed("other/file.rs"), None);
    }

    #[test]
    fn test_command_allowlist() {
        let mut perms = Permissions::default();
        perms.allowed_commands.insert("cargo".to_string());
        perms.denied_commands.insert("rm -rf".to_string());

        assert_eq!(perms.is_command_allowed("cargo build"), Some(true));
        assert_eq!(perms.is_command_allowed("rm -rf /"), Some(false));
        assert_eq!(perms.is_command_allowed("ls"), None);
    }

    #[test]
    fn test_path_matches_exact() {
        assert!(path_matches("src/main.rs", "src/main.rs"));
        assert!(!path_matches("src/main.rs", "src/lib.rs"));
    }

    #[test]
    fn test_path_matches_wildcard() {
        assert!(path_matches("src/main.rs", "src/*.rs"));
        assert!(path_matches("test.txt", "*.txt"));
        assert!(!path_matches("src/main.rs", "*.txt"));
    }

    #[test]
    fn test_path_matches_globstar() {
        assert!(path_matches("src/foo/bar/main.rs", "src/**/*.rs"));
        assert!(path_matches("a/b/c/d.txt", "**/*.txt"));
    }

    #[test]
    fn test_permissions_serialize() {
        let perms = Permissions::restrictive();
        let json = serde_json::to_string(&perms).unwrap();
        assert!(json.contains("\"write\":\"ask\""));
    }

    #[test]
    fn test_check_permission_trust_mode() {
        let mut cfg = Config::default();
        cfg.trust_mode = true;
        cfg.permissions = Permissions::restrictive();

        let check = check_tool_permission(&cfg, "bash", &serde_json::json!({"command": "rm -rf /"}));
        assert_eq!(check, PermissionCheck::Allowed);
    }

    #[test]
    fn test_check_permission_denied_command() {
        let mut cfg = Config::default();
        cfg.permissions.denied_commands.insert("rm -rf".to_string());

        let check = check_tool_permission(&cfg, "bash", &serde_json::json!({"command": "rm -rf /"}));
        assert!(matches!(check, PermissionCheck::Denied { .. }));
    }

    #[test]
    fn test_check_permission_allowed_command() {
        let mut cfg = Config::default();
        cfg.permissions.execute = PermissionMode::Ask;
        cfg.permissions.allowed_commands.insert("cargo".to_string());

        let check = check_tool_permission(&cfg, "bash", &serde_json::json!({"command": "cargo build"}));
        assert_eq!(check, PermissionCheck::Allowed);
    }

    #[test]
    fn test_check_permission_needs_confirmation() {
        let mut cfg = Config::default();
        cfg.permissions = Permissions::restrictive();

        let check = check_tool_permission(&cfg, "bash", &serde_json::json!({"command": "ls"}));
        assert!(matches!(check, PermissionCheck::NeedsConfirmation { .. }));
    }

    #[test]
    fn test_check_permission_read_auto() {
        let cfg = Config::default();

        let check = check_tool_permission(&cfg, "read", &serde_json::json!({"path": "/etc/passwd"}));
        assert_eq!(check, PermissionCheck::Allowed);
    }

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("short", 10), "short");
        assert_eq!(truncate("this is a longer string", 10), "this is...");
    }
}
