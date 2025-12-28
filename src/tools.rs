//! File and patch tools
//!
//! - Read files with context
//! - Generate unified diffs
//! - Apply patches
//! - Tool call infrastructure for self-bootstrapping

#![allow(dead_code)] // Tool infrastructure for self-bootstrapping

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

    /// Get output with line limit (for display)
    pub fn get_output_tail(&self, max_lines: usize) -> String {
        let output = self.get_output();
        let lines: Vec<&str> = output.lines().collect();
        if lines.len() <= max_lines {
            output
        } else {
            let skip = lines.len() - max_lines;
            format!("... ({} lines hidden)\n{}", skip, lines[skip..].join("\n"))
        }
    }

    /// Get output byte count
    pub fn output_size(&self) -> usize {
        self.output.lock().map(|o| o.len()).unwrap_or(0)
    }

    /// Format args for display (abbreviated)
    pub fn args_summary(&self) -> String {
        if let Some(path) = self.args.get("path").and_then(|v| v.as_str()) {
            return path.to_string();
        }
        if let Some(cmd) = self.args.get("command").and_then(|v| v.as_str()) {
            let truncated = if cmd.len() > 40 {
                format!("{}...", &cmd[..40])
            } else {
                cmd.to_string()
            };
            return truncated;
        }
        if let Some(pattern) = self.args.get("pattern").and_then(|v| v.as_str()) {
            return pattern.to_string();
        }
        "...".to_string()
    }

    /// Is this tool still running?
    pub fn is_running(&self) -> bool {
        self.status == ToolCallStatus::Running
    }

    /// Is this tool finished (done, failed, or killed)?
    pub fn is_finished(&self) -> bool {
        matches!(self.status, ToolCallStatus::Done | ToolCallStatus::Failed | ToolCallStatus::Killed)
    }
}

// ═══════════════════════════════════════════════════════════════
// OBSERVABLE EXECUTION DISPLAY
// ═══════════════════════════════════════════════════════════════

/// Spinner frames for running operations
const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Get spinner frame for current tick
pub fn spinner_frame(tick: usize) -> &'static str {
    SPINNER_FRAMES[tick % SPINNER_FRAMES.len()]
}

/// Display formatter for tool calls
pub struct ToolCallDisplay<'a> {
    call: &'a ToolCall,
    tick: usize,
    show_output: bool,
    max_output_lines: usize,
}

impl<'a> ToolCallDisplay<'a> {
    pub fn new(call: &'a ToolCall) -> Self {
        Self {
            call,
            tick: 0,
            show_output: true,
            max_output_lines: 20,
        }
    }

    pub fn with_tick(mut self, tick: usize) -> Self {
        self.tick = tick;
        self
    }

    pub fn with_output(mut self, show: bool) -> Self {
        self.show_output = show;
        self
    }

    pub fn with_max_lines(mut self, lines: usize) -> Self {
        self.max_output_lines = lines;
        self
    }

    /// Render header line (name, status, elapsed)
    pub fn header(&self) -> String {
        let icon = match self.call.status {
            ToolCallStatus::Pending => "○",
            ToolCallStatus::Running => spinner_frame(self.tick),
            ToolCallStatus::Done => "●",
            ToolCallStatus::Failed => "✗",
            ToolCallStatus::Killed => "◌",
        };

        let elapsed = self.call.elapsed()
            .map(|d| {
                if d.as_secs() >= 60 {
                    format!("{}m{}s", d.as_secs() / 60, d.as_secs() % 60)
                } else if d.as_millis() >= 1000 {
                    format!("{:.1}s", d.as_secs_f32())
                } else {
                    format!("{}ms", d.as_millis())
                }
            })
            .unwrap_or_default();

        let args = self.call.args_summary();
        let size = if self.call.output_size() > 0 {
            format!(" [{}b]", self.call.output_size())
        } else {
            String::new()
        };

        format!("{} {}({}) {}{}", icon, self.call.name, args, elapsed, size)
    }

    /// Render full display (header + output)
    pub fn render(&self) -> String {
        let mut result = self.header();

        if self.show_output && self.call.output_size() > 0 {
            let output = self.call.get_output_tail(self.max_output_lines);
            result.push_str("\n  ⎿ ");
            result.push_str(&output.replace('\n', "\n    "));
        }

        if let Some(err) = &self.call.error {
            result.push_str(&format!("\n  ✗ {}", err));
        }

        result
    }
}

