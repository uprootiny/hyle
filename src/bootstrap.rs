//! Self-development bootstrap for hyle
//!
//! Enables hyle to:
//! - Understand its own codebase
//! - Propose and make changes
//! - Run tests before/after
//! - Commit successful changes
//! - Self-analyze for improvements
//! - Keep docs synchronized with code
//! - Detect and repair issues

#![allow(dead_code)] // Forward-looking module for self-bootstrapping

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::git;
use crate::project::{is_self_development, self_project, Project};

// ═══════════════════════════════════════════════════════════════
// DEVELOPMENT TASK
// ═══════════════════════════════════════════════════════════════

/// A self-development task
#[derive(Debug, Clone)]
pub struct DevTask {
    pub description: String,
    pub affected_files: Vec<String>,
    pub test_command: String,
    pub commit_message: Option<String>,
}

impl DevTask {
    pub fn new(description: &str) -> Self {
        Self {
            description: description.to_string(),
            affected_files: Vec::new(),
            test_command: "cargo test".to_string(),
            commit_message: None,
        }
    }

    pub fn with_files(mut self, files: Vec<String>) -> Self {
        self.affected_files = files;
        self
    }

    pub fn with_commit_message(mut self, msg: &str) -> Self {
        self.commit_message = Some(msg.to_string());
        self
    }
}

// ═══════════════════════════════════════════════════════════════
// BOOTSTRAP RUNNER
// ═══════════════════════════════════════════════════════════════

/// Self-development bootstrap runner
pub struct Bootstrap {
    project: Project,
    verbose: bool,
}

impl Bootstrap {
    /// Create bootstrap for hyle's own development
    pub fn new() -> Result<Self> {
        let project = self_project().context("Could not detect hyle project")?;

        if !is_self_development() {
            bail!("Not running in hyle project - self-development disabled");
        }

        Ok(Self {
            project,
            verbose: true,
        })
    }

    /// Create bootstrap for any project
    pub fn for_project(path: &Path) -> Result<Self> {
        let project = Project::detect(path).context("Could not detect project")?;

        Ok(Self {
            project,
            verbose: true,
        })
    }

    /// Get project info
    pub fn project(&self) -> &Project {
        &self.project
    }

    /// Get context for LLM
    pub fn context(&self) -> String {
        self.project.context_for_llm()
    }

    /// Run tests and return success
    pub fn run_tests(&self) -> Result<TestResult> {
        self.log("Running tests...");

        let output = Command::new("cargo")
            .arg("test")
            .current_dir(&self.project.root)
            .output()
            .context("Failed to run cargo test")?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        // Parse test results
        let passed = stdout.contains("test result: ok");
        let test_count = extract_test_count(&stdout);

        Ok(TestResult {
            passed,
            test_count,
            output: format!("{}\n{}", stdout, stderr),
        })
    }

    /// Run clippy and return warnings
    pub fn run_clippy(&self) -> Result<LintResult> {
        self.log("Running clippy...");

        let output = Command::new("cargo")
            .args(["clippy", "--", "-D", "warnings"])
            .current_dir(&self.project.root)
            .output()
            .context("Failed to run cargo clippy")?;

        let stderr = String::from_utf8_lossy(&output.stderr);
        let warning_count = stderr.matches("warning:").count();

        Ok(LintResult {
            passed: output.status.success(),
            warning_count,
            output: stderr.to_string(),
        })
    }

    /// Check if build succeeds
    pub fn check_build(&self) -> Result<bool> {
        self.log("Checking build...");

        let status = Command::new("cargo")
            .arg("check")
            .current_dir(&self.project.root)
            .status()
            .context("Failed to run cargo check")?;

        Ok(status.success())
    }

