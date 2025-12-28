//! Skills, tools, and subagent extension system
//!
//! Provides a simple, composable way to extend hyle with:
//! - Skills: High-level capabilities (e.g., "refactor", "test", "explain")
//! - Tools: Low-level operations (e.g., "read_file", "write_file", "run_command")
//! - Subagents: Specialized workers for specific tasks

#![allow(dead_code)] // Forward-looking module
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::bootstrap::SelfAnalyzer;
use crate::backburner::parse_test_output;
use crate::prompts::{PromptLibrary, Toolbelt};

// ═══════════════════════════════════════════════════════════════
// TOOL DEFINITIONS
// ═══════════════════════════════════════════════════════════════

/// Tool parameter schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolParam {
    pub name: String,
    pub param_type: String,
    pub description: String,
    pub required: bool,
}

/// Tool definition for LLM function calling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub parameters: Vec<ToolParam>,
}

/// Result of a tool invocation
#[derive(Debug, Clone)]
pub struct ToolResult {
    pub success: bool,
    pub output: String,
    pub artifacts: Vec<Artifact>,
}

/// An artifact produced by a tool
#[derive(Debug, Clone)]
pub struct Artifact {
    pub kind: ArtifactKind,
    pub path: Option<PathBuf>,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ArtifactKind {
    Diff,
    File,
    Log,
    Error,
}

// ═══════════════════════════════════════════════════════════════
// BUILT-IN TOOLS
// ═══════════════════════════════════════════════════════════════

/// Read a file and return its contents
pub fn tool_read_file(path: &str) -> ToolResult {
    match std::fs::read_to_string(path) {
        Ok(content) => ToolResult {
            success: true,
            output: content,
            artifacts: vec![],
        },
        Err(e) => ToolResult {
            success: false,
            output: format!("Error reading {}: {}", path, e),
            artifacts: vec![],
        },
    }
}

/// Write content to a file
pub fn tool_write_file(path: &str, content: &str) -> ToolResult {
    match std::fs::write(path, content) {
        Ok(()) => ToolResult {
            success: true,
            output: format!("Wrote {} bytes to {}", content.len(), path),
            artifacts: vec![Artifact {
                kind: ArtifactKind::File,
                path: Some(PathBuf::from(path)),
                content: content.to_string(),
            }],
        },
        Err(e) => ToolResult {
            success: false,
            output: format!("Error writing {}: {}", path, e),
            artifacts: vec![],
        },
    }
}

/// List files matching a pattern
pub fn tool_glob(pattern: &str) -> ToolResult {
    match globwalk::glob(pattern) {
        Ok(walker) => {
            let files: Vec<String> = walker
                .filter_map(|e| e.ok())
                .map(|e| e.path().display().to_string())
                .collect();

            ToolResult {
                success: true,
                output: files.join("\n"),
                artifacts: vec![],
            }
        }
        Err(e) => ToolResult {
            success: false,
            output: format!("Glob error: {}", e),
            artifacts: vec![],
        },
    }
}

/// Run a shell command
pub fn tool_shell(command: &str, cwd: Option<&str>) -> ToolResult {
    use std::process::Command;

    let mut cmd = Command::new("sh");
    cmd.arg("-c").arg(command);

    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }

    match cmd.output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let combined = if stderr.is_empty() {
                stdout
            } else {
                format!("{}\n--- stderr ---\n{}", stdout, stderr)
            };

            ToolResult {
                success: output.status.success(),
                output: combined,
                artifacts: vec![],
            }
        }
        Err(e) => ToolResult {
            success: false,
            output: format!("Command error: {}", e),
            artifacts: vec![],
        },
    }
}

// ═══════════════════════════════════════════════════════════════
// GIT OPERATIONS
// ═══════════════════════════════════════════════════════════════

/// Git repository operations
pub mod git {
    use super::*;

    /// Check if current directory is a git repo
    pub fn is_repo() -> bool {
        std::path::Path::new(".git").exists()
    }

    /// Get current branch
    pub fn current_branch() -> Option<String> {
        let result = tool_shell("git branch --show-current", None);
        if result.success {
            Some(result.output.trim().to_string())
        } else {
            None
        }
    }

    /// Get git status
    pub fn status() -> ToolResult {
        tool_shell("git status --short", None)
    }

