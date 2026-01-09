//! Agent module for LLM-driven tool execution
//!
//! Parses LLM responses for tool calls and executes them in a loop.
//! This is the core of self-bootstrapping: hyle using hyle to develop hyle.

#![allow(dead_code)] // Forward-looking module

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::tools::{ToolCall, ToolCallStatus, ToolCallTracker, ToolExecutor};

// ═══════════════════════════════════════════════════════════════
// TOOL CALL PARSING
// ═══════════════════════════════════════════════════════════════

/// A parsed tool invocation from LLM response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedToolCall {
    pub name: String,
    pub args: serde_json::Value,
}

/// Parse tool calls from LLM response text
///
/// Supports multiple formats:
/// - JSON blocks: ```json\n{"tool": "read", "args": {...}}\n```
/// - Inline JSON: <tool>{"name": "read", "args": {...}}</tool>
/// - Function calls: read(path="/foo/bar")
pub fn parse_tool_calls(response: &str) -> Vec<ParsedToolCall> {
    let mut calls = Vec::new();

    // Try JSON code blocks first
    calls.extend(parse_json_blocks(response));

    // Try XML-style tool tags
    calls.extend(parse_tool_tags(response));

    // Try function-call syntax
    calls.extend(parse_function_calls(response));

    calls
}

/// Parse JSON code blocks containing tool calls
fn parse_json_blocks(text: &str) -> Vec<ParsedToolCall> {
    let mut calls = Vec::new();

    // Match ```json ... ``` or ``` ... ```
    let re = regex::Regex::new(r"```(?:json)?\s*\n([\s\S]*?)\n```").unwrap();

    for cap in re.captures_iter(text) {
        if let Some(json_str) = cap.get(1) {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(json_str.as_str()) {
                if let Some(call) = value_to_tool_call(&parsed) {
                    calls.push(call);
                }
                // Also try array of tool calls
                if let Some(arr) = parsed.as_array() {
                    for item in arr {
                        if let Some(call) = value_to_tool_call(item) {
                            calls.push(call);
                        }
                    }
                }
            }
        }
    }

    calls
}

/// Parse <tool>...</tool> tags
fn parse_tool_tags(text: &str) -> Vec<ParsedToolCall> {
    let mut calls = Vec::new();

    let re = regex::Regex::new(r"<tool>([\s\S]*?)</tool>").unwrap();

    for cap in re.captures_iter(text) {
        if let Some(content) = cap.get(1) {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(content.as_str()) {
                if let Some(call) = value_to_tool_call(&parsed) {
                    calls.push(call);
                }
            }
        }
    }

    calls
}