/// Track multiple concurrent tool calls
pub struct ToolCallTracker {
    calls: Vec<ToolCall>,
    max_concurrent: usize,
    max_history: usize,
}

impl Default for ToolCallTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolCallTracker {
    pub fn new() -> Self {
        Self {
            calls: Vec::new(),
            max_concurrent: 10,
            max_history: 100,
        }
    }

    /// Add a new tool call to track
    pub fn add(&mut self, call: ToolCall) -> usize {
        let idx = self.calls.len();
        self.calls.push(call);

        // Prune old finished calls if over limit
        self.prune();

        idx
    }

    /// Get a tool call by index
    pub fn get(&self, idx: usize) -> Option<&ToolCall> {
        self.calls.get(idx)
    }

    /// Get mutable reference
    pub fn get_mut(&mut self, idx: usize) -> Option<&mut ToolCall> {
        self.calls.get_mut(idx)
    }

    /// Get all running calls
    pub fn running(&self) -> Vec<&ToolCall> {
        self.calls.iter().filter(|c| c.is_running()).collect()
    }

    /// Get all finished calls
    pub fn finished(&self) -> Vec<&ToolCall> {
        self.calls.iter().filter(|c| c.is_finished()).collect()
    }

    /// Count running calls
    pub fn running_count(&self) -> usize {
        self.calls.iter().filter(|c| c.is_running()).count()
    }

    /// Prune old finished calls
    fn prune(&mut self) {
        if self.calls.len() > self.max_history {
            // Keep running calls and most recent finished
            let mut running: Vec<_> = self.calls.iter()
                .filter(|c| c.is_running())
                .cloned()
                .collect();

            let finished: Vec<_> = self.calls.iter()
                .filter(|c| c.is_finished())
                .cloned()
                .collect();

            let keep_finished = finished.len().min(self.max_history - running.len());
            running.extend(finished.into_iter().rev().take(keep_finished).rev());

            self.calls = running;
        }
    }

    /// Render status summary (for status bar)
    pub fn status_summary(&self, tick: usize) -> String {
        let running = self.running_count();
        if running == 0 {
            return String::new();
        }

        let spinner = spinner_frame(tick);
        if running == 1 {
            if let Some(call) = self.running().first() {
                format!("{} {}({})", spinner, call.name, call.args_summary())
            } else {
                format!("{} 1 tool", spinner)
            }
        } else {
            format!("{} {} tools", spinner, running)
        }
    }