    /// Execute a development task with guards
    pub fn execute_task<F>(&self, task: &DevTask, make_changes: F) -> Result<TaskResult>
    where
        F: FnOnce() -> Result<Vec<FileChange>>,
    {
        self.log(&format!("Executing task: {}", task.description));

        // Pre-flight checks
        self.log("Pre-flight: running tests...");
        let pre_tests = self.run_tests()?;
        if !pre_tests.passed {
            return Ok(TaskResult {
                success: false,
                message: "Pre-flight tests failed - aborting".to_string(),
                changes: vec![],
                tests_before: pre_tests,
                tests_after: None,
            });
        }
        self.log(&format!(
            "Pre-flight: {} tests passed",
            pre_tests.test_count
        ));

        // Make changes
        self.log("Making changes...");
        let changes = make_changes()?;
        self.log(&format!("Made {} file changes", changes.len()));

        // Post-flight checks
        self.log("Post-flight: running tests...");
        let post_tests = self.run_tests()?;

        if !post_tests.passed {
            self.log("Post-flight tests failed - changes may need review");
            return Ok(TaskResult {
                success: false,
                message: "Post-flight tests failed".to_string(),
                changes,
                tests_before: pre_tests,
                tests_after: Some(post_tests),
            });
        }

        self.log(&format!(
            "Post-flight: {} tests passed",
            post_tests.test_count
        ));

        // Success!
        Ok(TaskResult {
            success: true,
            message: format!(
                "Task completed: {} → {} tests",
                pre_tests.test_count, post_tests.test_count
            ),
            changes,
            tests_before: pre_tests,
            tests_after: Some(post_tests),
        })
    }

    /// Commit changes after successful task
    pub fn commit_changes(&self, task: &DevTask, result: &TaskResult) -> Result<String> {
        if !result.success {
            bail!("Cannot commit failed task");
        }

        let message = task
            .commit_message
            .clone()
            .unwrap_or_else(|| format!("feat: {}", task.description));

        // Stage changed files
        for change in &result.changes {
            git::stage_file(&self.project.root, &change.path)?;
        }

        // Commit
        let commit_hash = git::commit(&self.project.root, &message)?;
        self.log(&format!("Committed: {} ({})", message, &commit_hash[..8]));

        Ok(commit_hash)
    }