/// Parse function-call syntax: read(path="/foo")
fn parse_function_calls(text: &str) -> Vec<ParsedToolCall> {
    let mut calls = Vec::new();

    // Match tool_name(key="value", key2="value2")
    let re = regex::Regex::new(r"(\w+)\(([^)]*)\)").unwrap();

    for cap in re.captures_iter(text) {
        let name = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        let args_str = cap.get(2).map(|m| m.as_str()).unwrap_or("");

        // Only parse known tool names
        if !is_known_tool(name) {
            continue;
        }

        // Parse key="value" pairs
        let mut args = serde_json::Map::new();
        #[allow(clippy::regex_creation_in_loops)] // Not a hot path, called once per LLM response
        let arg_re = regex::Regex::new(r#"(\w+)\s*=\s*"([^"]*)""#).unwrap();

        for arg_cap in arg_re.captures_iter(args_str) {
            let key = arg_cap.get(1).map(|m| m.as_str()).unwrap_or("");
            let value = arg_cap.get(2).map(|m| m.as_str()).unwrap_or("");
            args.insert(
                key.to_string(),
                serde_json::Value::String(value.to_string()),
            );
        }

        if !args.is_empty() {
            calls.push(ParsedToolCall {
                name: name.to_string(),
                args: serde_json::Value::Object(args),
            });
        }
    }

    calls
}

/// Convert a JSON value to a ParsedToolCall if it has the right structure
fn value_to_tool_call(value: &serde_json::Value) -> Option<ParsedToolCall> {
    let obj = value.as_object()?;

    // Try {"tool": "name", "args": {...}}
    if let Some(tool) = obj.get("tool").and_then(|v| v.as_str()) {
        let args = obj.get("args").cloned().unwrap_or(serde_json::json!({}));
        return Some(ParsedToolCall {
            name: tool.to_string(),
            args,
        });
    }

    // Try {"name": "tool_name", "args": {...}}
    if let Some(name) = obj.get("name").and_then(|v| v.as_str()) {
        let args = obj.get("args").cloned().unwrap_or(serde_json::json!({}));
        return Some(ParsedToolCall {
            name: name.to_string(),
            args,
        });
    }

    // Try direct tool object: {"read": {"path": "..."}}
    for (key, val) in obj {
        if is_known_tool(key) {
            return Some(ParsedToolCall {
                name: key.clone(),
                args: val.clone(),
            });
        }
    }

    None
}

/// Check if a name is a known tool
fn is_known_tool(name: &str) -> bool {
    matches!(
        name,
        "read" | "write" | "glob" | "grep" | "bash" | "edit" | "search" | "patch" | "diff"
    )
}

// ═══════════════════════════════════════════════════════════════
// DIFF DETECTION IN RESPONSES
// ═══════════════════════════════════════════════════════════════

/// A diff/patch found in an LLM response
#[derive(Debug, Clone)]
pub struct DetectedDiff {
    pub target_file: Option<String>,
    pub content: String,
    pub is_unified: bool,
}

/// Detect unified diffs in LLM response
pub fn detect_diffs(response: &str) -> Vec<DetectedDiff> {
    let mut diffs = Vec::new();

    // Look for code blocks that look like diffs
    let re = regex::Regex::new(r"```(?:diff|patch)?\s*\n([\s\S]*?)\n```").unwrap();

    for cap in re.captures_iter(response) {
        if let Some(content) = cap.get(1) {
            let content = content.as_str();
            // Check if it looks like a unified diff
            if content.contains("@@") && (content.contains("---") || content.contains("+++")) {
                let target = crate::tools::extract_diff_target(content);
                diffs.push(DetectedDiff {
                    target_file: target,
                    content: content.to_string(),
                    is_unified: true,
                });
            }
        }
    }

    diffs
}

// ═══════════════════════════════════════════════════════════════
// TASK COMPLETION DETECTION
// ═══════════════════════════════════════════════════════════════

/// Signals that indicate task completion
const COMPLETION_SIGNALS: &[&str] = &[
    "task complete",
    "task completed",
    "done",
    "finished",
    "all changes applied",
    "successfully",
    "no more changes needed",
    "implementation complete",
];

/// Check if response indicates task completion
pub fn is_task_complete(response: &str) -> bool {
    let lower = response.to_lowercase();

    // Check for explicit completion signals
    for signal in COMPLETION_SIGNALS {
        if lower.contains(signal) {
            return true;
        }
    }

    // Check for no tool calls remaining
    let calls = parse_tool_calls(response);
    if calls.is_empty() && response.len() > 100 {
        // Long response with no tool calls might be final explanation
        return true;
    }

    false
}

/// Check if response indicates an error that should stop execution
pub fn is_fatal_error(response: &str) -> bool {
    let lower = response.to_lowercase();

    lower.contains("cannot proceed")
        || lower.contains("unable to continue")
        || lower.contains("fatal error")
        || lower.contains("aborting")
}

// ═══════════════════════════════════════════════════════════════
// AGENT EXECUTION
// ═══════════════════════════════════════════════════════════════

/// Agent configuration
pub struct AgentConfig {
    /// Base iteration limit
    pub max_iterations: usize,
    /// Maximum tool calls per iteration
    pub max_tool_calls_per_iteration: usize,
    /// Timeout for each tool execution
    pub timeout_per_tool_ms: u64,
    /// Extend iterations when making progress (add bonus_iterations)
    pub extend_on_progress: bool,
    /// Bonus iterations when progress is detected
    pub bonus_iterations: usize,
    /// Consecutive failures before declaring stuck (higher = more persistent)
    pub max_consecutive_failures: usize,
    /// Retry failed tools with alternative approaches
    pub retry_on_failure: bool,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_iterations: 20,
            max_tool_calls_per_iteration: 5,
            timeout_per_tool_ms: 60000,
            // Autonomy settings - be more persistent
            extend_on_progress: true,
            bonus_iterations: 5,
            max_consecutive_failures: 5, // was effectively 3
            retry_on_failure: true,
        }
    }
}