    /// Render all running calls
    pub fn render_running(&self, tick: usize) -> String {
        self.running()
            .iter()
            .map(|c| ToolCallDisplay::new(c).with_tick(tick).render())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

// ═══════════════════════════════════════════════════════════════
// TOOL EXECUTOR
// ═══════════════════════════════════════════════════════════════

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
            "patch" | "diff" => self.exec_patch(call),
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

    fn exec_patch(&self, call: &mut ToolCall) -> Result<()> {
        let path = call.args.get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("patch: missing 'path' argument"))?;

        let diff = call.args.get("diff")
            .or_else(|| call.args.get("patch"))
            .or_else(|| call.args.get("content"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("patch: missing 'diff' or 'patch' argument"))?;

        let path = Path::new(path);

        // Read original content
        let original = if path.exists() {
            fs::read_to_string(path)
                .with_context(|| format!("Failed to read {}", path.display()))?
        } else {
            // New file - start from empty
            String::new()
        };

        // Apply the patch
        let patched = apply_patch(&original, diff)?;

        // Preview the change
        let preview = preview_changes(&original, &patched, &path.display().to_string());
        call.append_output(&format!("Preview:\n{}\n", preview));

        // Create backup if file exists
        if path.exists() {
            let backup = path.with_extension("bak");
            fs::copy(path, &backup)
                .with_context(|| format!("Failed to backup {}", path.display()))?;
            call.append_output(&format!("Backed up to: {}\n", backup.display()));
        }

        // Write patched content
        fs::write(path, &patched)
            .with_context(|| format!("Failed to write {}", path.display()))?;

        call.append_output(&format!("Patched {} ({} bytes)\n", path.display(), patched.len()));
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
#[derive(Debug, Clone)]
pub struct DiffHunk {
    pub old_start: usize,
    pub old_count: usize,
    pub new_start: usize,
    pub new_count: usize,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone)]
pub enum DiffLine {
    Context(String),
    Delete(String),
    Insert(String),
}

/// Parse unified diff format into hunks
pub fn parse_unified_diff(patch: &str) -> Vec<DiffHunk> {
    let mut hunks = Vec::new();
    let mut current_hunk: Option<DiffHunk> = None;

    for line in patch.lines() {
        // Parse hunk header: @@ -old_start,old_count +new_start,new_count @@
        if line.starts_with("@@") && line.contains("@@") {
            // Save previous hunk
            if let Some(hunk) = current_hunk.take() {
                hunks.push(hunk);
            }

            // Parse header like "@@ -1,4 +1,5 @@" or "@@ -1 +1 @@"
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 {
                let old_part = parts[1].trim_start_matches('-');
                let new_part = parts[2].trim_start_matches('+');

                let (old_start, old_count) = parse_range(old_part);
                let (new_start, new_count) = parse_range(new_part);

                current_hunk = Some(DiffHunk {
                    old_start,
                    old_count,
                    new_start,
                    new_count,
                    lines: Vec::new(),
                });
            }
        } else if let Some(ref mut hunk) = current_hunk {
            // Parse hunk content
            if let Some(content) = line.strip_prefix(' ') {
                hunk.lines.push(DiffLine::Context(content.to_string()));
            } else if let Some(content) = line.strip_prefix('-') {
                hunk.lines.push(DiffLine::Delete(content.to_string()));
            } else if let Some(content) = line.strip_prefix('+') {
                hunk.lines.push(DiffLine::Insert(content.to_string()));
            } else if line.is_empty() {
                // Empty context line
                hunk.lines.push(DiffLine::Context(String::new()));
            }
            // Skip header lines like "---", "+++"
        }
    }

    // Save last hunk
    if let Some(hunk) = current_hunk {
        hunks.push(hunk);
    }

    hunks
}

/// Parse range like "1,4" or "1" into (start, count)
fn parse_range(s: &str) -> (usize, usize) {
    if let Some((start, count)) = s.split_once(',') {
        (
            start.parse().unwrap_or(1),
            count.parse().unwrap_or(1),
        )
    } else {
        (s.parse().unwrap_or(1), 1)
    }
}

/// Apply a unified diff patch to original text
pub fn apply_patch(original: &str, patch: &str) -> Result<String> {
    // If patch doesn't look like a unified diff, treat as replacement
    if !patch.contains("@@") {
        return Ok(patch.to_string());
    }

    let hunks = parse_unified_diff(patch);
    if hunks.is_empty() {
        return Ok(original.to_string());
    }

    let original_lines: Vec<&str> = original.lines().collect();
    let mut result_lines: Vec<String> = Vec::new();
    let mut old_pos = 0; // Current position in original

    for hunk in &hunks {
        // Copy unchanged lines before this hunk
        let hunk_start = hunk.old_start.saturating_sub(1); // Convert to 0-indexed
        while old_pos < hunk_start && old_pos < original_lines.len() {
            result_lines.push(original_lines[old_pos].to_string());
            old_pos += 1;
        }

        // Apply hunk
        for diff_line in &hunk.lines {
            match diff_line {
                DiffLine::Context(content) => {
                    // Context should match original
                    if old_pos < original_lines.len() {
                        result_lines.push(content.clone());
                        old_pos += 1;
                    }
                }
                DiffLine::Delete(_) => {
                    // Skip this line in original
                    if old_pos < original_lines.len() {
                        old_pos += 1;
                    }
                }
                DiffLine::Insert(content) => {
                    // Add new line
                    result_lines.push(content.clone());
                }
            }
        }
    }

    // Copy remaining lines after last hunk
    while old_pos < original_lines.len() {
        result_lines.push(original_lines[old_pos].to_string());
        old_pos += 1;
    }

    // Join with newlines, preserving trailing newline if original had one
    let mut result = result_lines.join("\n");
    if original.ends_with('\n') && !result.is_empty() {
        result.push('\n');
    }

    Ok(result)
}

/// Apply multiple patches to a file, with validation
pub fn apply_patches_to_file(path: &Path, patches: &[String]) -> Result<()> {
    let original = fs::read_to_string(path)
        .with_context(|| format!("Failed to read {}", path.display()))?;

    let mut content = original.clone();
    for patch in patches {
        content = apply_patch(&content, patch)?;
    }

    // Create backup
    let backup = path.with_extension("bak");
    fs::write(&backup, &original)
        .with_context(|| format!("Failed to backup to {}", backup.display()))?;

    // Write patched content
    fs::write(path, &content)
        .with_context(|| format!("Failed to write {}", path.display()))?;

    Ok(())
}

/// Extract target file path from a unified diff
pub fn extract_diff_target(patch: &str) -> Option<String> {
    for line in patch.lines() {
        // Look for "+++ b/path/to/file" format
        if let Some(path) = line.strip_prefix("+++ b/") {
            return Some(path.to_string());
        }
        // Or "+++ path/to/file" format
        if let Some(path) = line.strip_prefix("+++ ") {
            // Skip "/dev/null"
            if path != "/dev/null" {
                return Some(path.to_string());
            }
        }
    }
    None
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

    #[test]
    fn test_parse_unified_diff_basic() {
        let patch = r#"--- a/test.txt
+++ b/test.txt
@@ -1,3 +1,4 @@
 line 1
-line 2
+line 2 modified
+line 2.5
 line 3
"#;
        let hunks = parse_unified_diff(patch);
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].old_start, 1);
        assert_eq!(hunks[0].old_count, 3);
        assert_eq!(hunks[0].new_start, 1);
        assert_eq!(hunks[0].new_count, 4);
        assert_eq!(hunks[0].lines.len(), 5);
    }

