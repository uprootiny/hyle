//! Skills, tools, and subagent extension system
//!
//! Provides a simple, composable way to extend hyle with:
//! - Skills: High-level capabilities (e.g., "refactor", "test", "explain")
//! - Tools: Low-level operations (e.g., "read_file", "write_file", "run_command")
//! - Subagents: Specialized workers for specific tasks

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

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