impl AgentConfig {
    /// Create a more autonomous configuration
    pub fn autonomous() -> Self {
        Self {
            max_iterations: 30,
            max_tool_calls_per_iteration: 8,
            timeout_per_tool_ms: 120000,
            extend_on_progress: true,
            bonus_iterations: 10,
            max_consecutive_failures: 7,
            retry_on_failure: true,
        }
    }

    /// Create a conservative configuration (for risky operations)
    pub fn conservative() -> Self {
        Self {
            max_iterations: 10,
            max_tool_calls_per_iteration: 3,
            timeout_per_tool_ms: 30000,
            extend_on_progress: false,
            bonus_iterations: 0,
            max_consecutive_failures: 2,
            retry_on_failure: false,
        }
    }
}

/// Result of agent execution
#[derive(Debug)]
pub struct AgentResult {
    pub iterations: usize,
    pub tool_calls_executed: usize,
    pub final_response: String,
    pub success: bool,
    pub error: Option<String>,
    pub tokens_used: usize,
}

/// Execute tool calls from a parsed response
pub fn execute_tool_calls(
    calls: &[ParsedToolCall],
    executor: &mut ToolExecutor,
    tracker: &mut ToolCallTracker,
) -> Vec<(usize, Result<()>)> {
    let mut results = Vec::new();

    for parsed in calls {
        let call = ToolCall::new(&parsed.name, parsed.args.clone());
        let idx = tracker.add(call.clone());

        let result = executor.execute(tracker.get_mut(idx).unwrap());
        results.push((idx, result));
    }

    results
}

/// Format tool results for feedback to LLM
pub fn format_tool_results(tracker: &ToolCallTracker, indices: &[usize]) -> String {
    let mut output = String::new();

    for &idx in indices {
        if let Some(call) = tracker.get(idx) {
            output.push_str(&format!("\n## {} result:\n", call.name));

            match &call.status {
                ToolCallStatus::Done => {
                    let content = call.get_output();
                    if content.is_empty() {
                        output.push_str("(no output)\n");
                    } else {
                        output.push_str(&content);
                    }
                }
                ToolCallStatus::Failed => {
                    output.push_str(&format!(
                        "ERROR: {}\n",
                        call.error.as_deref().unwrap_or("unknown")
                    ));
                }
                ToolCallStatus::Killed => {
                    output.push_str("(killed by user)\n");
                }
                _ => {
                    output.push_str("(unexpected status)\n");
                }
            }
        }
    }

    output
}

// ═══════════════════════════════════════════════════════════════
// SYSTEM PROMPT
// ═══════════════════════════════════════════════════════════════

/// Generate system prompt for code assistant mode
pub fn code_assistant_prompt(work_dir: &Path) -> String {
    format!(
        r#"You are hyle, a Rust-native autonomous code assistant. You complete tasks independently.

Working directory: {}

## Tools

- read(path="..."): Read a file with line numbers
- write(path="...", content="..."): Write content to a file (creates backup)
- patch(path="...", diff="..."): Apply a unified diff patch to a file
- glob(pattern="..."): Find files matching a glob pattern
- grep(pattern="...", path="..."): Search for regex pattern in files
- bash(command="..."): Execute a shell command

## Tool Usage

JSON block format:
```json
{{"tool": "read", "args": {{"path": "src/main.rs"}}}}
```

Function syntax: read(path="src/main.rs")

For code changes, use unified diffs:
```json
{{"tool": "patch", "args": {{"path": "src/main.rs", "diff": "--- a/src/main.rs\n+++ b/src/main.rs\n@@ -1,3 +1,3 @@\n line1\n-old line\n+new line\n line3"}}}}
```

## Autonomy Guidelines

**Be proactive and persistent:**
- Complete the full task without stopping for confirmation
- Make reasonable decisions autonomously - don't ask for permission on minor choices
- If one approach fails, try an alternative before giving up
- Keep iterating until the task is truly complete

**Error recovery:**
- If a tool fails, analyze why and try a different approach
- Read error messages carefully and fix the underlying issue
- Don't repeat the same failing action - adapt

**Quality standards:**
- Read files before modifying them
- Use patch for targeted changes, write for complete rewrites
- Make atomic, focused changes
- Run tests after modifications
- Verify your changes work before declaring done

**Completion:**
When the task is fully complete (not just started), say "Task complete" and summarize what was done.
Only declare complete when you've verified the solution works.
"#,
        work_dir.display()
    )
}

