//! Prompt library and command mapping
//!
//! Features:
//! - Auto-save prompts that appear multiple times
//! - Prune trivial prompts (short, common acknowledgments)
//! - Map general commands to specific explicit prompts
//! - Track prompt context (files, topics present when used)
//!
//! NOTE: Module pending full TUI integration. Dead code warnings expected.

#![allow(dead_code)] // Module under construction, TUI wiring pending

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Minimum prompt length to consider saving (characters)
const MIN_PROMPT_LENGTH: usize = 10;

/// Number of uses before auto-saving to library
const AUTO_SAVE_THRESHOLD: u32 = 2;

/// Trivial prompts to ignore (lowercase)
const TRIVIAL_PROMPTS: &[&str] = &[
    "ok", "okay", "k", "yes", "no", "y", "n",
    "proceed", "continue", "go", "do it", "yes please",
    "thanks", "thank you", "thx", "ty",
    "uh huh", "uhuh", "mhm", "hmm", "hm",
    "next", "more", "again", "retry",
    "1", "2", "3", "4", "5", "6", "7", "8", "9", "0",
    "commit", "push", "save", // These are slash commands
];

/// A saved prompt with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedPrompt {
    pub text: String,
    pub count: u32,
    pub last_used: chrono::DateTime<chrono::Utc>,
    pub contexts: Vec<PromptContext>,
    pub category: PromptCategory,
    /// Mapped general command if this is an expansion
    pub general_form: Option<String>,
}

/// Context when a prompt was used
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptContext {
    pub files: Vec<String>,      // Files in focus
    pub keywords: Vec<String>,   // Extracted keywords
    pub project_type: Option<String>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Category of prompt
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PromptCategory {
    /// General command that can be expanded
    GeneralCommand,
    /// Specific explicit prompt
    Specific,
    /// Trivial (will be pruned)
    Trivial,
}

/// Mapping from general command to specific prompt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandMapping {
    pub general: String,
    pub specific: String,
    pub description: String,
}

/// The prompt library
#[derive(Debug, Serialize, Deserialize)]
pub struct PromptLibrary {
    /// Saved prompts by normalized text
    prompts: HashMap<String, SavedPrompt>,
    /// General command -> specific prompt mappings
    mappings: Vec<CommandMapping>,
    /// Recent prompts for repeat detection
    #[serde(skip)]
    recent: Vec<String>,
}

impl Default for PromptLibrary {
    fn default() -> Self {
        Self::new()
    }
}

impl PromptLibrary {
    pub fn new() -> Self {
        let mut lib = Self {
            prompts: HashMap::new(),
            mappings: Vec::new(),
            recent: Vec::new(),
        };

        // Add default command mappings
        lib.add_default_mappings();
        lib
    }

    /// Add default general -> specific command mappings
    fn add_default_mappings(&mut self) {
        let defaults = vec![
            ("test it", "Run the test suite, fix any failures, and report the results.", "Run tests"),
            ("build it", "Build the project in release mode and fix any compilation errors.", "Build project"),
            ("review", "Review the code for correctness, style, and potential issues. Provide an engineering assessment.", "Code review"),
            ("optimize", "Analyze performance bottlenecks and optimize the critical paths.", "Optimize code"),
            ("tie it up", "Finish the current task, ensure all loose ends are addressed, and prepare for commit.", "Finalize task"),
            ("clean up", "Remove dead code, fix warnings, improve organization.", "Code cleanup"),
            ("document", "Add or update documentation for the recent changes.", "Add docs"),
            ("refactor", "Refactor the code for better readability and maintainability.", "Refactor"),
            ("explain", "Explain what this code does and how it works.", "Explain code"),
            ("debug", "Find and fix the bug. Add a test to prevent regression.", "Debug issue"),
        ];

        for (general, specific, desc) in defaults {
            self.mappings.push(CommandMapping {
                general: general.to_string(),
                specific: specific.to_string(),
                description: desc.to_string(),
            });
        }
    }