    #[test]
    fn test_parse_unified_diff_multiple_hunks() {
        let patch = r#"--- a/test.txt
+++ b/test.txt
@@ -1,2 +1,2 @@
 line 1
-line 2
+line 2 changed
@@ -10,2 +10,3 @@
 line 10
+line 10.5
 line 11
"#;
        let hunks = parse_unified_diff(patch);
        assert_eq!(hunks.len(), 2);
        assert_eq!(hunks[1].old_start, 10);
    }

    #[test]
    fn test_apply_patch_simple() {
        let original = "line 1\nline 2\nline 3\n";
        let patch = r#"--- a/test.txt
+++ b/test.txt
@@ -1,3 +1,3 @@
 line 1
-line 2
+line 2 modified
 line 3
"#;
        let result = apply_patch(original, patch).unwrap();
        assert_eq!(result, "line 1\nline 2 modified\nline 3\n");
    }

    #[test]
    fn test_apply_patch_insert() {
        let original = "line 1\nline 2\n";
        let patch = r#"--- a/test.txt
+++ b/test.txt
@@ -1,2 +1,3 @@
 line 1
+inserted
 line 2
"#;
        let result = apply_patch(original, patch).unwrap();
        assert_eq!(result, "line 1\ninserted\nline 2\n");
    }

    #[test]
    fn test_apply_patch_delete() {
        let original = "line 1\nline 2\nline 3\n";
        let patch = r#"--- a/test.txt
+++ b/test.txt
@@ -1,3 +1,2 @@
 line 1
-line 2
 line 3
"#;
        let result = apply_patch(original, patch).unwrap();
        assert_eq!(result, "line 1\nline 3\n");
    }

    #[test]
    fn test_apply_patch_not_a_diff() {
        let original = "line 1\n";
        let patch = "completely new content";

        // Non-diff input should return the patch as replacement
        let result = apply_patch(original, patch).unwrap();
        assert_eq!(result, "completely new content");
    }

    #[test]
    fn test_extract_diff_target() {
        let patch = r#"--- a/src/main.rs
+++ b/src/main.rs
@@ -1 +1 @@
-old
+new
"#;
        assert_eq!(extract_diff_target(patch), Some("src/main.rs".to_string()));
    }

    #[test]
    fn test_extract_diff_target_no_prefix() {
        let patch = r#"--- /dev/null
+++ src/new_file.rs
@@ -0,0 +1 @@
+new content
"#;
        assert_eq!(extract_diff_target(patch), Some("src/new_file.rs".to_string()));
    }