// ═══════════════════════════════════════════════════════════════
// AUTONOMOUS AGENT LOOP
// ═══════════════════════════════════════════════════════════════

use crate::client::{self, StreamEvent};
use tokio::sync::mpsc;

/// Events emitted by the agent loop
#[derive(Debug, Clone)]
pub enum AgentEvent {
    /// LLM is generating text
    Token(String),
    /// LLM finished generating, parsed tool calls
    ToolCallsParsed(Vec<ParsedToolCall>),
    /// Executing a tool
    ToolExecuting { name: String, args: String },
    /// Tool finished with result
    ToolResult {
        name: String,
        success: bool,
        output: String,
    },
    /// Iteration complete, continuing
    IterationComplete { iteration: usize, tool_count: usize },
    /// Agent finished
    Complete { iterations: usize, success: bool },
    /// Error occurred
    Error(String),
    /// Status message
    Status(String),
}

/// Run the autonomous agent loop
///
/// This is the core function that makes hyle work like Claude Code:
/// 1. Send user prompt to LLM with tool-use system prompt
/// 2. Stream LLM response, parsing for tool calls
/// 3. Execute tool calls, collect results
/// 4. Feed results back to LLM
/// 5. Repeat until task complete or max iterations
pub async fn run_agent_loop(
    api_key: &str,
    model: &str,
    user_prompt: &str,
    work_dir: &Path,
    config: AgentConfig,
    event_tx: mpsc::Sender<AgentEvent>,
) -> AgentResult {
    let mut executor = ToolExecutor::new();
    let mut tracker = ToolCallTracker::new();
    let mut conversation: Vec<serde_json::Value> = Vec::new();
    let mut total_tool_calls = 0;
    let mut final_response = String::new();

    // Cognitive tracking for stuck detection
    let mut recent_actions: Vec<String> = Vec::new();
    let mut consecutive_failures = 0;

    // Dynamic iteration tracking
    let mut current_max_iterations = config.max_iterations;
    let mut progress_bonus_applied = false;
    let mut successful_iterations = 0;

    // Build system prompt with tool instructions
    let system_prompt = code_assistant_prompt(work_dir);

    // Start with system message
    conversation.push(serde_json::json!({
        "role": "system",
        "content": system_prompt
    }));

    // Add initial user message
    conversation.push(serde_json::json!({
        "role": "user",
        "content": user_prompt
    }));

    let mut iteration = 0;
    while iteration < current_max_iterations {
        let _ = event_tx
            .send(AgentEvent::Status(format!(
                "Iteration {} of {}{}",
                iteration + 1,
                current_max_iterations,
                if progress_bonus_applied { " (extended)" } else { "" }
            )))
            .await;

        // Stream LLM response - pass full conversation history
        let mut response = String::new();
        let last_user_msg = conversation
            .iter()
            .rev()
            .find(|m| m["role"] == "user")
            .and_then(|m| m["content"].as_str())
            .unwrap_or("");
        let history: Vec<_> = conversation
            .iter()
            .filter(|m| m["role"] != "system") // System handled separately
            .take(conversation.len().saturating_sub(1))
            .cloned()
            .collect();
        let stream_result =
            client::stream_completion_full(api_key, model, last_user_msg, None, &history).await;

        let mut rx = match stream_result {
            Ok(rx) => rx,
            Err(e) => {
                let _ = event_tx.send(AgentEvent::Error(e.to_string())).await;
                return AgentResult {
                    iterations: iteration,
                    tool_calls_executed: total_tool_calls,
                    final_response: response,
                    success: false,
                    error: Some(e.to_string()),
                    tokens_used: 0,
                };
            }
        };

        // Collect streaming response
        while let Some(event) = rx.recv().await {
            match event {
                StreamEvent::Token(t) => {
                    response.push_str(&t);
                    let _ = event_tx.send(AgentEvent::Token(t)).await;
                }
                StreamEvent::Done(_usage) => {
                    break;
                }
                StreamEvent::Error(e) => {
                    let _ = event_tx.send(AgentEvent::Error(e.clone())).await;
                    return AgentResult {
                        iterations: iteration,
                        tool_calls_executed: total_tool_calls,
                        final_response: response,
                        success: false,
                        error: Some(e),
                        tokens_used: 0,
                    };
                }
            }
        }

        // Add assistant response to conversation
        conversation.push(serde_json::json!({
            "role": "assistant",
            "content": response.clone()
        }));

        final_response = response.clone();

        // Check for fatal error
        if is_fatal_error(&response) {
            let _ = event_tx
                .send(AgentEvent::Error("Agent reported fatal error".into()))
                .await;
            return AgentResult {
                iterations: iteration + 1,
                tool_calls_executed: total_tool_calls,
                final_response,
                success: false,
                error: Some("Agent reported fatal error".into()),
                tokens_used: 0,
            };
        }

        // Parse tool calls
        let tool_calls = parse_tool_calls(&response);
        let _ = event_tx
            .send(AgentEvent::ToolCallsParsed(tool_calls.clone()))
            .await;

        // Check if task is complete (no tool calls or explicit completion)
        if tool_calls.is_empty() || is_task_complete(&response) {
            let _ = event_tx
                .send(AgentEvent::Complete {
                    iterations: iteration + 1,
                    success: true,
                })
                .await;
            return AgentResult {
                iterations: iteration + 1,
                tool_calls_executed: total_tool_calls,
                final_response,
                success: true,
                error: None,
                tokens_used: 0,
            };
        }

        // Execute tool calls (up to limit)
        let mut tool_results = String::new();
        let mut iteration_failures = 0;
        let calls_to_execute = tool_calls
            .into_iter()
            .take(config.max_tool_calls_per_iteration)
            .collect::<Vec<_>>();

        for parsed in &calls_to_execute {
            // Track action for stuck detection
            let action_sig = format!("{}:{}", parsed.name, parsed.args);
            recent_actions.push(action_sig.clone());
            if recent_actions.len() > 10 {
                recent_actions.remove(0);
            }

            let _ = event_tx
                .send(AgentEvent::ToolExecuting {
                    name: parsed.name.clone(),
                    args: parsed.args.to_string(),
                })
                .await;

            let call = ToolCall::new(&parsed.name, parsed.args.clone());
            let idx = tracker.add(call);

            let result = executor.execute(tracker.get_mut(idx).unwrap());
            total_tool_calls += 1;

            let success = result.is_ok();
            if !success {
                iteration_failures += 1;
            }
            let output = format_tool_results(&tracker, &[idx]);

            let _ = event_tx
                .send(AgentEvent::ToolResult {
                    name: parsed.name.clone(),
                    success,
                    output: output.clone(),
                })
                .await;

            tool_results.push_str(&output);
        }

        // Track consecutive failures for stuck detection
        let made_progress = iteration_failures < calls_to_execute.len();
        if iteration_failures == calls_to_execute.len() && !calls_to_execute.is_empty() {
            consecutive_failures += 1;
        } else {
            consecutive_failures = 0;
        }

        // Dynamic iteration extension: if making progress, extend runway
        if made_progress {
            successful_iterations += 1;
        }

        if config.extend_on_progress && made_progress && !progress_bonus_applied {
            // Apply bonus iterations when we've had 3+ successful iterations
            if successful_iterations >= 3 && iteration >= config.max_iterations / 2 {
                current_max_iterations += config.bonus_iterations;
                progress_bonus_applied = true;
                let _ = event_tx
                    .send(AgentEvent::Status(format!(
                        "Progress detected! Extended to {} iterations (+{})",
                        current_max_iterations, config.bonus_iterations
                    )))
                    .await;
            }
        }

        // Stuck detection: use configurable threshold
        let failure_threshold = config.max_consecutive_failures;
        let repeat_threshold = failure_threshold.min(5); // Cap at 5 for repeat detection

        let is_stuck = consecutive_failures >= failure_threshold || {
            // Check if same action repeated too many times in recent history
            recent_actions.len() >= repeat_threshold && {
                let last = recent_actions.last().unwrap();
                recent_actions.iter().filter(|a| *a == last).count() >= repeat_threshold
            }
        };

        if is_stuck {
            let _ = event_tx
                .send(AgentEvent::Error(format!(
                    "Agent appears stuck after {} consecutive failures or repeated actions",
                    consecutive_failures
                )))
                .await;
            return AgentResult {
                iterations: iteration + 1,
                tool_calls_executed: total_tool_calls,
                final_response,
                success: false,
                error: Some(format!("Agent stuck after {} failures", consecutive_failures)),
                tokens_used: 0,
            };
        }

        // Add tool results to conversation for next iteration
        conversation.push(serde_json::json!({
            "role": "user",
            "content": format!("Tool execution results:\n{}", tool_results)
        }));

        let _ = event_tx
            .send(AgentEvent::IterationComplete {
                iteration: iteration + 1,
                tool_count: calls_to_execute.len(),
            })
            .await;

        iteration += 1;
    }

    // Max iterations reached
    let _ = event_tx
        .send(AgentEvent::Error("Max iterations reached".into()))
        .await;
    AgentResult {
        iterations: config.max_iterations,
        tool_calls_executed: total_tool_calls,
        final_response,
        success: false,
        error: Some("Max iterations reached".into()),
        tokens_used: 0,
    }
}