    /// Record a prompt usage
    pub fn record(&mut self, text: &str, context: Option<PromptContext>) {
        let normalized = text.trim().to_lowercase();

        // Check if trivial
        if self.is_trivial(&normalized) {
            return;
        }

        // Add to recent for repeat detection
        self.recent.push(normalized.clone());
        if self.recent.len() > 100 {
            self.recent.remove(0);
        }

        // Update or create prompt entry
        if let Some(prompt) = self.prompts.get_mut(&normalized) {
            prompt.count += 1;
            prompt.last_used = chrono::Utc::now();
            if let Some(ctx) = context {
                // Keep last 5 contexts
                prompt.contexts.push(ctx);
                if prompt.contexts.len() > 5 {
                    prompt.contexts.remove(0);
                }
            }
        } else {
            // Check if appears in recent (repeat detection)
            let appearances = self.recent.iter().filter(|r| *r == &normalized).count();
            if appearances >= AUTO_SAVE_THRESHOLD as usize || text.len() > 50 {
                self.prompts.insert(normalized.clone(), SavedPrompt {
                    text: text.trim().to_string(),
                    count: appearances as u32,
                    last_used: chrono::Utc::now(),
                    contexts: context.map(|c| vec![c]).unwrap_or_default(),
                    category: self.categorize(text),
                    general_form: self.find_general_form(text),
                });
            }
        }
    }

    /// Check if a prompt is trivial
    fn is_trivial(&self, normalized: &str) -> bool {
        if normalized.len() < MIN_PROMPT_LENGTH {
            return true;
        }

        TRIVIAL_PROMPTS.contains(&normalized)
    }

    /// Categorize a prompt
    fn categorize(&self, text: &str) -> PromptCategory {
        let lower = text.to_lowercase();

        // Check if it matches a general command
        for mapping in &self.mappings {
            if lower.contains(&mapping.general) {
                return PromptCategory::GeneralCommand;
            }
        }

        PromptCategory::Specific
    }

    /// Find if this prompt is a general form
    fn find_general_form(&self, text: &str) -> Option<String> {
        let lower = text.to_lowercase();

        for mapping in &self.mappings {
            if lower.contains(&mapping.general) {
                return Some(mapping.general.clone());
            }
        }

        None
    }

    /// Expand a general command to specific prompt
    pub fn expand(&self, text: &str) -> Option<String> {
        let lower = text.to_lowercase().trim().to_string();

        for mapping in &self.mappings {
            if lower == mapping.general || lower.contains(&mapping.general) {
                return Some(mapping.specific.clone());
            }
        }

        None
    }

    /// Get saved prompts sorted by usage count
    pub fn top_prompts(&self, limit: usize) -> Vec<&SavedPrompt> {
        let mut prompts: Vec<_> = self.prompts.values().collect();
        prompts.sort_by(|a, b| b.count.cmp(&a.count));
        prompts.into_iter().take(limit).collect()
    }

    /// Get command mappings
    pub fn mappings(&self) -> &[CommandMapping] {
        &self.mappings
    }

    /// Add a custom mapping
    pub fn add_mapping(&mut self, general: &str, specific: &str, description: &str) {
        // Remove existing mapping for same general command
        self.mappings.retain(|m| m.general != general);

        self.mappings.push(CommandMapping {
            general: general.to_string(),
            specific: specific.to_string(),
            description: description.to_string(),
        });
    }

    /// Prune old/unused prompts
    pub fn prune(&mut self, min_count: u32, max_age_days: i64) {
        let cutoff = chrono::Utc::now() - chrono::Duration::days(max_age_days);

        self.prompts.retain(|_, p| {
            p.count >= min_count || p.last_used > cutoff
        });
    }

    /// Get prompts matching context
    pub fn suggest(&self, context: &PromptContext, limit: usize) -> Vec<&SavedPrompt> {
        let mut scored: Vec<_> = self.prompts.values()
            .map(|p| {
                let score = self.context_score(p, context);
                (p, score)
            })
            .filter(|(_, score)| *score > 0.0)
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.into_iter().take(limit).map(|(p, _)| p).collect()
    }

    /// Calculate context similarity score
    fn context_score(&self, prompt: &SavedPrompt, context: &PromptContext) -> f64 {
        let mut score = 0.0;

        for ctx in &prompt.contexts {
            // File overlap
            let file_overlap = ctx.files.iter()
                .filter(|f| context.files.contains(f))
                .count();
            score += file_overlap as f64 * 2.0;

            // Keyword overlap
            let kw_overlap = ctx.keywords.iter()
                .filter(|k| context.keywords.contains(k))
                .count();
            score += kw_overlap as f64;

            // Same project type
            if ctx.project_type == context.project_type {
                score += 1.0;
            }
        }

        // Boost by usage count
        score * (1.0 + (prompt.count as f64).ln())
    }