    fn log(&self, msg: &str) {
        if self.verbose {
            eprintln!("[bootstrap] {}", msg);
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// RESULT TYPES
// ═══════════════════════════════════════════════════════════════

/// Test run result
#[derive(Debug, Clone)]
pub struct TestResult {
    pub passed: bool,
    pub test_count: usize,
    pub output: String,
}

/// Lint result
#[derive(Debug, Clone)]
pub struct LintResult {
    pub passed: bool,
    pub warning_count: usize,
    pub output: String,
}

/// File change
#[derive(Debug, Clone)]
pub struct FileChange {
    pub path: String,
    pub kind: ChangeKind,
    pub lines_added: usize,
    pub lines_removed: usize,
}

#[derive(Debug, Clone)]
pub enum ChangeKind {
    Created,
    Modified,
    Deleted,
}

/// Task execution result
#[derive(Debug)]
pub struct TaskResult {
    pub success: bool,
    pub message: String,
    pub changes: Vec<FileChange>,
    pub tests_before: TestResult,
    pub tests_after: Option<TestResult>,
}

// ═══════════════════════════════════════════════════════════════
// HELPERS
// ═══════════════════════════════════════════════════════════════

fn extract_test_count(output: &str) -> usize {
    // Look for "X passed" in test output
    for line in output.lines() {
        if line.contains("passed") {
            // Parse "test result: ok. 42 passed; 0 failed"
            let parts: Vec<&str> = line.split_whitespace().collect();
            for (i, part) in parts.iter().enumerate() {
                if (*part == "passed" || part.starts_with("passed")) && i > 0 {
                    if let Ok(n) = parts[i - 1].parse::<usize>() {
                        return n;
                    }
                }
            }
        }
    }
    0
}

// ═══════════════════════════════════════════════════════════════
// SELF-ANALYSIS
// ═══════════════════════════════════════════════════════════════

/// Codebase analysis result
#[derive(Debug, Clone)]
pub struct CodebaseAnalysis {
    pub modules: Vec<ModuleInfo>,
    pub total_lines: usize,
    pub test_count: usize,
    pub dead_code_warnings: usize,
    pub todos: Vec<TodoItem>,
    pub health_score: f32, // 0.0 to 1.0
}

/// Info about a module
#[derive(Debug, Clone)]
pub struct ModuleInfo {
    pub name: String,
    pub path: PathBuf,
    pub lines: usize,
    pub functions: usize,
    pub tests: usize,
    pub doc_coverage: f32, // 0.0 to 1.0
    pub dependencies: Vec<String>,
}

/// A TODO item found in code
#[derive(Debug, Clone)]
pub struct TodoItem {
    pub file: PathBuf,
    pub line: usize,
    pub text: String,
    pub priority: TodoPriority,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TodoPriority {
    High,   // FIXME, HACK, XXX
    Medium, // TODO
    Low,    // NOTE, IDEA
}

/// Self-analyzer for hyle codebase
pub struct SelfAnalyzer {
    project: Project,
}

impl SelfAnalyzer {
    pub fn new() -> Result<Self> {
        let project = self_project().context("Could not detect hyle project")?;
        Ok(Self { project })
    }

    /// Full codebase analysis
    pub fn analyze(&self) -> Result<CodebaseAnalysis> {
        let modules = self.analyze_modules()?;
        let total_lines: usize = modules.iter().map(|m| m.lines).sum();
        let test_count = self.count_tests()?;
        let dead_code_warnings = self.count_dead_code()?;
        let todos = self.find_todos()?;

        // Calculate health score
        let test_ratio = (test_count as f32 / modules.len() as f32).min(10.0) / 10.0;
        let dead_code_penalty = (dead_code_warnings as f32 / 100.0).min(0.3);
        let todo_penalty = (todos
            .iter()
            .filter(|t| t.priority == TodoPriority::High)
            .count() as f32
            / 10.0)
            .min(0.2);

        let health_score =
            (0.5 + test_ratio * 0.3 - dead_code_penalty - todo_penalty).clamp(0.0, 1.0);

        Ok(CodebaseAnalysis {
            modules,
            total_lines,
            test_count,
            dead_code_warnings,
            todos,
            health_score,
        })
    }

    /// Analyze individual modules
    fn analyze_modules(&self) -> Result<Vec<ModuleInfo>> {
        let mut modules = Vec::new();
        let src_dir = self.project.root.join("src");

        if let Ok(entries) = std::fs::read_dir(&src_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.extension().map(|e| e == "rs").unwrap_or(false) {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        let name = path
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("unknown")
                            .to_string();

                        let lines = content.lines().count();
                        let functions = content.matches("fn ").count();
                        let tests = content.matches("#[test]").count();

                        // Count doc comments
                        let doc_lines = content
                            .lines()
                            .filter(|l| l.trim().starts_with("///") || l.trim().starts_with("//!"))
                            .count();
                        let doc_coverage = (doc_lines as f32 / lines.max(1) as f32).min(1.0);

                        // Extract dependencies (use statements)
                        let dependencies: Vec<String> = content
                            .lines()
                            .filter(|l| l.starts_with("use crate::"))
                            .filter_map(|l| {
                                l.strip_prefix("use crate::")
                                    .map(|s| s.split(':').next().unwrap_or(s))
                                    .map(|s| s.split(';').next().unwrap_or(s))
                                    .map(|s| s.to_string())
                            })
                            .collect();

                        modules.push(ModuleInfo {
                            name,
                            path,
                            lines,
                            functions,
                            tests,
                            doc_coverage,
                            dependencies,
                        });
                    }
                }
            }
        }

        modules.sort_by(|a, b| b.lines.cmp(&a.lines));
        Ok(modules)
    }

    /// Count total tests
    fn count_tests(&self) -> Result<usize> {
        let output = Command::new("cargo")
            .args(["test", "--", "--list"])
            .current_dir(&self.project.root)
            .output()
            .context("Failed to list tests")?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout.lines().filter(|l| l.ends_with(": test")).count())
    }

    /// Count dead code warnings
    fn count_dead_code(&self) -> Result<usize> {
        let output = Command::new("cargo")
            .args(["check", "--message-format=short"])
            .current_dir(&self.project.root)
            .output()
            .context("Failed to run cargo check")?;

        let stderr = String::from_utf8_lossy(&output.stderr);
        Ok(stderr.matches("never used").count() + stderr.matches("never constructed").count())
    }

    /// Find TODO/FIXME items
    fn find_todos(&self) -> Result<Vec<TodoItem>> {
        let mut todos = Vec::new();
        let src_dir = self.project.root.join("src");

        if let Ok(entries) = std::fs::read_dir(&src_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.extension().map(|e| e == "rs").unwrap_or(false) {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        for (i, line) in content.lines().enumerate() {
                            let upper = line.to_uppercase();
                            let priority = if upper.contains("FIXME")
                                || upper.contains("XXX")
                                || upper.contains("HACK")
                            {
                                Some(TodoPriority::High)
                            } else if upper.contains("TODO") {
                                Some(TodoPriority::Medium)
                            } else if upper.contains("NOTE:") || upper.contains("IDEA:") {
                                Some(TodoPriority::Low)
                            } else {
                                None
                            };

                            if let Some(p) = priority {
                                todos.push(TodoItem {
                                    file: path.clone(),
                                    line: i + 1,
                                    text: line.trim().to_string(),
                                    priority: p,
                                });
                            }
                        }
                    }
                }
            }
        }

