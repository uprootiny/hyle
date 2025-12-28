//! Agent module for LLM-driven tool execution
//!
//! Parses LLM responses for tool calls and executes them in a loop.
//! This is the core of self-bootstrapping: hyle using hyle to develop hyle.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::tools::{ToolCall, ToolCallStatus, ToolExecutor, ToolCallTracker};

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
        let arg_re = regex::Regex::new(r#"(\w+)\s*=\s*"([^"]*)""#).unwrap();

        for arg_cap in arg_re.captures_iter(args_str) {
            let key = arg_cap.get(1).map(|m| m.as_str()).unwrap_or("");
            let value = arg_cap.get(2).map(|m| m.as_str()).unwrap_or("");
            args.insert(key.to_string(), serde_json::Value::String(value.to_string()));
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
    matches!(name, "read" | "write" | "glob" | "grep" | "bash" | "edit" | "search")
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

    lower.contains("cannot proceed") ||
    lower.contains("unable to continue") ||
    lower.contains("fatal error") ||
    lower.contains("aborting")
}

// ═══════════════════════════════════════════════════════════════
// AGENT EXECUTION
// ═══════════════════════════════════════════════════════════════

/// Agent configuration
pub struct AgentConfig {
    pub max_iterations: usize,
    pub max_tool_calls_per_iteration: usize,
    pub timeout_per_tool_ms: u64,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_iterations: 20,
            max_tool_calls_per_iteration: 5,
            timeout_per_tool_ms: 60000,
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
}

/// Execute tool calls from a parsed response
pub fn execute_tool_calls(
    calls: &[ParsedToolCall],
    executor: &mut ToolExecutor,
    tracker: &mut ToolCallTracker,
) -> Vec<(usize, Result<()>)> {
    let mut results = Vec::new();

    for parsed in calls {
        let mut call = ToolCall::new(&parsed.name, parsed.args.clone());
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
                    output.push_str(&format!("ERROR: {}\n", call.error.as_deref().unwrap_or("unknown")));
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
    format!(r#"You are hyle, a Rust-native code assistant. You help users with software engineering tasks.

Working directory: {}

Available tools:
- read(path="..."): Read a file with line numbers
- write(path="...", content="..."): Write content to a file (creates backup)
- glob(pattern="..."): Find files matching a glob pattern
- grep(pattern="...", path="..."): Search for regex pattern in files
- bash(command="..."): Execute a shell command

To use a tool, respond with a JSON block:
```json
{{"tool": "read", "args": {{"path": "src/main.rs"}}}}
```

Or use function syntax:
read(path="src/main.rs")

After executing tools, I will show you the results. Continue until the task is complete.

When finished, say "Task complete" and summarize what was done.

Guidelines:
- Read files before modifying them
- Make atomic, focused changes
- Run tests after modifications
- Commit changes with clear messages
"#, work_dir.display())
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
        assert!(is_task_complete("I have finished the implementation. Task complete."));
        assert!(is_task_complete("All changes have been successfully applied."));
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
        assert!(prompt.contains("bash"));
        assert!(prompt.contains("Task complete"));
    }

    #[test]
    fn test_format_tool_results() {
        let mut tracker = ToolCallTracker::new();
        let mut executor = ToolExecutor::new();

        let mut call = ToolCall::new("bash", serde_json::json!({
            "command": "echo test"
        }));
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
        let parsed = vec![
            ParsedToolCall {
                name: "bash".to_string(),
                args: serde_json::json!({"command": "echo hello"}),
            }
        ];

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