// ═══════════════════════════════════════════════════════════════
// AGENT CORE - Unified interface for TUI and CLI
// ═══════════════════════════════════════════════════════════════

/// Agent core - the single source of truth for agentic behavior
///
/// Use this instead of directly calling run_agent_loop.
/// Both TUI and CLI consume events from this.
pub struct AgentCore {
    pub api_key: String,
    pub model: String,
    pub work_dir: std::path::PathBuf,
    pub config: AgentConfig,
}

impl AgentCore {
    pub fn new(api_key: &str, model: &str, work_dir: &Path) -> Self {
        Self {
            api_key: api_key.to_string(),
            model: model.to_string(),
            work_dir: work_dir.to_path_buf(),
            config: AgentConfig::default(),
        }
    }

    pub fn with_config(mut self, config: AgentConfig) -> Self {
        self.config = config;
        self
    }

    /// Run agent and return event receiver
    ///
    /// Spawns the agent loop in background, returns channel to receive events.
    /// Caller should poll the receiver and handle events appropriately.
    pub fn run(
        &self,
        prompt: &str,
    ) -> (
        mpsc::Receiver<AgentEvent>,
        tokio::task::JoinHandle<AgentResult>,
    ) {
        let (tx, rx) = mpsc::channel(256);

        let api_key = self.api_key.clone();
        let model = self.model.clone();
        let prompt = prompt.to_string();
        let work_dir = self.work_dir.clone();
        let config = self.config.clone();

        let handle = tokio::spawn(async move {
            run_agent_loop(&api_key, &model, &prompt, &work_dir, config, tx).await
        });

        (rx, handle)
    }