    /// Get git diff
    pub fn diff(staged: bool) -> ToolResult {
        let cmd = if staged { "git diff --cached" } else { "git diff" };
        tool_shell(cmd, None)
    }

    /// Stage files
    pub fn add(paths: &[&str]) -> ToolResult {
        let files = paths.join(" ");
        tool_shell(&format!("git add {}", files), None)
    }

    /// Commit with message
    pub fn commit(message: &str) -> ToolResult {
        tool_shell(&format!("git commit -m '{}'", message.replace('\'', "\\'")), None)
    }

    /// Get recent commits
    pub fn log(count: usize) -> ToolResult {
        tool_shell(&format!("git log --oneline -n {}", count), None)
    }

    /// Get changed files
    pub fn changed_files() -> Vec<String> {
        let result = tool_shell("git diff --name-only HEAD", None);
        if result.success {
            result.output.lines().map(|s| s.to_string()).collect()
        } else {
            vec![]
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// SKILL DEFINITIONS
// ═══════════════════════════════════════════════════════════════

/// A skill is a higher-level capability composed of tools
#[derive(Debug, Clone)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub prompt_template: String,
    pub required_tools: Vec<String>,
}

/// Built-in skills
pub fn builtin_skills() -> Vec<Skill> {
    vec![
        Skill {
            name: "build".into(),
            description: "Build the project".into(),
            prompt_template: "Build the project and report any errors.".into(),
            required_tools: vec!["shell".into()],
        },
        Skill {
            name: "test".into(),
            description: "Run project tests".into(),
            prompt_template: "Run the project tests and report results.".into(),
            required_tools: vec!["shell".into()],
        },
        Skill {
            name: "update".into(),
            description: "Update dependencies or self".into(),
            prompt_template: "Update project dependencies to latest versions.".into(),
            required_tools: vec!["shell".into()],
        },
        Skill {
            name: "explain".into(),
            description: "Explain code in detail".into(),
            prompt_template: "Explain the following code:\n\n{code}\n\nProvide a clear explanation.".into(),
            required_tools: vec!["read_file".into()],
        },
        Skill {
            name: "refactor".into(),
            description: "Refactor code for better quality".into(),
            prompt_template: "Refactor the following code for better readability and maintainability:\n\n{code}\n\nProvide the refactored code as a unified diff.".into(),
            required_tools: vec!["read_file".into(), "write_file".into()],
        },
        Skill {
            name: "test".into(),
            description: "Generate tests for code".into(),
            prompt_template: "Generate unit tests for the following code:\n\n{code}\n\nUse the appropriate testing framework.".into(),
            required_tools: vec!["read_file".into(), "write_file".into()],
        },
        Skill {
            name: "fix".into(),
            description: "Fix a bug or issue".into(),
            prompt_template: "Fix the following issue in this code:\n\nIssue: {issue}\n\nCode:\n{code}\n\nProvide the fix as a unified diff.".into(),
            required_tools: vec!["read_file".into(), "write_file".into()],
        },
        Skill {
            name: "document".into(),
            description: "Add documentation to code".into(),
            prompt_template: "Add comprehensive documentation to the following code:\n\n{code}\n\nInclude function docs, inline comments where helpful, and module-level docs.".into(),
            required_tools: vec!["read_file".into(), "write_file".into()],
        },
        Skill {
            name: "review".into(),
            description: "Review code for issues".into(),
            prompt_template: "Review the following code for:\n- Bugs\n- Security issues\n- Performance problems\n- Style issues\n\nCode:\n{code}".into(),
            required_tools: vec!["read_file".into()],
        },
    ]
}

// ═══════════════════════════════════════════════════════════════
// SUBAGENT DEFINITIONS
// ═══════════════════════════════════════════════════════════════

/// A subagent is a specialized worker with its own context
#[derive(Debug, Clone)]
pub struct SubagentDef {
    pub name: String,
    pub description: String,
    pub system_prompt: String,
    pub model: Option<String>,  // Override model if needed
}

/// Built-in subagents
pub fn builtin_subagents() -> Vec<SubagentDef> {
    vec![
        SubagentDef {
            name: "planner".into(),
            description: "Plans multi-step tasks".into(),
            system_prompt: "You are a planning agent. Break down tasks into clear, actionable steps. Output a numbered list of steps.".into(),
            model: None,
        },
        SubagentDef {
            name: "coder".into(),
            description: "Writes and modifies code".into(),
            system_prompt: "You are a coding agent. Write clean, well-tested code. Always output changes as unified diffs.".into(),
            model: None,
        },
        SubagentDef {
            name: "reviewer".into(),
            description: "Reviews code for issues".into(),
            system_prompt: "You are a code review agent. Look for bugs, security issues, and style problems. Be thorough but constructive.".into(),
            model: None,
        },
        SubagentDef {
            name: "debugger".into(),
            description: "Debugs issues".into(),
            system_prompt: "You are a debugging agent. Analyze errors, trace issues, and suggest fixes. Ask clarifying questions when needed.".into(),
            model: None,
        },
    ]
}

// ═══════════════════════════════════════════════════════════════
// TOOL REGISTRY
// ═══════════════════════════════════════════════════════════════

/// Registry of available tools
pub struct ToolRegistry {
    tools: HashMap<String, ToolDef>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            tools: HashMap::new(),
        };

        // Register built-in tools
        registry.register(ToolDef {
            name: "read_file".into(),
            description: "Read contents of a file".into(),
            parameters: vec![ToolParam {
                name: "path".into(),
                param_type: "string".into(),
                description: "Path to the file".into(),
                required: true,
            }],
        });

        registry.register(ToolDef {
            name: "write_file".into(),
            description: "Write content to a file".into(),
            parameters: vec![
                ToolParam {
                    name: "path".into(),
                    param_type: "string".into(),
                    description: "Path to the file".into(),
                    required: true,
                },
                ToolParam {
                    name: "content".into(),
                    param_type: "string".into(),
                    description: "Content to write".into(),
                    required: true,
                },
            ],
        });

        registry.register(ToolDef {
            name: "glob".into(),
            description: "List files matching a pattern".into(),
            parameters: vec![ToolParam {
                name: "pattern".into(),
                param_type: "string".into(),
                description: "Glob pattern (e.g., '**/*.rs')".into(),
                required: true,
            }],
        });

        registry.register(ToolDef {
            name: "shell".into(),
            description: "Run a shell command".into(),
            parameters: vec![
                ToolParam {
                    name: "command".into(),
                    param_type: "string".into(),
                    description: "Command to run".into(),
                    required: true,
                },
                ToolParam {
                    name: "cwd".into(),
                    param_type: "string".into(),
                    description: "Working directory".into(),
                    required: false,
                },
            ],
        });

        registry.register(ToolDef {
            name: "git_status".into(),
            description: "Get git repository status".into(),
            parameters: vec![],
        });

        registry.register(ToolDef {
            name: "git_diff".into(),
            description: "Get git diff".into(),
            parameters: vec![ToolParam {
                name: "staged".into(),
                param_type: "boolean".into(),
                description: "Show staged changes only".into(),
                required: false,
            }],
        });

        registry.register(ToolDef {
            name: "git_commit".into(),
            description: "Commit staged changes".into(),
            parameters: vec![ToolParam {
                name: "message".into(),
                param_type: "string".into(),
                description: "Commit message".into(),
                required: true,
            }],
        });

        registry
    }