        todos.sort_by_key(|t| std::cmp::Reverse(t.priority as u8));
        Ok(todos)
    }

    /// Get module dependency graph as mermaid
    pub fn dependency_graph(&self) -> Result<String> {
        let modules = self.analyze_modules()?;
        let mut graph = String::from("graph TD\n");

        for module in &modules {
            for dep in &module.dependencies {
                graph.push_str(&format!("    {} --> {}\n", module.name, dep));
            }
        }

        Ok(graph)
    }

    /// Generate LLM prompt for improvement suggestions
    pub fn improvement_prompt(&self) -> Result<String> {
        let analysis = self.analyze()?;

        let mut prompt = String::from("Analyze this Rust codebase and suggest improvements:\n\n");

        prompt.push_str("## Health Score\n");
        prompt.push_str(&format!(
            "{:.0}% (tests: {}, dead code warnings: {}, high-priority TODOs: {})\n\n",
            analysis.health_score * 100.0,
            analysis.test_count,
            analysis.dead_code_warnings,
            analysis
                .todos
                .iter()
                .filter(|t| t.priority == TodoPriority::High)
                .count()
        ));

        prompt.push_str("## Modules by Size\n");
        for m in analysis.modules.iter().take(10) {
            prompt.push_str(&format!(
                "- {} ({} lines, {} fns, {} tests, {:.0}% doc)\n",
                m.name,
                m.lines,
                m.functions,
                m.tests,
                m.doc_coverage * 100.0
            ));
        }

        prompt.push_str("\n## High Priority TODOs\n");
        for todo in analysis
            .todos
            .iter()
            .filter(|t| t.priority == TodoPriority::High)
            .take(5)
        {
            prompt.push_str(&format!(
                "- {}:{}: {}\n",
                todo.file.file_name().unwrap_or_default().to_string_lossy(),
                todo.line,
                todo.text
            ));
        }

        prompt.push_str("\n## Suggestions Requested\n");
        prompt.push_str("1. Which modules need more tests?\n");
        prompt.push_str("2. Which modules have too many responsibilities?\n");
        prompt.push_str("3. What dead code should be removed or wired up?\n");
        prompt.push_str("4. What documentation is missing?\n");
        prompt.push_str("5. What quick wins would improve code quality?\n");

        Ok(prompt)
    }
}

impl std::fmt::Display for CodebaseAnalysis {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Codebase Analysis")?;
        writeln!(f, "=================")?;
        writeln!(f, "Health Score: {:.0}%", self.health_score * 100.0)?;
        writeln!(f, "Modules: {}", self.modules.len())?;
        writeln!(f, "Total Lines: {}", self.total_lines)?;
        writeln!(f, "Tests: {}", self.test_count)?;
        writeln!(f, "Dead Code Warnings: {}", self.dead_code_warnings)?;
        writeln!(
            f,
            "TODOs: {} ({} high priority)",
            self.todos.len(),
            self.todos
                .iter()
                .filter(|t| t.priority == TodoPriority::High)
                .count()
        )?;
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════
// SELF-REPAIR
// ═══════════════════════════════════════════════════════════════

/// Issue detected in codebase
#[derive(Debug, Clone)]
pub struct Issue {
    pub kind: IssueKind,
    pub severity: Severity,
    pub file: Option<PathBuf>,
    pub line: Option<usize>,
    pub message: String,
    pub suggested_fix: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueKind {
    CompileError,
    TestFailure,
    DeadCode,
    MissingDoc,
    StyleViolation,
    SecurityConcern,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Critical, // Blocks execution
    High,     // Should fix soon
    Medium,   // Should fix eventually
    Low,      // Nice to have
}

/// Self-repair suggestions
pub struct SelfRepair {
    project: Project,
}

impl SelfRepair {
    pub fn new() -> Result<Self> {
        let project = self_project().context("Could not detect hyle project")?;
        Ok(Self { project })
    }

