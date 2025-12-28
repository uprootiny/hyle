//! Git integration for atomic commits and hygiene
//!
//! Provides:
//! - Status parsing (modified, added, deleted)
//! - Diff generation
//! - Atomic commit creation with message validation
//! - Branch management

#![allow(dead_code)] // Forward-looking module for git operations

use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

// ═══════════════════════════════════════════════════════════════
// GIT STATUS
// ═══════════════════════════════════════════════════════════════

/// Represents a file's status in git
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileStatus {
    Modified,
    Added,
    Deleted,
    Renamed,
    Copied,
    Untracked,
    Ignored,
    Unmerged,
}

/// A single file change in the working tree
#[derive(Debug, Clone)]
pub struct FileChange {
    pub path: String,
    pub status: FileStatus,
    pub staged: bool,
    pub old_path: Option<String>, // For renames
}

/// Result of parsing git status
#[derive(Debug, Clone, Default)]
pub struct GitStatus {
    pub changes: Vec<FileChange>,
    pub branch: Option<String>,
    pub ahead: usize,
    pub behind: usize,
    pub is_clean: bool,
}

impl GitStatus {
    /// Get all staged changes
    pub fn staged(&self) -> Vec<&FileChange> {
        self.changes.iter().filter(|c| c.staged).collect()
    }

    /// Get all unstaged changes
    pub fn unstaged(&self) -> Vec<&FileChange> {
        self.changes.iter().filter(|c| !c.staged).collect()
    }

    /// Get all untracked files
    pub fn untracked(&self) -> Vec<&FileChange> {
        self.changes.iter()
            .filter(|c| c.status == FileStatus::Untracked)
            .collect()
    }

    /// Get modified (non-untracked) unstaged changes
    pub fn modified_unstaged(&self) -> Vec<&FileChange> {
        self.changes.iter()
            .filter(|c| !c.staged && c.status != FileStatus::Untracked)
            .collect()
    }

    /// Summary for display
    pub fn summary(&self) -> String {
        let staged = self.staged().len();
        let modified = self.modified_unstaged().len();
        let untracked = self.untracked().len();

        if self.is_clean {
            return "Clean".to_string();
        }

        let mut parts = Vec::new();
        if staged > 0 {
            parts.push(format!("+{}", staged));
        }
        if modified > 0 {
            parts.push(format!("~{}", modified));
        }
        if untracked > 0 {
            parts.push(format!("?{}", untracked));
        }

        parts.join(" ")
    }
}