    pub fn register(&mut self, tool: ToolDef) {
        self.tools.insert(tool.name.clone(), tool);
    }

    pub fn get(&self, name: &str) -> Option<&ToolDef> {
        self.tools.get(name)
    }

    pub fn all(&self) -> Vec<&ToolDef> {
        self.tools.values().collect()
    }

    /// Convert to OpenRouter tool format
    pub fn to_openrouter_format(&self) -> Vec<serde_json::Value> {
        self.tools.values().map(|t| {
            let properties: serde_json::Map<String, serde_json::Value> = t.parameters.iter().map(|p| {
                (p.name.clone(), serde_json::json!({
                    "type": p.param_type,
                    "description": p.description
                }))
            }).collect();

            let required: Vec<String> = t.parameters.iter()
                .filter(|p| p.required)
                .map(|p| p.name.clone())
                .collect();

            serde_json::json!({
                "type": "function",
                "function": {
                    "name": t.name,
                    "description": t.description,
                    "parameters": {
                        "type": "object",
                        "properties": properties,
                        "required": required
                    }
                }
            })
        }).collect()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════════
// SLASH COMMANDS
// ═══════════════════════════════════════════════════════════════

/// Result of a slash command
#[derive(Debug)]
pub struct SlashResult {
    pub output: String,
    pub success: bool,
}

impl From<ToolResult> for SlashResult {
    fn from(r: ToolResult) -> Self {
        SlashResult {
            output: r.output,
            success: r.success,
        }
    }
}

/// Slash command context for stateful commands
pub struct SlashContext {
    pub project_type: Option<String>,
    pub model: String,
    pub session_id: String,
    pub total_tokens: u64,
    pub message_count: usize,
}

/// Execute a slash command directly (no LLM involved)
pub fn execute_slash_command(cmd: &str, project_type: Option<&str>) -> Option<SlashResult> {
    execute_slash_command_with_context(cmd, project_type, None)
}

/// Execute a slash command with full context
pub fn execute_slash_command_with_context(
    cmd: &str,
    project_type: Option<&str>,
    ctx: Option<&SlashContext>,
) -> Option<SlashResult> {
    let parts: Vec<&str> = cmd.trim().splitn(2, ' ').collect();
    let command = parts.first()?.trim_start_matches('/');
    let args = parts.get(1).copied().unwrap_or("");

    match command {
        // === Project Commands ===
        "build" => Some(run_build(project_type)),
        "test" => Some(run_test(project_type)),
        "update" => Some(run_update(project_type)),
        "clean" => Some(run_clean(project_type)),
        "check" | "lint" => Some(run_check(project_type)),

        // === Session Commands ===
        "clear" => Some(SlashResult {
            output: "CLEAR_CONVERSATION".into(),
            success: true,
        }),
        "compact" => Some(SlashResult {
            output: "COMPACT_CONVERSATION".into(),
            success: true,
        }),
        "cost" | "tokens" | "usage" => Some(run_cost(ctx)),
        "status" => {
            // /status git → git status, otherwise project status
            if args == "git" {
                Some(git::status().into())
            } else {
                Some(run_status(project_type, ctx))
            }
        }

        // === Git Commands ===
        "git" => Some(run_git(args)),
        "diff" => Some(git::diff(args == "staged" || args == "--staged").into()),
        "commit" => Some(run_commit(args)),

        // === Navigation ===
        "cd" => Some(run_cd(args)),
        "ls" | "files" => Some(run_ls(args)),
        "find" | "glob" => Some(tool_glob(if args.is_empty() { "**/*" } else { args }).into()),
        "grep" | "search" => Some(run_grep(args)),

        // === Utility ===
        "help" | "?" => Some(slash_help_full()),
        "doctor" => Some(run_doctor()),
        "version" => Some(SlashResult {
            output: format!("hyle v{}", env!("CARGO_PKG_VERSION")),
            success: true,
        }),
        "model" | "models" => Some(SlashResult {
            output: ctx.map(|c| format!("Current model: {}", c.model)).unwrap_or_else(|| "unknown".into()),
            success: true,
        }),
        "switch" => Some(SlashResult {
            // Return the switch target - ui.rs will handle actual switching
            output: if args.is_empty() {
                "SWITCH_MODEL_PICKER".into() // Signal to show picker
            } else {
                format!("SWITCH_MODEL:{}", args) // Signal to switch to specific model
            },
            success: true,
        }),

        // === Editor Integration ===
        "edit" | "open" => Some(run_edit(args)),
        "view" | "cat" | "read" => Some(tool_read_file(args).into()),

        // === Self-Analysis ===
        "analyze" | "health" => Some(run_analyze()),
        "improve" => Some(run_improve()),
        "deps" | "graph" => Some(run_deps()),
        "selftest" => Some(run_selftest()),

        // === Patch Operations ===
        "apply" => Some(run_apply(args)),
        "revert" => Some(run_revert(args)),

        // === Prompt Library ===
        "toolbelt" => Some(run_toolbelt(args)),
        "prompts" => Some(run_prompts()),

        _ => None, // Unknown command, let LLM handle
    }
}

fn run_build(project_type: Option<&str>) -> SlashResult {
    let cmd = match project_type {
        Some("Rust") => "cargo build",
        Some("Node.js") => "npm run build",
        Some("Python") => "python -m build",
        Some("Go") => "go build ./...",
        _ => "make build 2>/dev/null || cargo build 2>/dev/null || npm run build 2>/dev/null",
    };
    let result = tool_shell(cmd, None);
    SlashResult { output: result.output, success: result.success }
}

fn run_test(project_type: Option<&str>) -> SlashResult {
    let cmd = match project_type {
        Some("Rust") => "cargo test",
        Some("Node.js") => "npm test",
        Some("Python") => "pytest",
        Some("Go") => "go test ./...",
        _ => "make test 2>/dev/null || cargo test 2>/dev/null || npm test 2>/dev/null || pytest 2>/dev/null",
    };
    let result = tool_shell(cmd, None);
    SlashResult { output: result.output, success: result.success }
}

fn run_update(project_type: Option<&str>) -> SlashResult {
    let cmd = match project_type {
        Some("Rust") => "cargo update",
        Some("Node.js") => "npm update",
        Some("Python") => "pip install --upgrade -r requirements.txt",
        Some("Go") => "go get -u ./...",
        _ => "cargo update 2>/dev/null || npm update 2>/dev/null",
    };
    let result = tool_shell(cmd, None);
    SlashResult { output: result.output, success: result.success }
}

fn run_clean(project_type: Option<&str>) -> SlashResult {
    let cmd = match project_type {
        Some("Rust") => "cargo clean",
        Some("Node.js") => "rm -rf node_modules && npm install",
        Some("Python") => "find . -type d -name __pycache__ -exec rm -rf {} +",
        Some("Go") => "go clean -cache",
        _ => "cargo clean 2>/dev/null || rm -rf node_modules 2>/dev/null",
    };
    let result = tool_shell(cmd, None);
    SlashResult { output: result.output, success: result.success }
}

fn run_check(project_type: Option<&str>) -> SlashResult {
    let cmd = match project_type {
        Some("Rust") => "cargo check && cargo clippy",
        Some("Node.js") => "npm run lint",
        Some("Python") => "ruff check . || flake8",
        Some("Go") => "go vet ./...",
        _ => "cargo check 2>/dev/null || npm run lint 2>/dev/null",
    };
    let result = tool_shell(cmd, None);
    SlashResult { output: result.output, success: result.success }
}

fn slash_help_full() -> SlashResult {
    SlashResult {
        output: r#"═══ Project ═══
  /build          Build the project
  /test           Run tests
  /check, /lint   Run lints and checks
  /update         Update dependencies
  /clean          Clean build artifacts

═══ Session ═══
  /clear          Clear conversation history
  /compact        Summarize and compact history
  /cost, /tokens  Show token usage
  /status         Show session status
  /model          Show current model
  /switch [name]  Switch to different model

═══ Git ═══
  /git <cmd>      Run git command
  /diff [staged]  Show git diff
  /commit <msg>   Commit with message

═══ Files ═══
  /ls [path]      List files
  /find <pattern> Find files (glob)
  /grep <pattern> Search in files
  /view <file>    View file contents
  /edit <file>    Open in $EDITOR

═══ Utility ═══
  /help, /?       Show this help
  /doctor         Run diagnostics
  /version        Show version
  /cd <path>      Change directory

═══ Self-Analysis ═══
  /analyze        Codebase health analysis
  /improve        Generate improvement suggestions
  /deps           Show module dependency graph
  /selftest       Run cargo test and parse results

═══ Prompt Library ═══
  /toolbelt       Development phase commands
  /prompts        Saved prompts and mappings

═══ Patch Operations ═══
  /apply <file>   Apply unified diff to file
  /revert <file>  Restore from .bak backup"#.into(),
        success: true,
    }
}

fn run_cost(ctx: Option<&SlashContext>) -> SlashResult {
    match ctx {
        Some(c) => SlashResult {
            output: format!(
                "Session: {}\nMessages: {}\nTokens: {}\nModel: {}",
                c.session_id, c.message_count, c.total_tokens, c.model
            ),
            success: true,
        },
        None => SlashResult {
            output: "No session context available".into(),
            success: false,
        },
    }
}

fn run_status(project_type: Option<&str>, ctx: Option<&SlashContext>) -> SlashResult {
    let mut lines = vec![];

    if let Some(pt) = project_type {
        lines.push(format!("Project type: {}", pt));
    }

    if let Some(c) = ctx {
        lines.push(format!("Model: {}", c.model));
        lines.push(format!("Session: {}", c.session_id));
        lines.push(format!("Messages: {}", c.message_count));
        lines.push(format!("Tokens: {}", c.total_tokens));
    }

    if git::is_repo() {
        if let Some(branch) = git::current_branch() {
            lines.push(format!("Git branch: {}", branch));
        }
    }

    SlashResult {
        output: if lines.is_empty() { "No status available".into() } else { lines.join("\n") },
        success: true,
    }
}

fn run_git(args: &str) -> SlashResult {
    if args.is_empty() {
        git::status().into()
    } else {
        tool_shell(&format!("git {}", args), None).into()
    }
}

fn run_commit(msg: &str) -> SlashResult {
    if msg.is_empty() {
        SlashResult {
            output: "Usage: /commit <message>".into(),
            success: false,
        }
    } else {
        git::commit(msg).into()
    }
}

fn run_cd(path: &str) -> SlashResult {
    if path.is_empty() {
        SlashResult {
            output: std::env::current_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| "unknown".into()),
            success: true,
        }
    } else {
        match std::env::set_current_dir(path) {
            Ok(()) => SlashResult {
                output: format!("Changed to: {}", path),
                success: true,
            },
            Err(e) => SlashResult {
                output: format!("cd failed: {}", e),
                success: false,
            },
        }
    }
}

fn run_ls(path: &str) -> SlashResult {
    let target = if path.is_empty() { "." } else { path };
    tool_shell(&format!("ls -la {}", target), None).into()
}

fn run_grep(args: &str) -> SlashResult {
    if args.is_empty() {
        SlashResult {
            output: "Usage: /grep <pattern> [path]".into(),
            success: false,
        }
    } else {
        tool_shell(&format!("grep -rn --color=never {} .", args), None).into()
    }
}

fn run_edit(path: &str) -> SlashResult {
    if path.is_empty() {
        SlashResult {
            output: "Usage: /edit <file>".into(),
            success: false,
        }
    } else {
        let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".into());
        SlashResult {
            output: format!("Open with: {} {}", editor, path),
            success: true,
        }
    }
}

fn run_improve() -> SlashResult {
    match SelfAnalyzer::new() {
        Ok(analyzer) => {
            match analyzer.improvement_prompt() {
                Ok(prompt) => SlashResult {
                    output: prompt,
                    success: true,
                },
                Err(e) => SlashResult {
                    output: format!("Failed to generate improvement prompt: {}", e),
                    success: false,
                }
            }
        }
        Err(e) => SlashResult {
            output: format!("Not in hyle project: {}", e),
            success: false,
        }
    }
}

fn run_deps() -> SlashResult {
    match SelfAnalyzer::new() {
        Ok(analyzer) => {
            match analyzer.dependency_graph() {
                Ok(graph) => SlashResult {
                    output: format!("Module Dependencies (Mermaid):\n\n```mermaid\n{}\n```", graph),
                    success: true,
                },
                Err(e) => SlashResult {
                    output: format!("Failed to generate dependency graph: {}", e),
                    success: false,
                }
            }
        }
        Err(e) => SlashResult {
            output: format!("Not in hyle project: {}", e),
            success: false,
        }
    }
}

fn run_apply(args: &str) -> SlashResult {
    use crate::tools::{apply_patch, preview_changes};

    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    if parts.is_empty() || parts[0].is_empty() {
        return SlashResult {
            output: "Usage: /apply <file> [diff]\n\nApplies a unified diff to a file.\nIf diff is not provided, reads from stdin or last clipboard.\n\nExamples:\n  /apply src/main.rs\n  /apply src/main.rs \"--- a/...\"".into(),
            success: false,
        };
    }

    let path = parts[0];
    let file_path = std::path::Path::new(path);

    // Read original file
    let original = match std::fs::read_to_string(file_path) {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return SlashResult {
            output: format!("Failed to read {}: {}", path, e),
            success: false,
        }
    };

    // Get diff content
    let diff = if parts.len() > 1 {
        parts[1].to_string()
    } else {
        // Try to get from environment or return usage
        return SlashResult {
            output: format!(
                "No diff provided. Usage: /apply {} \"<diff content>\"\n\nOr pipe diff: cat patch.diff | hyle /apply {}",
                path, path
            ),
            success: false,
        };
    };

    // Apply patch
    match apply_patch(&original, &diff) {
        Ok(patched) => {
            // Show preview
            let preview = preview_changes(&original, &patched, path);

            // Backup original
            if file_path.exists() {
                let backup = file_path.with_extension("bak");
                if let Err(e) = std::fs::copy(file_path, &backup) {
                    return SlashResult {
                        output: format!("Failed to backup: {}", e),
                        success: false,
                    };
                }
            }

            // Write patched content
            match std::fs::write(file_path, &patched) {
                Ok(()) => SlashResult {
                    output: format!("{}\n\nApplied successfully. Backup saved as {}.bak", preview, path),
                    success: true,
                },
                Err(e) => SlashResult {
                    output: format!("Failed to write: {}", e),
                    success: false,
                }
            }
        }
        Err(e) => SlashResult {
            output: format!("Failed to apply patch: {}", e),
            success: false,
        }
    }
}

fn run_revert(args: &str) -> SlashResult {
    if args.is_empty() {
        return SlashResult {
            output: "Usage: /revert <file>\n\nRestores a file from its .bak backup.".into(),
            success: false,
        };
    }

    let path = std::path::Path::new(args);
    let backup = path.with_extension("bak");

    if !backup.exists() {
        return SlashResult {
            output: format!("No backup found: {}", backup.display()),
            success: false,
        };
    }

    match std::fs::copy(&backup, path) {
        Ok(_) => {
            // Remove backup after successful restore
            std::fs::remove_file(&backup).ok();
            SlashResult {
                output: format!("Reverted {} from backup", args),
                success: true,
            }
        }
        Err(e) => SlashResult {
            output: format!("Failed to revert: {}", e),
            success: false,
        }
    }
}

fn run_selftest() -> SlashResult {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    // Check if we're in a Rust project
    if !cwd.join("Cargo.toml").exists() {
        return SlashResult {
            output: "Not in a Rust project (no Cargo.toml)".into(),
            success: false,
        };
    }

    // Run cargo test and capture output
    let start = std::time::Instant::now();
    let output = std::process::Command::new("cargo")
        .args(["test", "--", "--color=never"])
        .current_dir(&cwd)
        .output();

    let duration = start.elapsed();

    match output {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);

            let results = parse_test_output(&stdout, &stderr);

            let mut out = String::new();
            out.push_str(&format!("Test Results ({:.1}s)\n", duration.as_secs_f64()));
            out.push_str("═══════════════════════════\n");
            out.push_str(&format!("Passed:  {}\n", results.passed));
            out.push_str(&format!("Failed:  {}\n", results.failed));
            out.push_str(&format!("Ignored: {}\n", results.ignored));

            if !results.failed_tests.is_empty() {
                out.push_str("\nFailed Tests:\n");
                for name in &results.failed_tests {
                    out.push_str(&format!("  ✗ {}\n", name));
                }
            }

            // Add summary
            if results.success() {
                out.push_str("\n✓ All tests passed!");
            } else {
                out.push_str(&format!("\n✗ {} tests failed", results.failed));
            }

            SlashResult {
                output: out,
                success: results.success(),
            }
        }
        Err(e) => SlashResult {
            output: format!("Failed to run tests: {}", e),
            success: false,
        }
    }
}

