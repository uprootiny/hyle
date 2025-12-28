//! Environment awareness and situational mapping
//!
//! Provides a holistic view of the user's working environment:
//! - Available tools and capabilities
//! - Recent activity and projects
//! - System constraints and resources
//! - Remote access and connectivity

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

// ═══════════════════════════════════════════════════════════════
// ENVIRONMENT MAP
// ═══════════════════════════════════════════════════════════════

/// Complete environment map
#[derive(Debug, Default)]
pub struct EnvironmentMap {
    pub tools: ToolInventory,
    pub projects: ProjectMap,
    pub resources: SystemResources,
    pub access: AccessMap,
    pub activity: RecentActivity,
}

impl EnvironmentMap {
    /// Gather complete environment information
    pub fn gather() -> Self {
        Self {
            tools: ToolInventory::detect(),
            projects: ProjectMap::scan(),
            resources: SystemResources::check(),
            access: AccessMap::discover(),
            activity: RecentActivity::collect(),
        }
    }

    /// Render as human-readable text
    pub fn display(&self) -> String {
        let mut out = String::new();

        out.push_str("═══ Environment Map ═══\n\n");

        // Tools
        out.push_str("▸ Tools\n");
        for (name, available) in &self.tools.tools {
            let icon = if *available { "✓" } else { "·" };
            out.push_str(&format!("  {} {}\n", icon, name));
        }

        // Current project
        out.push_str("\n▸ Current Project\n");
        if let Some(ref proj) = self.projects.current {
            out.push_str(&format!("  {} ({})\n", proj.name, proj.project_type));
            if !proj.git_remote.is_empty() {
                out.push_str(&format!("  remote: {}\n", proj.git_remote));
            }
            if !proj.git_branch.is_empty() {
                out.push_str(&format!("  branch: {}\n", proj.git_branch));
            }
        } else {
            out.push_str("  (not in a project)\n");
        }

        // Recent projects
        if !self.projects.recent.is_empty() {
            out.push_str("\n▸ Recent Projects\n");
            for proj in self.projects.recent.iter().take(5) {
                out.push_str(&format!("  {} - {}\n", proj.name, proj.path.display()));
            }
        }

        // Resources
        out.push_str("\n▸ Resources\n");
        out.push_str(&format!("  memory: {}% used\n", self.resources.memory_percent));
        out.push_str(&format!("  disk: {}% used\n", self.resources.disk_percent));
        out.push_str(&format!("  load: {:.1}\n", self.resources.load_avg));

        // Access
        out.push_str("\n▸ Access\n");
        out.push_str(&format!("  ssh keys: {}\n", self.access.ssh_keys.len()));
        out.push_str(&format!("  gh auth: {}\n", if self.access.gh_authenticated { "yes" } else { "no" }));
        if !self.access.known_hosts.is_empty() {
            out.push_str(&format!("  known hosts: {}\n", self.access.known_hosts.len()));
        }

        // Activity
        out.push_str("\n▸ Recent Activity\n");
        out.push_str(&format!("  hyle sessions: {}\n", self.activity.session_count));
        if !self.activity.recent_files.is_empty() {
            out.push_str("  recent files:\n");
            for f in self.activity.recent_files.iter().take(5) {
                out.push_str(&format!("    {}\n", f));
            }
        }

        out
    }

    /// Render as JSON
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "tools": self.tools.tools,
            "current_project": self.projects.current.as_ref().map(|p| {
                serde_json::json!({
                    "name": p.name,
                    "type": p.project_type,
                    "path": p.path.display().to_string(),
                    "git_remote": p.git_remote,
                    "git_branch": p.git_branch,
                })
            }),
            "recent_projects": self.projects.recent.iter().map(|p| {
                serde_json::json!({
                    "name": p.name,
                    "path": p.path.display().to_string(),
                })
            }).collect::<Vec<_>>(),
            "resources": {
                "memory_percent": self.resources.memory_percent,
                "disk_percent": self.resources.disk_percent,
                "load_avg": self.resources.load_avg,
            },
            "access": {
                "ssh_keys": self.access.ssh_keys.len(),
                "gh_authenticated": self.access.gh_authenticated,
                "known_hosts": self.access.known_hosts.len(),
            },
            "activity": {
                "sessions": self.activity.session_count,
                "recent_files": self.activity.recent_files,
            }
        })
    }
}