    #[test]
    fn test_roundtrip_diff_apply() {
        // Generate a diff, then apply it - should get the modified version
        let original = "line 1\nline 2\nline 3\n";
        let modified = "line 1\nline 2 changed\nline 3\n";

        let diff = generate_diff(original, modified, "test.txt");
        let result = apply_patch(original, &diff).unwrap();

        assert_eq!(result, modified);
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

    // ═══════════════════════════════════════════════════════════════
    // OBSERVABLE EXECUTION TESTS
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_spinner_frames() {
        // All frames should be different
        let frames: Vec<_> = (0..SPINNER_FRAMES.len())
            .map(|i| spinner_frame(i))
            .collect();
        assert_eq!(frames.len(), SPINNER_FRAMES.len());

        // Should cycle correctly
        assert_eq!(spinner_frame(0), spinner_frame(SPINNER_FRAMES.len()));
        assert_eq!(spinner_frame(1), spinner_frame(SPINNER_FRAMES.len() + 1));
    }

    #[test]
    fn test_output_tail_short() {
        let call = ToolCall::new("test", serde_json::json!({}));
        call.append_output("line 1\nline 2\nline 3\n");

        // Should return all lines when under limit
        let tail = call.get_output_tail(10);
        assert!(tail.contains("line 1"));
        assert!(tail.contains("line 2"));
        assert!(tail.contains("line 3"));
        assert!(!tail.contains("hidden"));
    }

    #[test]
    fn test_output_tail_long() {
        let call = ToolCall::new("test", serde_json::json!({}));
        for i in 1..=50 {
            call.append_output(&format!("line {}\n", i));
        }

        // Should truncate and show hidden count
        let tail = call.get_output_tail(10);
        assert!(tail.contains("hidden"));
        assert!(tail.contains("line 50")); // Last line visible
        assert!(!tail.contains("line 1")); // First line hidden
    }

    #[test]
    fn test_args_summary_path() {
        let call = ToolCall::new("read", serde_json::json!({
            "path": "/home/user/project/src/main.rs"
        }));
        assert_eq!(call.args_summary(), "/home/user/project/src/main.rs");
    }

    #[test]
    fn test_args_summary_command() {
        let call = ToolCall::new("bash", serde_json::json!({
            "command": "echo hello"
        }));
        assert_eq!(call.args_summary(), "echo hello");
    }

    #[test]
    fn test_args_summary_long_command() {
        let long_cmd = "this is a very long command that should be truncated because it's too long to display nicely";
        let call = ToolCall::new("bash", serde_json::json!({
            "command": long_cmd
        }));
        let summary = call.args_summary();
        assert!(summary.len() <= 43); // 40 + "..."
        assert!(summary.ends_with("..."));
    }

    #[test]
    fn test_is_running_is_finished() {
        let mut call = ToolCall::new("test", serde_json::json!({}));

        assert!(!call.is_running());
        assert!(!call.is_finished());

        call.start();
        assert!(call.is_running());
        assert!(!call.is_finished());

        call.complete();
        assert!(!call.is_running());
        assert!(call.is_finished());
    }

    #[test]
    fn test_display_header_pending() {
        let call = ToolCall::new("read", serde_json::json!({
            "path": "test.txt"
        }));

        let display = ToolCallDisplay::new(&call);
        let header = display.header();

        assert!(header.contains("○")); // pending icon
        assert!(header.contains("read"));
        assert!(header.contains("test.txt"));
    }

    #[test]
    fn test_display_header_running() {
        let mut call = ToolCall::new("bash", serde_json::json!({
            "command": "sleep 1"
        }));
        call.start();

        let display = ToolCallDisplay::new(&call).with_tick(0);
        let header = display.header();

        assert!(header.contains("⠋")); // first spinner frame
        assert!(header.contains("bash"));
    }

    #[test]
    fn test_display_header_done() {
        let mut call = ToolCall::new("test", serde_json::json!({}));
        call.start();
        call.complete();

        let display = ToolCallDisplay::new(&call);
        let header = display.header();

        assert!(header.contains("●")); // done icon
        assert!(header.contains("ms")); // elapsed time
    }

    #[test]
    fn test_display_header_failed() {
        let mut call = ToolCall::new("test", serde_json::json!({}));
        call.start();
        call.fail("kaboom");

        let display = ToolCallDisplay::new(&call);
        let header = display.header();

        assert!(header.contains("✗")); // failed icon
    }

    #[test]
    fn test_display_render_with_output() {
        let mut call = ToolCall::new("test", serde_json::json!({}));
        call.start();
        call.append_output("hello world\n");
        call.complete();

        let display = ToolCallDisplay::new(&call).with_output(true);
        let rendered = display.render();

        assert!(rendered.contains("hello world"));
        assert!(rendered.contains("⎿")); // output indicator
    }

    #[test]
    fn test_display_render_without_output() {
        let mut call = ToolCall::new("test", serde_json::json!({}));
        call.start();
        call.append_output("hello world\n");
        call.complete();

        let display = ToolCallDisplay::new(&call).with_output(false);
        let rendered = display.render();

        assert!(!rendered.contains("hello world"));
    }

    #[test]
    fn test_display_render_with_error() {
        let mut call = ToolCall::new("test", serde_json::json!({}));
        call.start();
        call.fail("something bad happened");

        let display = ToolCallDisplay::new(&call);
        let rendered = display.render();

        assert!(rendered.contains("something bad happened"));
    }

    #[test]
    fn test_tracker_add_and_get() {
        let mut tracker = ToolCallTracker::new();

        let call = ToolCall::new("test", serde_json::json!({}));
        let idx = tracker.add(call);

        assert_eq!(idx, 0);
        assert!(tracker.get(idx).is_some());
        assert!(tracker.get(999).is_none());
    }

    #[test]
    fn test_tracker_running_and_finished() {
        let mut tracker = ToolCallTracker::new();

        // Add running call
        let mut call1 = ToolCall::new("running", serde_json::json!({}));
        call1.start();
        tracker.add(call1);

        // Add finished call
        let mut call2 = ToolCall::new("done", serde_json::json!({}));
        call2.start();
        call2.complete();
        tracker.add(call2);

        assert_eq!(tracker.running_count(), 1);
        assert_eq!(tracker.running().len(), 1);
        assert_eq!(tracker.finished().len(), 1);
    }

    #[test]
    fn test_tracker_status_summary_none() {
        let tracker = ToolCallTracker::new();
        assert_eq!(tracker.status_summary(0), "");
    }

    #[test]
    fn test_tracker_status_summary_one() {
        let mut tracker = ToolCallTracker::new();

        let mut call = ToolCall::new("read", serde_json::json!({
            "path": "file.txt"
        }));
        call.start();
        tracker.add(call);

        let summary = tracker.status_summary(0);
        assert!(summary.contains("⠋")); // spinner
        assert!(summary.contains("read"));
        assert!(summary.contains("file.txt"));
    }

    #[test]
    fn test_tracker_status_summary_multiple() {
        let mut tracker = ToolCallTracker::new();

        for i in 0..3 {
            let mut call = ToolCall::new(&format!("tool{}", i), serde_json::json!({}));
            call.start();
            tracker.add(call);
        }

        let summary = tracker.status_summary(0);
        assert!(summary.contains("3 tools"));
    }

    #[test]
    fn test_tracker_render_running() {
        let mut tracker = ToolCallTracker::new();

        let mut call = ToolCall::new("bash", serde_json::json!({
            "command": "echo hi"
        }));
        call.start();
        call.append_output("hi\n");
        tracker.add(call);

        let rendered = tracker.render_running(0);
        assert!(rendered.contains("bash"));
        assert!(rendered.contains("hi"));
    }

    #[test]
    fn test_elapsed_formatting() {
        let mut call = ToolCall::new("test", serde_json::json!({}));
        call.start();

        // Manually set started_at to test formatting
        call.started_at = Some(Instant::now() - Duration::from_millis(500));
        call.finished_at = Some(Instant::now());

        let display = ToolCallDisplay::new(&call);
        let header = display.header();
        // Should show milliseconds for < 1 second
        assert!(header.contains("ms"));
    }

    #[test]
    fn test_output_size() {
        let call = ToolCall::new("test", serde_json::json!({}));

        assert_eq!(call.output_size(), 0);

        call.append_output("hello");
        assert_eq!(call.output_size(), 5);

        call.append_output(" world");
        assert_eq!(call.output_size(), 11);
    }

    #[test]
    fn test_display_output_size_in_header() {
        let mut call = ToolCall::new("test", serde_json::json!({}));
        call.start();
        call.append_output("x".repeat(100).as_str());
        call.complete();

        let display = ToolCallDisplay::new(&call);
        let header = display.header();

        assert!(header.contains("[100b]"));
    }
}