fn run_analyze() -> SlashResult {
    match SelfAnalyzer::new() {
        Ok(analyzer) => {
            match analyzer.analyze() {
                Ok(analysis) => {
                    let mut output = format!("{}", analysis);

                    // Add top modules by size
                    output.push_str("\nTop Modules:\n");
                    for m in analysis.modules.iter().take(5) {
                        output.push_str(&format!(
                            "  {} ({} lines, {} tests, {:.0}% doc)\n",
                            m.name, m.lines, m.tests, m.doc_coverage * 100.0
                        ));
                    }

                    // Add high priority TODOs
                    let high_todos: Vec<_> = analysis.todos.iter()
                        .filter(|t| t.priority == crate::bootstrap::TodoPriority::High)
                        .take(5)
                        .collect();

                    if !high_todos.is_empty() {
                        output.push_str("\nHigh Priority TODOs:\n");
                        for todo in high_todos {
                            output.push_str(&format!(
                                "  {}:{}: {}\n",
                                todo.file.file_name().unwrap_or_default().to_string_lossy(),
                                todo.line,
                                todo.text.chars().take(60).collect::<String>()
                            ));
                        }
                    }

                    SlashResult { output, success: true }
                }
                Err(e) => SlashResult {
                    output: format!("Analysis failed: {}", e),
                    success: false,
                }
            }
        }
        Err(e) => SlashResult {
            output: format!("Not in hyle project: {}", e),
            success: false,
        }
    }
}