// ═══════════════════════════════════════════════════════════════
// TOOL INVENTORY
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Default)]
pub struct ToolInventory {
    pub tools: HashMap<String, bool>,
}

impl ToolInventory {
    pub fn detect() -> Self {
        let check = |name: &str| -> bool {
            Command::new("which")
                .arg(name)
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
        };

        let mut tools = HashMap::new();

        // Development tools
        tools.insert("git".into(), check("git"));
        tools.insert("cargo".into(), check("cargo"));
        tools.insert("rustc".into(), check("rustc"));
        tools.insert("npm".into(), check("npm"));
        tools.insert("node".into(), check("node"));
        tools.insert("python".into(), check("python3") || check("python"));
        tools.insert("go".into(), check("go"));
        tools.insert("docker".into(), check("docker"));

        // GitHub CLI
        tools.insert("gh".into(), check("gh"));

        // Editors
        tools.insert("vim".into(), check("vim") || check("nvim"));
        tools.insert("code".into(), check("code"));

        // Utilities
        tools.insert("tmux".into(), check("tmux"));
        tools.insert("curl".into(), check("curl"));
        tools.insert("jq".into(), check("jq"));
        tools.insert("rg".into(), check("rg"));
        tools.insert("fd".into(), check("fd"));

        Self { tools }
    }

    pub fn has(&self, tool: &str) -> bool {
        self.tools.get(tool).copied().unwrap_or(false)
    }
}

// ═══════════════════════════════════════════════════════════════
// PROJECT MAP
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Default)]
pub struct ProjectMap {
    pub current: Option<ProjectInfo>,
    pub recent: Vec<ProjectInfo>,
}

#[derive(Debug, Clone)]
pub struct ProjectInfo {
    pub name: String,
    pub path: PathBuf,
    pub project_type: String,
    pub git_remote: String,
    pub git_branch: String,
}

impl ProjectMap {
    pub fn scan() -> Self {
        let current = std::env::current_dir()
            .ok()
            .and_then(|p| ProjectInfo::from_path(&p));

        // Get recent projects from hyle sessions
        let recent = crate::session::list_sessions()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|s| {
                let path = PathBuf::from(&s.working_dir);
                if path.exists() && Some(&path) != current.as_ref().map(|c| &c.path) {
                    Some(ProjectInfo {
                        name: path.file_name()?.to_str()?.to_string(),
                        path,
                        project_type: String::new(),
                        git_remote: String::new(),
                        git_branch: String::new(),
                    })
                } else {
                    None
                }
            })
            .take(10)
            .collect();

        Self { current, recent }
    }
}

impl ProjectInfo {
    pub fn from_path(path: &Path) -> Option<Self> {
        let name = path.file_name()?.to_str()?.to_string();

        // Detect project type
        let project_type = if path.join("Cargo.toml").exists() {
            "Rust"
        } else if path.join("package.json").exists() {
            "Node.js"
        } else if path.join("pyproject.toml").exists() || path.join("setup.py").exists() {
            "Python"
        } else if path.join("go.mod").exists() {
            "Go"
        } else {
            "Unknown"
        };

        // Get git info
        let git_remote = Command::new("git")
            .args(["remote", "get-url", "origin"])
            .current_dir(path)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default();

        let git_branch = Command::new("git")
            .args(["branch", "--show-current"])
            .current_dir(path)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default();

        Some(Self {
            name,
            path: path.to_path_buf(),
            project_type: project_type.to_string(),
            git_remote,
            git_branch,
        })
    }
}

// ═══════════════════════════════════════════════════════════════
// SYSTEM RESOURCES
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Default)]
pub struct SystemResources {
    pub memory_percent: u8,
    pub disk_percent: u8,
    pub load_avg: f32,
}

impl SystemResources {
    pub fn check() -> Self {
        Self {
            memory_percent: Self::get_memory_percent(),
            disk_percent: Self::get_disk_percent(),
            load_avg: Self::get_load_avg(),
        }
    }