    /// Load from file
    pub fn load() -> anyhow::Result<Self> {
        let path = Self::library_path()?;
        if !path.exists() {
            return Ok(Self::new());
        }

        let content = std::fs::read_to_string(&path)?;
        let mut lib: Self = serde_json::from_str(&content)?;
        lib.add_default_mappings(); // Ensure defaults exist
        Ok(lib)
    }

    /// Save to file
    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::library_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, content)?;
        Ok(())
    }

    fn library_path() -> anyhow::Result<PathBuf> {
        let cache = crate::config::cache_dir()?;
        Ok(cache.join("prompt_library.json"))
    }
}

/// Toolbelt: A collection of named commands for project development
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Toolbelt {
    pub name: String,
    pub commands: Vec<ToolbeltCommand>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolbeltCommand {
    pub name: String,
    pub prompt: String,
    pub description: String,
    pub phase: DevelopmentPhase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DevelopmentPhase {
    Init,       // Project setup
    Implement,  // Core implementation
    Test,       // Testing
    Review,     // Code review
    Polish,     // Cleanup, optimization
    Document,   // Documentation
    Ship,       // Final checks, deployment
}

impl Default for Toolbelt {
    fn default() -> Self {
        Self {
            name: "Standard Development".to_string(),
            commands: vec![
                ToolbeltCommand {
                    name: "scaffold".to_string(),
                    prompt: "Set up the project structure, dependencies, and basic configuration.".to_string(),
                    description: "Initial project setup".to_string(),
                    phase: DevelopmentPhase::Init,
                },
                ToolbeltCommand {
                    name: "implement".to_string(),
                    prompt: "Implement the core functionality as specified.".to_string(),
                    description: "Build the feature".to_string(),
                    phase: DevelopmentPhase::Implement,
                },
                ToolbeltCommand {
                    name: "test".to_string(),
                    prompt: "Write comprehensive tests and ensure they pass.".to_string(),
                    description: "Add test coverage".to_string(),
                    phase: DevelopmentPhase::Test,
                },
                ToolbeltCommand {
                    name: "review".to_string(),
                    prompt: "Review code for correctness, style, security, and performance.".to_string(),
                    description: "Code review".to_string(),
                    phase: DevelopmentPhase::Review,
                },
                ToolbeltCommand {
                    name: "polish".to_string(),
                    prompt: "Fix warnings, optimize hot paths, improve error handling.".to_string(),
                    description: "Polish and optimize".to_string(),
                    phase: DevelopmentPhase::Polish,
                },
                ToolbeltCommand {
                    name: "document".to_string(),
                    prompt: "Update documentation to reflect the changes.".to_string(),
                    description: "Add documentation".to_string(),
                    phase: DevelopmentPhase::Document,
                },
                ToolbeltCommand {
                    name: "ship".to_string(),
                    prompt: "Final checks, create commit, prepare for deployment.".to_string(),
                    description: "Ship it".to_string(),
                    phase: DevelopmentPhase::Ship,
                },
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trivial_detection() {
        let lib = PromptLibrary::new();
        assert!(lib.is_trivial("ok"));
        assert!(lib.is_trivial("proceed"));
        assert!(lib.is_trivial("1"));
        assert!(!lib.is_trivial("please implement the feature"));
    }

    #[test]
    fn test_expand_command() {
        let lib = PromptLibrary::new();

        let expanded = lib.expand("test it");
        assert!(expanded.is_some());
        assert!(expanded.unwrap().contains("test suite"));

        let expanded = lib.expand("review");
        assert!(expanded.is_some());
        assert!(expanded.unwrap().contains("engineering assessment"));
    }

    #[test]
    fn test_record_and_retrieve() {
        let mut lib = PromptLibrary::new();

        // Record same prompt twice
        lib.record("please review the authentication module", None);
        lib.record("please review the authentication module", None);

        let top = lib.top_prompts(10);
        assert!(!top.is_empty());
        assert!(top[0].text.contains("authentication"));
    }

    #[test]
    fn test_toolbelt() {
        let belt = Toolbelt::default();
        assert_eq!(belt.commands.len(), 7);
        assert_eq!(belt.commands[0].phase, DevelopmentPhase::Init);
        assert_eq!(belt.commands[6].phase, DevelopmentPhase::Ship);
    }
}
