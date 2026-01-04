//! GitHub CLI integration
//!
//! Wraps the `gh` CLI tool for GitHub operations.
//! Requires `gh` to be installed and authenticated.

use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

// ═══════════════════════════════════════════════════════════════
// TYPES
// ═══════════════════════════════════════════════════════════════

/// Pull request info
#[derive(Debug, Clone)]
pub struct PullRequest {
    pub number: u64,
    pub title: String,
    pub state: String,
    pub author: String,
    pub branch: String,
    pub url: String,
    pub draft: bool,
}

/// Issue info
#[derive(Debug, Clone)]
pub struct Issue {
    pub number: u64,
    pub title: String,
    pub state: String,
    pub author: String,
    pub labels: Vec<String>,
    pub url: String,
}

/// PR review status
#[derive(Debug, Clone)]
pub struct ReviewStatus {
    pub approved: u32,
    pub changes_requested: u32,
    pub commented: u32,
    pub pending: u32,
}

// ═══════════════════════════════════════════════════════════════
// CHECKS
// ═══════════════════════════════════════════════════════════════

/// Check if gh CLI is available
pub fn is_gh_available() -> bool {
    Command::new("gh")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Check if gh is authenticated
pub fn is_gh_authenticated() -> bool {
    Command::new("gh")
        .args(["auth", "status"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Get current repo info (owner/repo)
pub fn get_repo_info(work_dir: &Path) -> Result<(String, String)> {
    let output = Command::new("gh")
        .args(["repo", "view", "--json", "owner,name"])
        .current_dir(work_dir)
        .output()
        .context("Failed to run gh repo view")?;

    if !output.status.success() {
        anyhow::bail!("Not a GitHub repository or gh not authenticated");
    }

    let json: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let owner = json["owner"]["login"].as_str().unwrap_or("").to_string();
    let name = json["name"].as_str().unwrap_or("").to_string();

    Ok((owner, name))
}

// ═══════════════════════════════════════════════════════════════
// PULL REQUESTS
// ═══════════════════════════════════════════════════════════════

/// List pull requests
pub fn list_prs(work_dir: &Path, state: &str, limit: usize) -> Result<Vec<PullRequest>> {
    let output = Command::new("gh")
        .args([
            "pr",
            "list",
            "--state",
            state,
            "--limit",
            &limit.to_string(),
            "--json",
            "number,title,state,author,headRefName,url,isDraft",
        ])
        .current_dir(work_dir)
        .output()
        .context("Failed to run gh pr list")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("gh pr list failed: {}", stderr);
    }

    let json: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout)?;
    let prs = json
        .iter()
        .map(|pr| PullRequest {
            number: pr["number"].as_u64().unwrap_or(0),
            title: pr["title"].as_str().unwrap_or("").to_string(),
            state: pr["state"].as_str().unwrap_or("").to_string(),
            author: pr["author"]["login"].as_str().unwrap_or("").to_string(),
            branch: pr["headRefName"].as_str().unwrap_or("").to_string(),
            url: pr["url"].as_str().unwrap_or("").to_string(),
            draft: pr["isDraft"].as_bool().unwrap_or(false),
        })
        .collect();

    Ok(prs)
}

/// View a specific PR
pub fn view_pr(work_dir: &Path, pr_number: u64) -> Result<String> {
    let output = Command::new("gh")
        .args(["pr", "view", &pr_number.to_string()])
        .current_dir(work_dir)
        .output()
        .context("Failed to run gh pr view")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("gh pr view failed: {}", stderr);
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Get PR diff
pub fn pr_diff(work_dir: &Path, pr_number: u64) -> Result<String> {
    let output = Command::new("gh")
        .args(["pr", "diff", &pr_number.to_string()])
        .current_dir(work_dir)
        .output()
        .context("Failed to run gh pr diff")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("gh pr diff failed: {}", stderr);
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Create a new PR
pub fn create_pr(
    work_dir: &Path,
    title: &str,
    body: &str,
    base: Option<&str>,
    draft: bool,
) -> Result<String> {
    let mut args = vec!["pr", "create", "--title", title, "--body", body];

    if let Some(base_branch) = base {
        args.push("--base");
        args.push(base_branch);
    }

    if draft {
        args.push("--draft");
    }

    let output = Command::new("gh")
        .args(&args)
        .current_dir(work_dir)
        .output()
        .context("Failed to run gh pr create")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("gh pr create failed: {}", stderr);
    }

    // Output contains the PR URL
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Get PR review status
pub fn pr_review_status(work_dir: &Path, pr_number: u64) -> Result<ReviewStatus> {
    let output = Command::new("gh")
        .args(["pr", "view", &pr_number.to_string(), "--json", "reviews"])
        .current_dir(work_dir)
        .output()
        .context("Failed to run gh pr view")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("gh pr view failed: {}", stderr);
    }

    let json: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let empty_vec = vec![];
    let reviews = json["reviews"].as_array().unwrap_or(&empty_vec);

    let mut status = ReviewStatus {
        approved: 0,
        changes_requested: 0,
        commented: 0,
        pending: 0,
    };

    for review in reviews {
        match review["state"].as_str().unwrap_or("") {
            "APPROVED" => status.approved += 1,
            "CHANGES_REQUESTED" => status.changes_requested += 1,
            "COMMENTED" => status.commented += 1,
            "PENDING" => status.pending += 1,
            _ => {}
        }
    }

    Ok(status)
}

/// Checkout a PR branch locally
pub fn checkout_pr(work_dir: &Path, pr_number: u64) -> Result<()> {
    let output = Command::new("gh")
        .args(["pr", "checkout", &pr_number.to_string()])
        .current_dir(work_dir)
        .output()
        .context("Failed to run gh pr checkout")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("gh pr checkout failed: {}", stderr);
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════
// ISSUES
// ═══════════════════════════════════════════════════════════════

/// List issues
pub fn list_issues(work_dir: &Path, state: &str, limit: usize) -> Result<Vec<Issue>> {
    let output = Command::new("gh")
        .args([
            "issue",
            "list",
            "--state",
            state,
            "--limit",
            &limit.to_string(),
            "--json",
            "number,title,state,author,labels,url",
        ])
        .current_dir(work_dir)
        .output()
        .context("Failed to run gh issue list")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("gh issue list failed: {}", stderr);
    }

    let json: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout)?;
    let issues = json
        .iter()
        .map(|issue| {
            let labels = issue["labels"]
                .as_array()
                .unwrap_or(&vec![])
                .iter()
                .filter_map(|l| l["name"].as_str().map(String::from))
                .collect();

            Issue {
                number: issue["number"].as_u64().unwrap_or(0),
                title: issue["title"].as_str().unwrap_or("").to_string(),
                state: issue["state"].as_str().unwrap_or("").to_string(),
                author: issue["author"]["login"].as_str().unwrap_or("").to_string(),
                labels,
                url: issue["url"].as_str().unwrap_or("").to_string(),
            }
        })
        .collect();

    Ok(issues)
}

/// View a specific issue
pub fn view_issue(work_dir: &Path, issue_number: u64) -> Result<String> {
    let output = Command::new("gh")
        .args(["issue", "view", &issue_number.to_string()])
        .current_dir(work_dir)
        .output()
        .context("Failed to run gh issue view")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("gh issue view failed: {}", stderr);
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Create a new issue
pub fn create_issue(work_dir: &Path, title: &str, body: &str, labels: &[&str]) -> Result<String> {
    let mut args = vec!["issue", "create", "--title", title, "--body", body];

    for label in labels {
        args.push("--label");
        args.push(label);
    }

    let output = Command::new("gh")
        .args(&args)
        .current_dir(work_dir)
        .output()
        .context("Failed to run gh issue create")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("gh issue create failed: {}", stderr);
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

// ═══════════════════════════════════════════════════════════════
// WORKFLOW / ACTIONS
// ═══════════════════════════════════════════════════════════════

/// List recent workflow runs
pub fn list_runs(work_dir: &Path, limit: usize) -> Result<String> {
    let output = Command::new("gh")
        .args(["run", "list", "--limit", &limit.to_string()])
        .current_dir(work_dir)
        .output()
        .context("Failed to run gh run list")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("gh run list failed: {}", stderr);
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// View workflow run details
pub fn view_run(work_dir: &Path, run_id: u64) -> Result<String> {
    let output = Command::new("gh")
        .args(["run", "view", &run_id.to_string()])
        .current_dir(work_dir)
        .output()
        .context("Failed to run gh run view")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("gh run view failed: {}", stderr);
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

// ═══════════════════════════════════════════════════════════════
// HELPER: Format for display
// ═══════════════════════════════════════════════════════════════

impl PullRequest {
    pub fn display(&self) -> String {
        let draft = if self.draft { " [DRAFT]" } else { "" };
        format!(
            "#{} {} by @{}{}\n  {} → {}\n  {}",
            self.number, self.title, self.author, draft, self.branch, self.state, self.url
        )
    }
}

impl Issue {
    pub fn display(&self) -> String {
        let labels = if self.labels.is_empty() {
            String::new()
        } else {
            format!(" [{}]", self.labels.join(", "))
        };
        format!(
            "#{} {} by @{}{}\n  {} {}",
            self.number, self.title, self.author, labels, self.state, self.url
        )
    }
}

impl ReviewStatus {
    pub fn display(&self) -> String {
        format!(
            "Reviews: {} approved, {} changes requested, {} commented, {} pending",
            self.approved, self.changes_requested, self.commented, self.pending
        )
    }
}

// ═══════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_gh_available() {
        // Just test that it doesn't panic
        let _ = is_gh_available();
    }

    #[test]
    fn test_pr_display() {
        let pr = PullRequest {
            number: 42,
            title: "Add feature".to_string(),
            state: "OPEN".to_string(),
            author: "user".to_string(),
            branch: "feature".to_string(),
            url: "https://github.com/org/repo/pull/42".to_string(),
            draft: false,
        };
        let display = pr.display();
        assert!(display.contains("#42"));
        assert!(display.contains("Add feature"));
        assert!(display.contains("@user"));
    }

    #[test]
    fn test_pr_display_draft() {
        let pr = PullRequest {
            number: 1,
            title: "WIP".to_string(),
            state: "OPEN".to_string(),
            author: "dev".to_string(),
            branch: "wip".to_string(),
            url: "url".to_string(),
            draft: true,
        };
        assert!(pr.display().contains("[DRAFT]"));
    }

    #[test]
    fn test_issue_display() {
        let issue = Issue {
            number: 10,
            title: "Bug report".to_string(),
            state: "OPEN".to_string(),
            author: "reporter".to_string(),
            labels: vec!["bug".to_string(), "priority".to_string()],
            url: "https://github.com/org/repo/issues/10".to_string(),
        };
        let display = issue.display();
        assert!(display.contains("#10"));
        assert!(display.contains("bug, priority"));
    }

    #[test]
    fn test_review_status_display() {
        let status = ReviewStatus {
            approved: 2,
            changes_requested: 1,
            commented: 3,
            pending: 0,
        };
        let display = status.display();
        assert!(display.contains("2 approved"));
        assert!(display.contains("1 changes requested"));
    }
}
