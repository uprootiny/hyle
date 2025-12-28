//! Docs Watcher - Side conversation for documentation maintenance
//!
//! Uses a free LLM to watch for code changes and suggest doc updates.
//! Designed to run as a background process alongside the main session.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::collections::HashMap;
use chrono::{DateTime, Utc};

/// A documentation file being watched
#[derive(Debug, Clone)]
pub struct DocFile {
    pub path: PathBuf,
    pub content: String,
    pub last_modified: DateTime<Utc>,
    pub sections: Vec<DocSection>,
}

/// A section within a doc file
#[derive(Debug, Clone)]
pub struct DocSection {
    pub heading: String,
    pub level: u8,  // 1=h1, 2=h2, etc.
    pub content: String,
    pub line_start: usize,
    pub line_end: usize,
}

/// Change detected in the codebase
#[derive(Debug, Clone)]
pub struct CodeChange {
    pub file: PathBuf,
    pub change_type: ChangeType,
    pub summary: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub enum ChangeType {
    Added,
    Modified,
    Deleted,
    Renamed(PathBuf),
}

/// Suggested documentation update
#[derive(Debug, Clone)]
pub struct DocSuggestion {
    pub doc_file: PathBuf,
    pub section: Option<String>,
    pub suggestion: String,
    pub reason: String,
    pub priority: Priority,
    pub code_changes: Vec<CodeChange>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    High,    // Breaking change, needs immediate doc update
    Medium,  // New feature, should document soon
    Low,     // Minor change, optional doc update
}

/// Documentation watcher state
pub struct DocsWatcher {
    /// Root directory being watched
    pub root: PathBuf,

    /// Tracked documentation files
    pub docs: HashMap<PathBuf, DocFile>,

    /// Recent code changes
    pub changes: Vec<CodeChange>,

    /// Pending suggestions
    pub suggestions: Vec<DocSuggestion>,

    /// Files to watch for changes
    pub watch_patterns: Vec<String>,

    /// Files to ignore
    pub ignore_patterns: Vec<String>,
}

impl DocsWatcher {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
            docs: HashMap::new(),
            changes: Vec::new(),
            suggestions: Vec::new(),
            watch_patterns: vec![
                "README.md".into(),
                "docs/**/*.md".into(),
                "CHANGELOG.md".into(),
                "*.md".into(),
            ],
            ignore_patterns: vec![
                "node_modules/**".into(),
                "target/**".into(),
                ".git/**".into(),
            ],
        }
    }

    /// Scan for documentation files
    pub fn scan_docs(&mut self) -> Vec<PathBuf> {
        let mut found = Vec::new();

        // Check common doc files
        let common = ["README.md", "CHANGELOG.md", "docs"];
        for name in common {
            let path = self.root.join(name);
            if path.exists() {
                if path.is_file() {
                    found.push(path);
                } else if path.is_dir() {
                    // Recursively find .md files
                    if let Ok(entries) = glob::glob(&format!("{}/**/*.md", path.display())) {
                        for entry in entries.filter_map(|e| e.ok()) {
                            found.push(entry);
                        }
                    }
                }
            }
        }

        found
    }

    /// Parse a markdown file into sections
    pub fn parse_doc(&self, path: &Path) -> Option<DocFile> {
        let content = std::fs::read_to_string(path).ok()?;
        let mut sections = Vec::new();
        let mut current_section: Option<(String, u8, usize)> = None;
        let mut section_content = String::new();

        for (i, line) in content.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with('#') {
                // Save previous section
                if let Some((heading, level, start)) = current_section.take() {
                    sections.push(DocSection {
                        heading,
                        level,
                        content: section_content.trim().to_string(),
                        line_start: start,
                        line_end: i.saturating_sub(1),
                    });
                    section_content.clear();
                }

                // Parse new heading
                let level = trimmed.chars().take_while(|&c| c == '#').count() as u8;
                let heading = trimmed.trim_start_matches('#').trim().to_string();
                current_section = Some((heading, level, i));
            } else if current_section.is_some() {
                section_content.push_str(line);
                section_content.push('\n');
            }
        }

        // Save last section
        if let Some((heading, level, start)) = current_section {
            sections.push(DocSection {
                heading,
                level,
                content: section_content.trim().to_string(),
                line_start: start,
                line_end: content.lines().count(),
            });
        }

        Some(DocFile {
            path: path.to_path_buf(),
            content,
            last_modified: Utc::now(),
            sections,
        })
    }

    /// Check for code changes since last check
    pub fn check_changes(&mut self) -> Vec<CodeChange> {
        let mut changes = Vec::new();

        // Use git to detect changes
        if let Ok(output) = std::process::Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(&self.root)
            .output()
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if line.len() < 3 {
                    continue;
                }

                let status = &line[..2];
                let file = line[3..].trim();
                let path = self.root.join(file);

                let change_type = match status.trim() {
                    "A" | "??" => ChangeType::Added,
                    "M" | " M" | "MM" => ChangeType::Modified,
                    "D" | " D" => ChangeType::Deleted,
                    s if s.starts_with('R') => {
                        // Renamed - need to parse old path
                        ChangeType::Renamed(path.clone())
                    }
                    _ => continue,
                };

                changes.push(CodeChange {
                    file: path,
                    change_type,
                    summary: String::new(),  // Will be filled by LLM
                    timestamp: Utc::now(),
                });
            }
        }

        self.changes.extend(changes.clone());
        changes
    }

    /// Generate prompt for LLM to analyze changes and suggest doc updates
    pub fn analysis_prompt(&self) -> String {
        let mut prompt = String::from(
            "Analyze these code changes and suggest documentation updates.\n\n"
        );

        prompt.push_str("## Recent Changes\n\n");
        for change in &self.changes {
            let change_type = match &change.change_type {
                ChangeType::Added => "Added",
                ChangeType::Modified => "Modified",
                ChangeType::Deleted => "Deleted",
                ChangeType::Renamed(_) => "Renamed",
            };
            prompt.push_str(&format!("- {} {}\n", change_type, change.file.display()));
        }

        prompt.push_str("\n## Current Documentation\n\n");
        for (path, doc) in &self.docs {
            prompt.push_str(&format!("### {}\n", path.display()));
            prompt.push_str("Sections:\n");
            for section in &doc.sections {
                prompt.push_str(&format!("- {} ({})\n", section.heading, section.content.len()));
            }
        }

        prompt.push_str("\n## Task\n\n");
        prompt.push_str("For each significant code change, suggest specific documentation updates.\n");
        prompt.push_str("Focus on:\n");
        prompt.push_str("- New features that need documentation\n");
        prompt.push_str("- Changed APIs or behaviors\n");
        prompt.push_str("- Removed features that should be noted\n");
        prompt.push_str("- README sections that might be outdated\n\n");
        prompt.push_str("Format: For each suggestion, specify:\n");
        prompt.push_str("1. Which doc file to update\n");
        prompt.push_str("2. Which section\n");
        prompt.push_str("3. What to add/change\n");
        prompt.push_str("4. Priority (high/medium/low)\n");

        prompt
    }

    /// Get suggestions as a formatted string
    pub fn format_suggestions(&self) -> String {
        if self.suggestions.is_empty() {
            return "No documentation updates suggested.".into();
        }

        let mut out = String::from("Documentation Update Suggestions:\n\n");
        for (i, s) in self.suggestions.iter().enumerate() {
            let priority = match s.priority {
                Priority::High => "[HIGH]",
                Priority::Medium => "[MEDIUM]",
                Priority::Low => "[LOW]",
            };
            out.push_str(&format!("{}. {} {}\n", i + 1, priority, s.doc_file.display()));
            if let Some(ref section) = s.section {
                out.push_str(&format!("   Section: {}\n", section));
            }
            out.push_str(&format!("   Suggestion: {}\n", s.suggestion));
            out.push_str(&format!("   Reason: {}\n\n", s.reason));
        }
        out
    }
}