    /// Run agent synchronously, blocking until complete
    ///
    /// Useful for CLI batch mode.
    pub async fn run_blocking(&self, prompt: &str) -> AgentResult {
        let (mut rx, handle) = self.run(prompt);

        // Drain events (caller can also process them if needed)
        while rx.recv().await.is_some() {}

        handle.await.unwrap_or_else(|e| AgentResult {
            iterations: 0,
            tool_calls_executed: 0,
            final_response: String::new(),
            success: false,
            error: Some(e.to_string()),
            tokens_used: 0,
        })
    }

    /// Run agent with event callback
    ///
    /// Events are passed to callback as they arrive.
    pub async fn run_with_callback<F>(&self, prompt: &str, mut on_event: F) -> AgentResult
    where
        F: FnMut(&AgentEvent),
    {
        let (mut rx, handle) = self.run(prompt);

        while let Some(event) = rx.recv().await {
            on_event(&event);
        }

        handle.await.unwrap_or_else(|e| AgentResult {
            iterations: 0,
            tool_calls_executed: 0,
            final_response: String::new(),
            success: false,
            error: Some(e.to_string()),
            tokens_used: 0,
        })
    }
}

// Make AgentConfig cloneable for AgentCore
impl Clone for AgentConfig {
    fn clone(&self) -> Self {
        Self {
            max_iterations: self.max_iterations,
            max_tool_calls_per_iteration: self.max_tool_calls_per_iteration,
            timeout_per_tool_ms: self.timeout_per_tool_ms,
            extend_on_progress: self.extend_on_progress,
            bonus_iterations: self.bonus_iterations,
            max_consecutive_failures: self.max_consecutive_failures,
            retry_on_failure: self.retry_on_failure,
        }
    }
}

