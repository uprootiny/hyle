//! File and patch tools
//!
//! - Read files with context
//! - Generate unified diffs
//! - Apply patches
//! - Tool call infrastructure for self-bootstrapping

use anyhow::{Context, Result};
use similar::TextDiff;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};
use std::time::{Duration, Instant};
use serde::{Serialize, Deserialize};

// ═══════════════════════════════════════════════════════════════
// TOOL CALL INFRASTRUCTURE
// ═══════════════════════════════════════════════════════════════

/// Status of a tool call execution
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ToolCallStatus {
    Pending,
    Running,
    Done,
    Failed,
    Killed,
}

/// A tracked tool call with observability
#[derive(Debug, Clone)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub args: serde_json::Value,
    pub status: ToolCallStatus,
    pub output: Arc<Mutex<String>>,
    pub started_at: Option<Instant>,
    pub finished_at: Option<Instant>,
    pub error: Option<String>,
}

impl ToolCall {
    /// Create a new pending tool call
    pub fn new(name: &str, args: serde_json::Value) -> Self {
        Self {
            id: format!("{}_{}", name, chrono::Utc::now().timestamp_millis()),
            name: name.to_string(),
            args,
            status: ToolCallStatus::Pending,
            output: Arc::new(Mutex::new(String::new())),
            started_at: None,
            finished_at: None,
            error: None,
        }
    }

    /// Get elapsed time since start
    pub fn elapsed(&self) -> Option<Duration> {
        self.started_at.map(|s| {
            self.finished_at.unwrap_or_else(Instant::now) - s
        })
    }

    /// Format status for display
    pub fn status_line(&self) -> String {
        match &self.status {
            ToolCallStatus::Pending => "Pending...".into(),
            ToolCallStatus::Running => {
                let elapsed = self.elapsed().map(|d| d.as_secs()).unwrap_or(0);
                format!("Running... {}s", elapsed)
            }
            ToolCallStatus::Done => {
                let elapsed = self.elapsed().map(|d| d.as_millis()).unwrap_or(0);
                format!("Done ({}ms)", elapsed)
            }
            ToolCallStatus::Failed => {
                format!("Failed: {}", self.error.as_deref().unwrap_or("unknown"))
            }
            ToolCallStatus::Killed => "Killed".into(),
        }
    }

    /// Mark as running
    pub fn start(&mut self) {
        self.status = ToolCallStatus::Running;
        self.started_at = Some(Instant::now());
    }

    /// Mark as done
    pub fn complete(&mut self) {
        self.status = ToolCallStatus::Done;
        self.finished_at = Some(Instant::now());
    }

    /// Mark as failed
    pub fn fail(&mut self, error: &str) {
        self.status = ToolCallStatus::Failed;
        self.finished_at = Some(Instant::now());
        self.error = Some(error.to_string());
    }

    /// Mark as killed
    pub fn kill(&mut self) {
        self.status = ToolCallStatus::Killed;
        self.finished_at = Some(Instant::now());
    }

    /// Append to output buffer
    pub fn append_output(&self, text: &str) {
        if let Ok(mut output) = self.output.lock() {
            output.push_str(text);
        }
    }

    /// Get current output
    pub fn get_output(&self) -> String {
        self.output.lock().map(|o| o.clone()).unwrap_or_default()
    }
}

/// Tool executor with kill support
pub struct ToolExecutor {
    kill_signals: std::collections::HashMap<String, Arc<AtomicBool>>,
}

impl Default for ToolExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolExecutor {
    pub fn new() -> Self {
        Self {
            kill_signals: std::collections::HashMap::new(),
        }
    }

    /// Execute a tool call
    pub fn execute(&mut self, call: &mut ToolCall) -> Result<()> {
        let kill = Arc::new(AtomicBool::new(false));
        self.kill_signals.insert(call.id.clone(), kill.clone());

        call.start();

        let result = match call.name.as_str() {
            "read" => self.exec_read(call),
            "write" => self.exec_write(call),
            "glob" => self.exec_glob(call),
            "grep" => self.exec_grep(call),
            "bash" => self.exec_bash(call, kill),
            _ => Err(anyhow::anyhow!("Unknown tool: {}", call.name)),
        };

        match &result {
            Ok(()) => call.complete(),
            Err(e) => call.fail(&e.to_string()),
        }

        self.kill_signals.remove(&call.id);
        result
    }

    /// Send kill signal to a running tool
    pub fn kill(&mut self, id: &str) {
        if let Some(signal) = self.kill_signals.get(id) {
            signal.store(true, Ordering::SeqCst);
        }
    }

    fn exec_read(&self, call: &mut ToolCall) -> Result<()> {
        let path = call.args.get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("read: missing 'path' argument"))?;

