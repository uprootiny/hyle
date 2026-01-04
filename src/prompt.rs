//! System prompt generation for LLM interactions
//!
//! Announces hyle's presence and capabilities to the LLM.

#![allow(dead_code)] // Forward-looking module for LLM integration

use crate::intent::IntentStack;
use crate::project::Project;

// ═══════════════════════════════════════════════════════════════
// SYSTEM PROMPT BUILDER
// ═══════════════════════════════════════════════════════════════

/// Builds context-aware system prompts
pub struct SystemPrompt {
    project: Option<Project>,
    intents: Option<IntentStack>,
    tools_enabled: Vec<String>,
    custom_instructions: Vec<String>,
}

impl SystemPrompt {
    pub fn new() -> Self {
        Self {
            project: None,
            intents: None,
            tools_enabled: default_tools(),
            custom_instructions: Vec::new(),
        }
    }

    pub fn with_project(mut self, project: Project) -> Self {
        self.project = Some(project);
        self
    }

    pub fn with_intents(mut self, intents: IntentStack) -> Self {
        self.intents = Some(intents);
        self
    }

    pub fn with_tools(mut self, tools: Vec<String>) -> Self {
        self.tools_enabled = tools;
        self
    }

    pub fn add_instruction(mut self, instruction: &str) -> Self {
        self.custom_instructions.push(instruction.to_string());
        self
    }

    /// Generate the full system prompt
    pub fn build(&self) -> String {
        let mut prompt = String::new();

        // Identity
        prompt.push_str(&self.identity_section());

        // Capabilities
        prompt.push_str(&self.capabilities_section());

        // Project context
        if let Some(ref project) = self.project {
            prompt.push_str(&self.project_section(project));
        }

        // Current intent
        if let Some(ref intents) = self.intents {
            prompt.push_str(&self.intent_section(intents));
        }

        // Guidelines
        prompt.push_str(&self.guidelines_section());

        // Custom instructions
        for instruction in &self.custom_instructions {
            prompt.push_str(&format!("\n{}\n", instruction));
        }

        prompt
    }

    fn identity_section(&self) -> String {
        r#"<identity>
You are hyle, a Rust-native code assistant. You help developers with:
- Code understanding and navigation
- Implementing features and fixes
- Running tests and builds
- Git operations and commits
- Debugging and problem solving

You communicate concisely and act decisively. When asked to do something, do it rather than explaining how to do it.
</identity>

"#.to_string()
    }

    fn capabilities_section(&self) -> String {
        let mut section = String::from("<capabilities>\n");

        section.push_str("Available tools:\n");
        for tool in &self.tools_enabled {
            let desc = tool_description(tool);
            section.push_str(&format!("- {}: {}\n", tool, desc));
        }

        section.push_str("\nTool call format:\n");
        section.push_str("```json\n{\"tool\": \"<name>\", \"args\": {<arguments>}}\n```\n");

        section.push_str("</capabilities>\n\n");
        section
    }

    fn project_section(&self, project: &Project) -> String {
        let mut section = String::from("<project>\n");

        section.push_str(&format!("Name: {}\n", project.name));
        section.push_str(&format!("Type: {:?}\n", project.project_type));
        section.push_str(&format!(
            "Files: {} ({} lines)\n",
            project.files.len(),
            project.total_lines()
        ));

        // Include structure summary
        section.push_str("\nStructure:\n");
        for file in project.files.iter().take(20) {
            section.push_str(&format!("  {} ({} lines)\n", file.relative, file.lines));
        }
        if project.files.len() > 20 {
            section.push_str(&format!(
                "  ... and {} more files\n",
                project.files.len() - 20
            ));
        }

        section.push_str("</project>\n\n");
        section
    }