/// Simplified agent runner that collects output to a callback
pub async fn run_agent_simple<F>(
    api_key: &str,
    model: &str,
    prompt: &str,
    work_dir: &Path,
    mut on_event: F,
) -> AgentResult
where
    F: FnMut(AgentEvent) + Send + 'static,
{
    let (tx, mut rx) = mpsc::channel(256);
    let config = AgentConfig::default();

    let api_key = api_key.to_string();
    let model = model.to_string();
    let prompt = prompt.to_string();
    let work_dir = work_dir.to_path_buf();

    let handle = tokio::spawn(async move {
        run_agent_loop(&api_key, &model, &prompt, &work_dir, config, tx).await
    });

    // Forward events to callback
    while let Some(event) = rx.recv().await {
        on_event(event);
    }

    handle.await.unwrap_or_else(|e| AgentResult {
        iterations: 0,
        tool_calls_executed: 0,
        final_response: String::new(),
        success: false,
        error: Some(e.to_string()),
        tokens_used: 0,
    })
}

// ═══════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_json_block_tool_format() {
        let response = r#"
I'll read the file.

```json
{"tool": "read", "args": {"path": "src/main.rs"}}
```
"#;

        let calls = parse_tool_calls(response);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "read");
        assert_eq!(calls[0].args["path"], "src/main.rs");
    }

    #[test]
    fn test_parse_json_block_name_format() {
        let response = r#"
```json
{"name": "bash", "args": {"command": "cargo test"}}
```
"#;

        let calls = parse_tool_calls(response);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "bash");
        assert_eq!(calls[0].args["command"], "cargo test");
    }

    #[test]
    fn test_parse_json_block_direct_tool() {
        let response = r#"
```json
{"read": {"path": "Cargo.toml"}}
```
"#;

        let calls = parse_tool_calls(response);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "read");
        assert_eq!(calls[0].args["path"], "Cargo.toml");
    }

    #[test]
    fn test_parse_tool_tags() {
        let response = r#"
Let me check the file.

<tool>{"tool": "read", "args": {"path": "README.md"}}</tool>
"#;

        let calls = parse_tool_calls(response);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "read");
    }

    #[test]
    fn test_parse_function_calls() {
        let response = r#"
I'll read the file: read(path="src/lib.rs")
"#;

        let calls = parse_tool_calls(response);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "read");
        assert_eq!(calls[0].args["path"], "src/lib.rs");
    }

    #[test]
    fn test_parse_multiple_tools() {
        let response = r#"
First, let me check the files:

```json
{"tool": "glob", "args": {"pattern": "src/*.rs"}}
```

Then read one:

```json
{"tool": "read", "args": {"path": "src/main.rs"}}
```
"#;

        let calls = parse_tool_calls(response);
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].name, "glob");
        assert_eq!(calls[1].name, "read");
    }

    #[test]
    fn test_parse_no_tools() {
        let response = "This is just a regular response with no tool calls.";

        let calls = parse_tool_calls(response);
        assert!(calls.is_empty());
    }

    #[test]
    fn test_parse_ignores_unknown_functions() {
        let response = "Call some_function(arg=\"value\") which is not a tool.";

        let calls = parse_tool_calls(response);
        assert!(calls.is_empty());
    }

    #[test]
    fn test_is_task_complete_positive() {
        assert!(is_task_complete(
            "I have finished the implementation. Task complete."
        ));
        assert!(is_task_complete(
            "All changes have been successfully applied."
        ));
        assert!(is_task_complete("Done! The tests are now passing."));
    }

    #[test]
    fn test_is_task_complete_negative() {
        assert!(!is_task_complete("Let me read the file first."));
        assert!(!is_task_complete("I need to make more changes."));
    }

    #[test]
    fn test_is_fatal_error() {
        assert!(is_fatal_error("I cannot proceed without more information."));
        assert!(is_fatal_error("Fatal error: file not found."));
        assert!(!is_fatal_error("This is a normal response."));
    }

    #[test]
    fn test_is_known_tool() {
        assert!(is_known_tool("read"));
        assert!(is_known_tool("write"));
        assert!(is_known_tool("bash"));
        assert!(is_known_tool("glob"));
        assert!(is_known_tool("grep"));
        assert!(!is_known_tool("unknown"));
        assert!(!is_known_tool("println"));
    }

    #[test]
    fn test_code_assistant_prompt() {
        let prompt = code_assistant_prompt(Path::new("/home/user/project"));

        assert!(prompt.contains("hyle"));
        assert!(prompt.contains("/home/user/project"));
        assert!(prompt.contains("read"));
        assert!(prompt.contains("write"));
        assert!(prompt.contains("patch"));
        assert!(prompt.contains("bash"));
        assert!(prompt.contains("Task complete"));
    }

    #[test]
    fn test_detect_diffs_in_code_block() {
        let response = r#"Here's the change:

```diff
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,3 +1,3 @@
 fn main() {
-    println!("hello");
+    println!("hello world");
 }
```
"#;
        let diffs = detect_diffs(response);
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].target_file, Some("src/main.rs".to_string()));
        assert!(diffs[0].is_unified);
    }

    #[test]
    fn test_detect_multiple_diffs() {
        let response = r#"
```diff
--- a/file1.rs
+++ b/file1.rs
@@ -1 +1 @@
-old
+new
```

```diff
--- a/file2.rs
+++ b/file2.rs
@@ -1 +1 @@
-foo
+bar
```
"#;
        let diffs = detect_diffs(response);
        assert_eq!(diffs.len(), 2);
    }

    #[test]
    fn test_detect_no_diffs() {
        let response = "This is just a regular response with no diffs.";
        let diffs = detect_diffs(response);
        assert!(diffs.is_empty());
    }

    #[test]
    fn test_parse_patch_tool() {
        let response = r#"
```json
{"tool": "patch", "args": {"path": "test.rs", "diff": "--- a/test.rs\n+++ b/test.rs\n@@ -1 +1 @@\n-old\n+new"}}
```
"#;
        let calls = parse_tool_calls(response);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "patch");
        assert_eq!(calls[0].args["path"], "test.rs");
    }

    #[test]
    fn test_format_tool_results() {
        let mut tracker = ToolCallTracker::new();
        let mut executor = ToolExecutor::new();

        let call = ToolCall::new(
            "bash",
            serde_json::json!({
                "command": "echo test"
            }),
        );
        let idx = tracker.add(call.clone());
        executor.execute(tracker.get_mut(idx).unwrap()).ok();

        let output = format_tool_results(&tracker, &[idx]);
        assert!(output.contains("bash result"));
        assert!(output.contains("test"));
    }

    #[test]
    fn test_format_tool_results_error() {
        let mut tracker = ToolCallTracker::new();

        let mut call = ToolCall::new("test", serde_json::json!({}));
        call.start();
        call.fail("something went wrong");
        let idx = tracker.add(call);

        let output = format_tool_results(&tracker, &[idx]);
        assert!(output.contains("ERROR"));
        assert!(output.contains("something went wrong"));
    }

    #[test]
    fn test_parse_json_array() {
        let response = r#"
```json
[
    {"tool": "read", "args": {"path": "a.rs"}},
    {"tool": "read", "args": {"path": "b.rs"}}
]
```
"#;

        let calls = parse_tool_calls(response);
        assert_eq!(calls.len(), 2);
    }

    #[test]
    fn test_function_call_with_multiple_args() {
        let response = r#"grep(pattern="fn main", path="src/main.rs")"#;

        let calls = parse_tool_calls(response);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "grep");
        assert_eq!(calls[0].args["pattern"], "fn main");
        assert_eq!(calls[0].args["path"], "src/main.rs");
    }

    #[test]
    fn test_execute_tool_calls() {
        let parsed = vec![ParsedToolCall {
            name: "bash".to_string(),
            args: serde_json::json!({"command": "echo hello"}),
        }];

        let mut executor = ToolExecutor::new();
        let mut tracker = ToolCallTracker::new();

        let results = execute_tool_calls(&parsed, &mut executor, &mut tracker);

        assert_eq!(results.len(), 1);
        assert!(results[0].1.is_ok());

        let call = tracker.get(results[0].0).unwrap();
        assert_eq!(call.status, ToolCallStatus::Done);
        assert!(call.get_output().contains("hello"));
    }
}