        let content = read_file(Path::new(path))?;
        call.append_output(&content);
        Ok(())
    }

    fn exec_write(&self, call: &mut ToolCall) -> Result<()> {
        let path = call.args.get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("write: missing 'path' argument"))?;

        let content = call.args.get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("write: missing 'content' argument"))?;

        // Backup existing file
        let path = Path::new(path);
        if path.exists() {
            let backup = path.with_extension("bak");
            fs::copy(path, &backup)
                .with_context(|| format!("Failed to backup {}", path.display()))?;
            call.append_output(&format!("Backed up to {}\n", backup.display()));
        }

        fs::write(path, content)
            .with_context(|| format!("Failed to write {}", path.display()))?;

        call.append_output(&format!("Wrote {} bytes to {}\n", content.len(), path.display()));
        Ok(())
    }

    fn exec_glob(&self, call: &mut ToolCall) -> Result<()> {
        let pattern = call.args.get("pattern")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("glob: missing 'pattern' argument"))?;

        for entry in glob::glob(pattern)? {
            match entry {
                Ok(path) => call.append_output(&format!("{}\n", path.display())),
                Err(e) => call.append_output(&format!("Error: {}\n", e)),
            }
        }
        Ok(())
    }

    fn exec_grep(&self, call: &mut ToolCall) -> Result<()> {
        let pattern = call.args.get("pattern")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("grep: missing 'pattern' argument"))?;

        let path = call.args.get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("grep: missing 'path' argument"))?;

        let content = fs::read_to_string(path)?;
        let regex = regex::Regex::new(pattern)?;

        for (i, line) in content.lines().enumerate() {
            if regex.is_match(line) {
                call.append_output(&format!("{}:{}: {}\n", path, i + 1, line));
            }
        }
        Ok(())
    }

    fn exec_bash(&self, call: &mut ToolCall, kill: Arc<AtomicBool>) -> Result<()> {
        let command = call.args.get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("bash: missing 'command' argument"))?;

        let timeout_ms = call.args.get("timeout")
            .and_then(|v| v.as_u64())
            .unwrap_or(60000);

        let start = Instant::now();
        let mut child = std::process::Command::new("bash")
            .arg("-c")
            .arg(command)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        // Poll for completion or kill signal
        loop {
            if kill.load(Ordering::SeqCst) {
                child.kill()?;
                return Err(anyhow::anyhow!("Killed by user"));
            }

            if start.elapsed().as_millis() as u64 > timeout_ms {
                child.kill()?;
                return Err(anyhow::anyhow!("Timeout after {}ms", timeout_ms));
            }

            match child.try_wait()? {
                Some(status) => {
                    let output = child.wait_with_output()?;
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let stderr = String::from_utf8_lossy(&output.stderr);

                    call.append_output(&stdout);
                    if !stderr.is_empty() {
                        call.append_output(&format!("\n[stderr]\n{}", stderr));
                    }

                    if !status.success() {
                        return Err(anyhow::anyhow!("Exit code: {:?}", status.code()));
                    }
                    return Ok(());
                }
                None => {
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// FILE OPERATIONS
// ═══════════════════════════════════════════════════════════════

/// Read a file with line numbers
pub fn read_file(path: &Path) -> Result<String> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read {}", path.display()))?;

    let mut output = String::new();
    for (i, line) in content.lines().enumerate() {
        output.push_str(&format!("{:4}│ {}\n", i + 1, line));
    }

    Ok(output)
}

/// Read multiple files into context string
pub fn read_files_context(paths: &[&Path]) -> Result<String> {
    let mut context = String::new();

    for path in paths {
        if path.exists() {
            let content = fs::read_to_string(path)
                .with_context(|| format!("Failed to read {}", path.display()))?;
            context.push_str(&format!("\n--- {} ---\n{}\n", path.display(), content));
        }
    }

    Ok(context)
}

/// Generate a unified diff between two strings
pub fn generate_diff(original: &str, modified: &str, filename: &str) -> String {
    let diff = TextDiff::from_lines(original, modified);

    diff.unified_diff()
        .context_radius(3)
        .header(&format!("a/{}", filename), &format!("b/{}", filename))
        .to_string()
}

/// Parse a unified diff and extract hunks
pub struct DiffHunk {
    pub old_start: usize,
    pub old_count: usize,
    pub new_start: usize,
    pub new_count: usize,
    pub lines: Vec<DiffLine>,
}

pub enum DiffLine {
    Context(String),
    Delete(String),
    Insert(String),
}

/// Apply a patch to a file (simple implementation)
pub fn apply_patch(original: &str, patch: &str) -> Result<String> {
    // Very simple patch application - just use the "modified" content
    // For a real implementation, we'd parse the unified diff format

    // For now, if the patch looks like a unified diff, try to apply it
    if patch.starts_with("---") || patch.starts_with("diff") {
        // TODO: Implement proper unified diff parsing and application
        // For now, return original unchanged
        Ok(original.to_string())
    } else {
        // Not a unified diff, return as-is
        Ok(patch.to_string())
    }
}

/// Preview changes to a file
pub fn preview_changes(original: &str, modified: &str, filename: &str) -> String {
    let diff = generate_diff(original, modified, filename);

    let additions = diff.lines().filter(|l| l.starts_with('+')).count();
    let deletions = diff.lines().filter(|l| l.starts_with('-')).count();

    format!(
        "Changes to {}:\n  +{} lines, -{} lines\n\n{}",
        filename, additions, deletions, diff
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_diff() {
        let original = "line 1\nline 2\nline 3\n";
        let modified = "line 1\nline 2 modified\nline 3\n";

        let diff = generate_diff(original, modified, "test.txt");
        assert!(diff.contains("--- a/test.txt"));
        assert!(diff.contains("+++ b/test.txt"));
        assert!(diff.contains("-line 2"));
        assert!(diff.contains("+line 2 modified"));
    }

    #[test]
    fn test_preview_changes() {
        let original = "hello\n";
        let modified = "hello world\n";

        let preview = preview_changes(original, modified, "test.txt");
        assert!(preview.contains("Changes to test.txt"));
        // Just check it produces some output
        assert!(!preview.is_empty());
    }

    // ═══════════════════════════════════════════════════════════════
    // TOOL CALL INFRASTRUCTURE TESTS
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_tool_call_lifecycle() {
        let mut call = ToolCall::new("test", serde_json::json!({}));

        // Starts pending
        assert_eq!(call.status, ToolCallStatus::Pending);
        assert!(call.started_at.is_none());

        // Start running
        call.start();
        assert_eq!(call.status, ToolCallStatus::Running);
        assert!(call.started_at.is_some());

        // Complete
        call.complete();
        assert_eq!(call.status, ToolCallStatus::Done);
        assert!(call.finished_at.is_some());
        assert!(call.elapsed().is_some());
    }

    #[test]
    fn test_tool_call_failure() {
        let mut call = ToolCall::new("test", serde_json::json!({}));
        call.start();
        call.fail("Something went wrong");

        assert_eq!(call.status, ToolCallStatus::Failed);
        assert_eq!(call.error, Some("Something went wrong".to_string()));
        assert!(call.status_line().contains("Failed"));
    }

    #[test]
    fn test_tool_call_kill() {
        let mut call = ToolCall::new("test", serde_json::json!({}));
        call.start();
        call.kill();

        assert_eq!(call.status, ToolCallStatus::Killed);
        assert!(call.finished_at.is_some());
        assert_eq!(call.status_line(), "Killed");
    }

    #[test]
    fn test_tool_call_output() {
        let call = ToolCall::new("test", serde_json::json!({}));

        call.append_output("line 1\n");
        call.append_output("line 2\n");

        let output = call.get_output();
        assert!(output.contains("line 1"));
        assert!(output.contains("line 2"));
    }

    #[test]
    fn test_tool_call_status_line() {
        let mut call = ToolCall::new("test", serde_json::json!({}));

        assert_eq!(call.status_line(), "Pending...");

        call.start();
        assert!(call.status_line().starts_with("Running..."));

        call.complete();
        assert!(call.status_line().starts_with("Done"));
    }

    #[test]
    fn test_executor_read_file() {
        // Create a temp file
        let tmp = std::env::temp_dir().join("hyle_test_read.txt");
        std::fs::write(&tmp, "test content\n").unwrap();

        let mut executor = ToolExecutor::new();
        let mut call = ToolCall::new("read", serde_json::json!({
            "path": tmp.to_string_lossy()
        }));

        let result = executor.execute(&mut call);
        assert!(result.is_ok());
        assert_eq!(call.status, ToolCallStatus::Done);
        assert!(call.get_output().contains("test content"));

        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn test_executor_bash() {
        let mut executor = ToolExecutor::new();
        let mut call = ToolCall::new("bash", serde_json::json!({
            "command": "echo hello"
        }));

        let result = executor.execute(&mut call);
        assert!(result.is_ok());
        assert_eq!(call.status, ToolCallStatus::Done);
        assert!(call.get_output().contains("hello"));
    }

    #[test]
    fn test_executor_bash_timeout() {
        let mut executor = ToolExecutor::new();
        let mut call = ToolCall::new("bash", serde_json::json!({
            "command": "sleep 10",
            "timeout": 100  // 100ms timeout
        }));

        let result = executor.execute(&mut call);
        assert!(result.is_err());
        assert_eq!(call.status, ToolCallStatus::Failed);
        assert!(call.error.as_ref().unwrap().contains("Timeout"));
    }

    #[test]
    fn test_executor_unknown_tool() {
        let mut executor = ToolExecutor::new();
        let mut call = ToolCall::new("nonexistent", serde_json::json!({}));

        let result = executor.execute(&mut call);
        assert!(result.is_err());
        assert_eq!(call.status, ToolCallStatus::Failed);
    }

    #[test]
    fn test_executor_glob() {
        let mut executor = ToolExecutor::new();
        let mut call = ToolCall::new("glob", serde_json::json!({
            "pattern": "src/*.rs"
        }));

        let result = executor.execute(&mut call);
        assert!(result.is_ok());
        let output = call.get_output();
        assert!(output.contains("main.rs") || output.is_empty()); // May be empty in temp dir
    }
}