/// Prompt for a free LLM to update a specific doc section
pub fn doc_update_prompt(doc: &DocFile, section: &DocSection, changes: &[CodeChange]) -> String {
    let mut prompt = format!(
        "Update this documentation section based on recent code changes.\n\n\
         File: {}\n\
         Section: {} (level {})\n\n\
         Current content:\n{}\n\n\
         Recent changes:\n",
        doc.path.display(),
        section.heading,
        section.level,
        section.content
    );

    for change in changes {
        let change_type = match &change.change_type {
            ChangeType::Added => "Added",
            ChangeType::Modified => "Modified",
            ChangeType::Deleted => "Deleted",
            ChangeType::Renamed(_) => "Renamed",
        };
        prompt.push_str(&format!("- {} {}\n", change_type, change.file.display()));
        if !change.summary.is_empty() {
            prompt.push_str(&format!("  Summary: {}\n", change.summary));
        }
    }

    prompt.push_str("\nProvide the updated section content in markdown format.\n");
    prompt.push_str("Keep the same heading level and style.\n");
    prompt.push_str("Only update what's necessary based on the changes.\n");

    prompt
}

/// Prompt to generate changelog entry
pub fn changelog_prompt(changes: &[CodeChange], version: Option<&str>) -> String {
    let mut prompt = String::from("Generate a changelog entry for these changes.\n\n");

    if let Some(v) = version {
        prompt.push_str(&format!("Version: {}\n\n", v));
    }

    prompt.push_str("Changes:\n");
    for change in changes {
        let change_type = match &change.change_type {
            ChangeType::Added => "Added",
            ChangeType::Modified => "Modified",
            ChangeType::Deleted => "Deleted",
            ChangeType::Renamed(_) => "Renamed",
        };
        prompt.push_str(&format!("- {} {}\n", change_type, change.file.display()));
        if !change.summary.is_empty() {
            prompt.push_str(&format!("  {}\n", change.summary));
        }
    }

    prompt.push_str("\nFormat the changelog entry using Keep a Changelog format:\n");
    prompt.push_str("- Group by: Added, Changed, Deprecated, Removed, Fixed, Security\n");
    prompt.push_str("- Be concise but informative\n");
    prompt.push_str("- Focus on user-facing changes\n");

    prompt
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_doc() {
        let watcher = DocsWatcher::new(".");

        // Create temp file
        let content = "# Title\n\nSome content.\n\n## Section\n\nMore content.";
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("test_doc.md");
        std::fs::write(&temp_file, content).unwrap();

        let doc = watcher.parse_doc(&temp_file).unwrap();
        assert_eq!(doc.sections.len(), 2);
        assert_eq!(doc.sections[0].heading, "Title");
        assert_eq!(doc.sections[1].heading, "Section");

        std::fs::remove_file(temp_file).ok();
    }
}
