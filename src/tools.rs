//! File and patch tools
//!
//! - Read files with context
//! - Generate unified diffs
//! - Apply patches

use anyhow::{Context, Result};
use similar::TextDiff;
use std::fs;
use std::path::Path;

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
}