    fn get_memory_percent() -> u8 {
        // Read from /proc/meminfo on Linux
        std::fs::read_to_string("/proc/meminfo")
            .ok()
            .and_then(|content| {
                let mut total = 0u64;
                let mut available = 0u64;
                for line in content.lines() {
                    if line.starts_with("MemTotal:") {
                        total = line.split_whitespace().nth(1)?.parse().ok()?;
                    } else if line.starts_with("MemAvailable:") {
                        available = line.split_whitespace().nth(1)?.parse().ok()?;
                    }
                }
                if total > 0 {
                    Some((100 - (available * 100 / total)) as u8)
                } else {
                    None
                }
            })
            .unwrap_or(0)
    }

    fn get_disk_percent() -> u8 {
        Command::new("df")
            .args(["--output=pcent", "."])
            .output()
            .ok()
            .and_then(|o| {
                let output = String::from_utf8_lossy(&o.stdout);
                output.lines()
                    .nth(1)?
                    .trim()
                    .trim_end_matches('%')
                    .parse()
                    .ok()
            })
            .unwrap_or(0)
    }

    fn get_load_avg() -> f32 {
        std::fs::read_to_string("/proc/loadavg")
            .ok()
            .and_then(|s| s.split_whitespace().next()?.parse().ok())
            .unwrap_or(0.0)
    }
}

// ═══════════════════════════════════════════════════════════════
// ACCESS MAP
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Default)]
pub struct AccessMap {
    pub ssh_keys: Vec<String>,
    pub known_hosts: Vec<String>,
    pub gh_authenticated: bool,
}

impl AccessMap {
    pub fn discover() -> Self {
        Self {
            ssh_keys: Self::list_ssh_keys(),
            known_hosts: Self::list_known_hosts(),
            gh_authenticated: crate::github::is_gh_authenticated(),
        }
    }

    fn list_ssh_keys() -> Vec<String> {
        // List keys from ssh-add
        Command::new("ssh-add")
            .arg("-l")
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .lines()
                    .filter_map(|line| {
                        // Format: size fingerprint comment (type)
                        let parts: Vec<&str> = line.split_whitespace().collect();
                        parts.get(2).map(|s| s.to_string())
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn list_known_hosts() -> Vec<String> {
        let home = dirs::home_dir().unwrap_or_default();
        let known_hosts = home.join(".ssh/known_hosts");

        std::fs::read_to_string(known_hosts)
            .ok()
            .map(|content| {
                content.lines()
                    .filter_map(|line| {
                        // Format: hostname,ip key-type key comment
                        line.split_whitespace()
                            .next()
                            .map(|h| h.split(',').next().unwrap_or(h).to_string())
                    })
                    .collect::<std::collections::HashSet<_>>()
                    .into_iter()
                    .collect()
            })
            .unwrap_or_default()
    }
}

// ═══════════════════════════════════════════════════════════════
// RECENT ACTIVITY
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Default)]
pub struct RecentActivity {
    pub session_count: usize,
    pub recent_files: Vec<String>,
    pub recent_commits: Vec<String>,
}

impl RecentActivity {
    pub fn collect() -> Self {
        Self {
            session_count: crate::session::list_sessions()
                .map(|s| s.len())
                .unwrap_or(0),
            recent_files: Self::get_recent_files(),
            recent_commits: Self::get_recent_commits(),
        }
    }

    fn get_recent_files() -> Vec<String> {
        // Get recently modified files in current directory
        Command::new("find")
            .args([".", "-type", "f", "-mmin", "-60", "-not", "-path", "./.git/*"])
            .output()
            .ok()
            .map(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .lines()
                    .take(10)
                    .map(|s| s.trim_start_matches("./").to_string())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn get_recent_commits() -> Vec<String> {
        Command::new("git")
            .args(["log", "--oneline", "-5"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .lines()
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default()
    }
}

// ═══════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_inventory() {
        let tools = ToolInventory::detect();
        // git should be available in most environments
        assert!(tools.tools.contains_key("git"));
    }

    #[test]
    fn test_system_resources() {
        let res = SystemResources::check();
        // Should return reasonable values
        assert!(res.memory_percent <= 100);
        assert!(res.disk_percent <= 100);
    }

    #[test]
    fn test_environment_map_display() {
        let map = EnvironmentMap::gather();
        let display = map.display();
        assert!(display.contains("Environment Map"));
        assert!(display.contains("Tools"));
    }

    #[test]
    fn test_project_info() {
        let cwd = std::env::current_dir().unwrap();
        if let Some(info) = ProjectInfo::from_path(&cwd) {
            assert!(!info.name.is_empty());
        }
    }
}