    /// Detect issues in codebase
    pub fn detect_issues(&self) -> Result<Vec<Issue>> {
        let mut issues = Vec::new();

        // Check if it compiles
        let compile = Command::new("cargo")
            .arg("check")
            .current_dir(&self.project.root)
            .output()?;

        if !compile.status.success() {
            let stderr = String::from_utf8_lossy(&compile.stderr);
            for line in stderr.lines() {
                if line.contains("error[") {
                    issues.push(Issue {
                        kind: IssueKind::CompileError,
                        severity: Severity::Critical,
                        file: None,
                        line: None,
                        message: line.to_string(),
                        suggested_fix: None,
                    });
                }
            }
        }

        // Check tests
        let tests = Command::new("cargo")
            .args(["test", "--no-run"])
            .current_dir(&self.project.root)
            .output()?;

        if !tests.status.success() {
            issues.push(Issue {
                kind: IssueKind::TestFailure,
                severity: Severity::High,
                file: None,
                line: None,
                message: "Some tests fail to compile".to_string(),
                suggested_fix: None,
            });
        }

        // Sort by severity
        issues.sort_by(|a, b| a.severity.cmp(&b.severity));
        Ok(issues)
    }

    /// Generate fix suggestions
    pub fn suggest_fixes(&self, issues: &[Issue]) -> Vec<String> {
        issues
            .iter()
            .filter_map(|i| match i.kind {
                IssueKind::CompileError => Some(format!("Fix compile error: {}", i.message)),
                IssueKind::TestFailure => Some("Review and fix failing tests".to_string()),
                IssueKind::DeadCode => Some(format!(
                    "Remove or wire up dead code: {}",
                    i.file
                        .as_ref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_default()
                )),
                _ => None,
            })
            .collect()
    }
}

// ═══════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dev_task_builder() {
        let task = DevTask::new("Add feature X")
            .with_files(vec!["src/lib.rs".to_string()])
            .with_commit_message("feat: add feature X");

        assert_eq!(task.description, "Add feature X");
        assert_eq!(task.affected_files.len(), 1);
        assert_eq!(task.commit_message, Some("feat: add feature X".to_string()));
    }

    #[test]
    fn test_extract_test_count() {
        let output = "test result: ok. 42 passed; 0 failed; 0 ignored";
        assert_eq!(extract_test_count(output), 42);

        let output2 = "test result: ok. 118 passed; 0 failed; 0 ignored; 0 measured";
        assert_eq!(extract_test_count(output2), 118);
    }

    #[test]
    fn test_bootstrap_creation() {
        // This test will pass if we're in the hyle project
        match Bootstrap::new() {
            Ok(bs) => {
                assert!(bs.project().name == "hyle" || bs.project().name == "claude-replacement");
                println!("Bootstrap created for: {}", bs.project().name);
            }
            Err(e) => {
                println!(
                    "Bootstrap not available: {} (expected outside hyle project)",
                    e
                );
            }
        }
    }

    #[test]
    fn test_run_tests() {
        if let Ok(bs) = Bootstrap::new() {
            let result = bs.run_tests().expect("Failed to run tests");
            println!(
                "Tests passed: {}, count: {}",
                result.passed, result.test_count
            );
            // Don't assert on pass/fail as we might be testing mid-development
            assert!(result.test_count > 0);
        }
    }

    #[test]
    fn test_check_build() {
        if let Ok(bs) = Bootstrap::new() {
            let builds = bs.check_build().expect("Failed to check build");
            println!("Build check: {}", if builds { "OK" } else { "FAIL" });
        }
    }
}