fn run_doctor() -> SlashResult {
    let mut lines = vec!["hyle doctor".to_string(), "".to_string()];

    // Check git
    let git_ok = git::is_repo();
    lines.push(format!("[{}] Git repo: {}",
        if git_ok { "✓" } else { "○" },
        if git_ok { "detected" } else { "not a repo" }
    ));

    // Check project files
    let has_cargo = std::path::Path::new("Cargo.toml").exists();
    let has_package = std::path::Path::new("package.json").exists();
    let has_pyproject = std::path::Path::new("pyproject.toml").exists();

    if has_cargo {
        lines.push("[✓] Cargo.toml found (Rust project)".into());
    }
    if has_package {
        lines.push("[✓] package.json found (Node project)".into());
    }
    if has_pyproject {
        lines.push("[✓] pyproject.toml found (Python project)".into());
    }
    if !has_cargo && !has_package && !has_pyproject {
        lines.push("[○] No recognized project manifest".into());
    }

    // Check tools
    let has_rg = tool_shell("which rg", None).success;
    let has_fd = tool_shell("which fd", None).success;
    lines.push(format!("[{}] ripgrep: {}", if has_rg { "✓" } else { "○" }, if has_rg { "available" } else { "not found" }));
    lines.push(format!("[{}] fd: {}", if has_fd { "✓" } else { "○" }, if has_fd { "available" } else { "not found" }));

    SlashResult {
        output: lines.join("\n"),
        success: true,
    }
}