    fn intent_section(&self, intents: &IntentStack) -> String {
        let mut section = String::from("<intent>\n");

        if let Some(primary) = intents.primary() {
            section.push_str(&format!("Primary goal: {}\n", primary.description));
        }

        section.push_str(&format!("Current focus: {}\n", intents.status_line()));

        let depth = intents.aside_depth();
        if depth > 0 {
            section.push_str(&format!(
                "Note: Currently {} level(s) into an aside. Stay focused or return to main task.\n",
                depth
            ));
        }

        section.push_str("</intent>\n\n");
        section
    }

    fn guidelines_section(&self) -> String {
        r#"<guidelines>
Response format:
- Be concise. Skip preamble.
- Use code blocks with language tags.
- For file changes, show diffs or full content.
- For multi-step tasks, announce what you're doing.

Safety:
- Never expose secrets, API keys, or credentials
- Confirm before destructive operations
- Keep changes minimal and focused

Quality:
- Write idiomatic, well-structured code
- Add tests for new functionality
- Follow existing project conventions
</guidelines>
"#
        .to_string()
    }
}

impl Default for SystemPrompt {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════════
// TOOL DEFINITIONS
// ═══════════════════════════════════════════════════════════════

fn default_tools() -> Vec<String> {
    vec![
        "read".into(),
        "write".into(),
        "edit".into(),
        "bash".into(),
        "glob".into(),
        "grep".into(),
    ]
}

fn tool_description(name: &str) -> &'static str {
    match name {
        "read" => "Read file contents. Args: {path: string}",
        "write" => "Write file contents. Args: {path: string, content: string}",
        "edit" => "Edit file with search/replace. Args: {path: string, old: string, new: string}",
        "bash" => "Execute shell command. Args: {command: string}",
        "glob" => "Find files matching pattern. Args: {pattern: string}",
        "grep" => "Search file contents. Args: {pattern: string, path?: string}",
        "git_status" => "Get git status",
        "git_diff" => "Get git diff. Args: {staged?: bool}",
        "git_commit" => "Create commit. Args: {message: string}",
        _ => "Unknown tool",
    }
}

// ═══════════════════════════════════════════════════════════════
// QUICK BUILDERS
// ═══════════════════════════════════════════════════════════════

/// Build a minimal system prompt
pub fn minimal_prompt() -> String {
    SystemPrompt::new().build()
}

/// Build a project-aware system prompt
pub fn project_prompt(project: &Project) -> String {
    SystemPrompt::new().with_project(project.clone()).build()
}

/// Build a full context-aware prompt
pub fn full_prompt(project: Option<&Project>, intents: Option<&IntentStack>) -> String {
    let mut builder = SystemPrompt::new();

    if let Some(p) = project {
        builder = builder.with_project(p.clone());
    }

    if let Some(i) = intents {
        builder = builder.with_intents(i.clone());
    }

    builder.build()
}

// ═══════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_minimal_prompt() {
        let prompt = minimal_prompt();

        assert!(prompt.contains("<identity>"));
        assert!(prompt.contains("hyle"));
        assert!(prompt.contains("<capabilities>"));
        assert!(prompt.contains("read"));
        assert!(prompt.contains("<guidelines>"));
    }

    #[test]
    fn test_system_prompt_builder() {
        let prompt = SystemPrompt::new()
            .add_instruction("Always use tabs for indentation")
            .build();

        assert!(prompt.contains("tabs for indentation"));
    }

    #[test]
    fn test_with_intents() {
        let mut intents = IntentStack::new();
        intents.set_primary("Build feature X");
        intents.push_subtask("Add tests");

        let prompt = SystemPrompt::new().with_intents(intents).build();

        assert!(prompt.contains("<intent>"));
        assert!(prompt.contains("Build feature X"));
        assert!(prompt.contains("Add tests"));
    }

    #[test]
    fn test_tool_descriptions() {
        assert!(tool_description("read").contains("Read file"));
        assert!(tool_description("bash").contains("Execute"));
        assert_eq!(tool_description("unknown"), "Unknown tool");
    }

    #[test]
    fn test_full_prompt() {
        let prompt = full_prompt(None, None);
        assert!(prompt.contains("<identity>"));
        assert!(prompt.contains("<capabilities>"));
    }
}