/// Check if a directory is a git repository
pub fn is_git_repo(path: &Path) -> bool {
    path.join(".git").exists() ||
    Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .current_dir(path)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Parse git status output
pub fn parse_status(work_dir: &Path) -> Result<GitStatus> {
    let output = Command::new("git")
        .args(["status", "--porcelain=v2", "--branch"])
        .current_dir(work_dir)
        .output()
        .context("Failed to run git status")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git status failed: {}", stderr);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_status_output(&stdout)
}

/// Parse the porcelain v2 output
fn parse_status_output(output: &str) -> Result<GitStatus> {
    let mut status = GitStatus::default();

    for line in output.lines() {
        if line.starts_with("# branch.head ") {
            status.branch = Some(line[14..].to_string());
        } else if line.starts_with("# branch.ab ") {
            // Parse "+N -M" format
            let parts: Vec<&str> = line[12..].split_whitespace().collect();
            if let Some(ahead) = parts.first() {
                status.ahead = ahead.trim_start_matches('+').parse().unwrap_or(0);
            }
            if let Some(behind) = parts.get(1) {
                status.behind = behind.trim_start_matches('-').parse().unwrap_or(0);
            }
        } else if line.starts_with("1 ") || line.starts_with("2 ") {
            // Changed entry
            if let Some(change) = parse_change_line(line) {
                status.changes.push(change);
            }
        } else if line.starts_with("? ") {
            // Untracked file
            status.changes.push(FileChange {
                path: line[2..].to_string(),
                status: FileStatus::Untracked,
                staged: false,
                old_path: None,
            });
        }
    }

    status.is_clean = status.changes.is_empty();
    Ok(status)
}

/// Parse a single change line from porcelain v2
fn parse_change_line(line: &str) -> Option<FileChange> {
    let parts: Vec<&str> = line.split_whitespace().collect();

    // Format: "1 XY sub mH mI mW hH hI path" or "2 XY sub mH mI mW hH hI X score path\torigPath"
    if parts.len() < 9 {
        return None;
    }

    let xy = parts[1];
    let path = if line.starts_with("2 ") {
        // Rename/copy - path is after the score
        parts.get(10).map(|s| s.to_string())?
    } else {
        parts[8].to_string()
    };

    let index_status = xy.chars().next()?;
    let worktree_status = xy.chars().nth(1)?;

    // Determine status from XY codes
    let (status, staged) = match (index_status, worktree_status) {
        ('M', _) => (FileStatus::Modified, true),
        (_, 'M') => (FileStatus::Modified, false),
        ('A', _) => (FileStatus::Added, true),
        ('D', _) => (FileStatus::Deleted, true),
        (_, 'D') => (FileStatus::Deleted, false),
        ('R', _) => (FileStatus::Renamed, true),
        ('C', _) => (FileStatus::Copied, true),
        ('U', _) | (_, 'U') => (FileStatus::Unmerged, false),
        _ => (FileStatus::Modified, index_status != '.'),
    };

    Some(FileChange {
        path,
        status,
        staged,
        old_path: None,
    })
}

// ═══════════════════════════════════════════════════════════════
// GIT DIFF
// ═══════════════════════════════════════════════════════════════

/// Get diff for staged changes
pub fn get_staged_diff(work_dir: &Path) -> Result<String> {
    let output = Command::new("git")
        .args(["diff", "--cached"])
        .current_dir(work_dir)
        .output()
        .context("Failed to run git diff")?;

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Get diff for unstaged changes
pub fn get_unstaged_diff(work_dir: &Path) -> Result<String> {
    let output = Command::new("git")
        .args(["diff"])
        .current_dir(work_dir)
        .output()
        .context("Failed to run git diff")?;

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Get diff for a specific file
pub fn get_file_diff(work_dir: &Path, file: &str) -> Result<String> {
    let output = Command::new("git")
        .args(["diff", file])
        .current_dir(work_dir)
        .output()
        .context("Failed to run git diff")?;

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

// ═══════════════════════════════════════════════════════════════
// COMMIT MESSAGE VALIDATION
// ═══════════════════════════════════════════════════════════════

/// Validation result for commit message
#[derive(Debug, Clone)]
pub struct MessageValidation {
    pub valid: bool,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

impl MessageValidation {
    fn new() -> Self {
        Self {
            valid: true,
            warnings: Vec::new(),
            errors: Vec::new(),
        }
    }

    fn warn(&mut self, msg: &str) {
        self.warnings.push(msg.to_string());
    }

    fn error(&mut self, msg: &str) {
        self.valid = false;
        self.errors.push(msg.to_string());
    }
}

/// Validate a commit message
pub fn validate_commit_message(msg: &str) -> MessageValidation {
    let mut result = MessageValidation::new();

    // Must not be empty
    if msg.trim().is_empty() {
        result.error("Commit message cannot be empty");
        return result;
    }

    let lines: Vec<&str> = msg.lines().collect();
    let subject = lines[0];

    // Subject line checks
    if subject.len() > 72 {
        result.warn(&format!("Subject line too long ({} chars, max 72)", subject.len()));
    }

    if subject.len() < 10 {
        result.warn("Subject line too short (min 10 chars recommended)");
    }

    // Should not end with period
    if subject.ends_with('.') {
        result.warn("Subject should not end with a period");
    }

    // Should start with capital letter
    if subject.chars().next().map(|c| c.is_lowercase()).unwrap_or(false) {
        result.warn("Subject should start with a capital letter");
    }

    // Check for imperative mood (common non-imperative starters)
    let non_imperative = ["added", "fixed", "updated", "removed", "changed", "implemented"];
    let first_word = subject.split_whitespace().next().unwrap_or("").to_lowercase();
    if non_imperative.contains(&first_word.as_str()) {
        result.warn("Use imperative mood (e.g., 'Add' instead of 'Added')");
    }

    // Check for WIP markers
    if subject.to_lowercase().contains("wip") {
        result.warn("Contains WIP marker - should not commit work in progress");
    }

    // Body checks
    if lines.len() > 1
        && !lines[1].is_empty() {
            result.warn("Second line should be blank (separates subject from body)");
        }

    result
}

// ═══════════════════════════════════════════════════════════════
// ATOMIC COMMITS
// ═══════════════════════════════════════════════════════════════

/// Stage a single file
pub fn stage_file(work_dir: &Path, file: &str) -> Result<()> {
    stage_files(work_dir, &[file])
}

/// Stage files for commit
pub fn stage_files(work_dir: &Path, files: &[&str]) -> Result<()> {
    if files.is_empty() {
        return Ok(());
    }

    let mut cmd = Command::new("git");
    cmd.arg("add").current_dir(work_dir);

    for file in files {
        cmd.arg(file);
    }

    let output = cmd.output().context("Failed to run git add")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git add failed: {}", stderr);
    }

    Ok(())
}

/// Stage all changes
pub fn stage_all(work_dir: &Path) -> Result<()> {
    let output = Command::new("git")
        .args(["add", "-A"])
        .current_dir(work_dir)
        .output()
        .context("Failed to run git add")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git add failed: {}", stderr);
    }

    Ok(())
}

/// Create a commit with the given message
pub fn commit(work_dir: &Path, message: &str) -> Result<String> {
    // Validate message first
    let validation = validate_commit_message(message);
    if !validation.valid {
        anyhow::bail!("Invalid commit message: {}", validation.errors.join(", "));
    }

    let output = Command::new("git")
        .args(["commit", "-m", message])
        .current_dir(work_dir)
        .output()
        .context("Failed to run git commit")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git commit failed: {}", stderr);
    }

    // Extract commit hash from output
    let stdout = String::from_utf8_lossy(&output.stdout);
    let hash = stdout.split_whitespace()
        .find(|s| s.len() >= 7 && s.chars().all(|c| c.is_ascii_hexdigit()))
        .unwrap_or("unknown")
        .to_string();

    Ok(hash)
}

/// Get recent commit messages for style reference
pub fn get_recent_commits(work_dir: &Path, count: usize) -> Result<Vec<String>> {
    let output = Command::new("git")
        .args(["log", &format!("-{}", count), "--format=%s"])
        .current_dir(work_dir)
        .output()
        .context("Failed to run git log")?;

    if !output.status.success() {
        return Ok(Vec::new());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.lines().map(|s| s.to_string()).collect())
}

/// Check if there are staged changes
pub fn has_staged_changes(work_dir: &Path) -> Result<bool> {
    let output = Command::new("git")
        .args(["diff", "--cached", "--quiet"])
        .current_dir(work_dir)
        .output()
        .context("Failed to run git diff")?;

    // Exit code 0 = no changes, 1 = changes
    Ok(!output.status.success())
}

// ═══════════════════════════════════════════════════════════════
// BRANCH OPERATIONS
// ═══════════════════════════════════════════════════════════════

/// Get current branch name
pub fn current_branch(work_dir: &Path) -> Result<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(work_dir)
        .output()
        .context("Failed to get current branch")?;

    if !output.status.success() {
        anyhow::bail!("Not in a git repository");
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Create and checkout a new branch
pub fn create_branch(work_dir: &Path, name: &str) -> Result<()> {
    let output = Command::new("git")
        .args(["checkout", "-b", name])
        .current_dir(work_dir)
        .output()
        .context("Failed to create branch")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git checkout -b failed: {}", stderr);
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_status_enum() {
        assert_eq!(FileStatus::Modified, FileStatus::Modified);
        assert_ne!(FileStatus::Added, FileStatus::Deleted);
    }

    #[test]
    fn test_parse_status_output_clean() {
        let output = "# branch.head master\n# branch.ab +0 -0\n";
        let status = parse_status_output(output).unwrap();

        assert!(status.is_clean);
        assert_eq!(status.branch, Some("master".to_string()));
        assert_eq!(status.ahead, 0);
        assert_eq!(status.behind, 0);
    }

    #[test]
    fn test_parse_status_output_with_changes() {
        let output = r#"# branch.head main
# branch.ab +2 -1
1 .M N... 100644 100644 100644 abc123 def456 src/main.rs
? new_file.txt
"#;
        let status = parse_status_output(output).unwrap();

        assert!(!status.is_clean);
        assert_eq!(status.branch, Some("main".to_string()));
        assert_eq!(status.ahead, 2);
        assert_eq!(status.behind, 1);
        assert_eq!(status.changes.len(), 2);
    }

    #[test]
    fn test_parse_status_output_untracked() {
        let output = "? untracked.txt\n";
        let status = parse_status_output(output).unwrap();

        assert_eq!(status.changes.len(), 1);
        assert_eq!(status.changes[0].status, FileStatus::Untracked);
        assert_eq!(status.changes[0].path, "untracked.txt");
    }

    #[test]
    fn test_git_status_summary_clean() {
        let status = GitStatus {
            is_clean: true,
            ..Default::default()
        };
        assert_eq!(status.summary(), "Clean");
    }

    #[test]
    fn test_git_status_summary_mixed() {
        let status = GitStatus {
            changes: vec![
                FileChange {
                    path: "a.rs".into(),
                    status: FileStatus::Modified,
                    staged: true,
                    old_path: None,
                },
                FileChange {
                    path: "b.rs".into(),
                    status: FileStatus::Modified,
                    staged: false,
                    old_path: None,
                },
                FileChange {
                    path: "c.rs".into(),
                    status: FileStatus::Untracked,
                    staged: false,
                    old_path: None,
                },
            ],
            is_clean: false,
            ..Default::default()
        };

        let summary = status.summary();
        assert!(summary.contains("+1")); // staged
        assert!(summary.contains("~1")); // modified unstaged (not untracked)
        assert!(summary.contains("?1")); // untracked
    }

    #[test]
    fn test_validate_commit_message_valid() {
        let result = validate_commit_message("Add new feature for user authentication");

        assert!(result.valid);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_validate_commit_message_empty() {
        let result = validate_commit_message("");

        assert!(!result.valid);
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn test_validate_commit_message_too_long() {
        let long_msg = "A".repeat(100);
        let result = validate_commit_message(&long_msg);

        assert!(result.valid); // Warning, not error
        assert!(!result.warnings.is_empty());
        assert!(result.warnings[0].contains("too long"));
    }

    #[test]
    fn test_validate_commit_message_ends_with_period() {
        let result = validate_commit_message("Add new feature.");

        assert!(result.valid);
        assert!(result.warnings.iter().any(|w| w.contains("period")));
    }

    #[test]
    fn test_validate_commit_message_non_imperative() {
        let result = validate_commit_message("Added new feature");

        assert!(result.valid);
        assert!(result.warnings.iter().any(|w| w.contains("imperative")));
    }

    #[test]
    fn test_validate_commit_message_wip() {
        let result = validate_commit_message("WIP: working on feature");

        assert!(result.valid);
        assert!(result.warnings.iter().any(|w| w.contains("WIP")));
    }

    #[test]
    fn test_validate_commit_message_lowercase() {
        let result = validate_commit_message("add new feature");

        assert!(result.valid);
        assert!(result.warnings.iter().any(|w| w.contains("capital")));
    }

    #[test]
    fn test_validate_commit_message_missing_blank_line() {
        let result = validate_commit_message("Add feature\nThis is the body without blank line");

        assert!(result.valid);
        assert!(result.warnings.iter().any(|w| w.contains("blank")));
    }

    #[test]
    fn test_validate_commit_message_with_body() {
        let msg = "Add feature\n\nThis is the body with proper blank line.";
        let result = validate_commit_message(msg);

        assert!(result.valid);
        // Should not have the blank line warning
        assert!(!result.warnings.iter().any(|w| w.contains("blank")));
    }

    #[test]
    fn test_is_git_repo_false() {
        // /tmp should not be a git repo
        assert!(!is_git_repo(Path::new("/tmp")));
    }

    #[test]
    fn test_file_change_creation() {
        let change = FileChange {
            path: "src/main.rs".to_string(),
            status: FileStatus::Modified,
            staged: true,
            old_path: None,
        };

        assert_eq!(change.path, "src/main.rs");
        assert_eq!(change.status, FileStatus::Modified);
        assert!(change.staged);
    }

    #[test]
    fn test_git_status_staged_filter() {
        let status = GitStatus {
            changes: vec![
                FileChange {
                    path: "staged.rs".into(),
                    status: FileStatus::Modified,
                    staged: true,
                    old_path: None,
                },
                FileChange {
                    path: "unstaged.rs".into(),
                    status: FileStatus::Modified,
                    staged: false,
                    old_path: None,
                },
            ],
            is_clean: false,
            ..Default::default()
        };

        assert_eq!(status.staged().len(), 1);
        assert_eq!(status.staged()[0].path, "staged.rs");
        assert_eq!(status.unstaged().len(), 1);
        assert_eq!(status.unstaged()[0].path, "unstaged.rs");
    }

    #[test]
    fn test_message_validation_short() {
        let result = validate_commit_message("Fix");

        assert!(result.valid);
        assert!(result.warnings.iter().any(|w| w.contains("short")));
    }

    #[test]
    fn test_parse_change_line_modified() {
        let line = "1 .M N... 100644 100644 100644 abc123 def456 src/main.rs";
        let change = parse_change_line(line).unwrap();

        assert_eq!(change.path, "src/main.rs");
        assert_eq!(change.status, FileStatus::Modified);
        assert!(!change.staged);
    }

    #[test]
    fn test_parse_change_line_staged() {
        let line = "1 M. N... 100644 100644 100644 abc123 def456 src/lib.rs";
        let change = parse_change_line(line).unwrap();

        assert_eq!(change.path, "src/lib.rs");
        assert_eq!(change.status, FileStatus::Modified);
        assert!(change.staged);
    }
}