/// Check if input is a slash command
pub fn is_slash_command(input: &str) -> bool {
    input.trim().starts_with('/')
}

/// Show development toolbelt
fn run_toolbelt(args: &str) -> SlashResult {
    let belt = Toolbelt::default();

    // If specific command requested, show its prompt
    if !args.is_empty() {
        if let Some(cmd) = belt.commands.iter().find(|c| c.name == args) {
            return SlashResult {
                output: format!("{}: {}\n\nPrompt: {}", cmd.name, cmd.description, cmd.prompt),
                success: true,
            };
        }
        return SlashResult {
            output: format!("Unknown toolbelt command: {}", args),
            success: false,
        };
    }

    // Show all commands
    let mut lines = vec!["Development Toolbelt:".to_string(), String::new()];

    for cmd in &belt.commands {
        lines.push(format!("  {:12} {:?} - {}", cmd.name, cmd.phase, cmd.description));
    }

    lines.push(String::new());
    lines.push("Use /toolbelt <name> for full prompt".to_string());

    SlashResult {
        output: lines.join("\n"),
        success: true,
    }
}

/// Show saved prompts and command mappings
fn run_prompts() -> SlashResult {
    let lib = PromptLibrary::load().unwrap_or_default();

    let mut lines = vec!["Command Mappings:".to_string(), String::new()];

    for mapping in lib.mappings() {
        lines.push(format!("  \"{}\" → {}", mapping.general, mapping.description));
    }

    lines.push(String::new());
    lines.push("Top Saved Prompts:".to_string());

    for prompt in lib.top_prompts(10) {
        let preview = if prompt.text.len() > 40 {
            format!("{}...", &prompt.text[..40])
        } else {
            prompt.text.clone()
        };
        lines.push(format!("  [{}×] {}", prompt.count, preview));
    }

    if lib.top_prompts(10).is_empty() {
        lines.push("  (none yet - prompts are auto-saved after 2+ uses)".to_string());
    }

    SlashResult {
        output: lines.join("\n"),
        success: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_read_file() {
        let result = tool_read_file("Cargo.toml");
        assert!(result.success);
        assert!(result.output.contains("[package]"));
    }

    #[test]
    fn test_tool_glob() {
        let result = tool_glob("src/*.rs");
        assert!(result.success);
        assert!(result.output.contains("main.rs"));
    }

    #[test]
    fn test_git_is_repo() {
        // This test depends on running in a git repo
        let _ = git::is_repo();
    }

    #[test]
    fn test_tool_registry() {
        let registry = ToolRegistry::new();
        assert!(registry.get("read_file").is_some());
        assert!(registry.get("write_file").is_some());
        assert!(registry.get("shell").is_some());
    }

    #[test]
    fn test_builtin_skills() {
        let skills = builtin_skills();
        assert!(!skills.is_empty());
        assert!(skills.iter().any(|s| s.name == "explain"));
        assert!(skills.iter().any(|s| s.name == "refactor"));
    }
}
