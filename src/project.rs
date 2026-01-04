//! Project context awareness for self-bootstrapping
//!
//! Provides:
//! - Project type detection (Rust, Git, etc)
//! - Codebase structure indexing
//! - File change tracking
//! - Context generation for LLM

#![allow(dead_code)] // Forward-looking module for self-bootstrapping

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

// ═══════════════════════════════════════════════════════════════
// PROJECT DETECTION
// ═══════════════════════════════════════════════════════════════

/// Detected project type
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ProjectType {
    Rust,
    Node,
    Python,
    Go,
    Unknown,
}

/// Project metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub root: PathBuf,
    pub project_type: ProjectType,
    pub name: String,
    pub git_root: Option<PathBuf>,
    pub files: Vec<SourceFile>,
    pub structure: String,
}

/// Source file info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceFile {
    pub path: PathBuf,
    pub relative: String,
    pub lines: usize,
    pub language: String,
}

impl Project {
    /// Detect and index project from a directory
    pub fn detect(dir: &Path) -> Option<Self> {
        let root = find_project_root(dir)?;
        let project_type = detect_project_type(&root);
        let name = root.file_name()?.to_string_lossy().to_string();
        let git_root = find_git_root(&root);

        let mut project = Self {
            root: root.clone(),
            project_type,
            name,
            git_root,
            files: Vec::new(),
            structure: String::new(),
        };

        project.index_files();
        project.build_structure();

        Some(project)
    }

    /// Index source files in the project
    fn index_files(&mut self) {
        let extensions = match self.project_type {
            ProjectType::Rust => vec!["rs"],
            ProjectType::Node => vec!["js", "ts", "jsx", "tsx"],
            ProjectType::Python => vec!["py"],
            ProjectType::Go => vec!["go"],
            ProjectType::Unknown => vec!["rs", "py", "js", "ts", "go"],
        };

        self.files = collect_source_files(&self.root, &extensions);
    }

    /// Build structure summary
    fn build_structure(&mut self) {
        let mut structure = String::new();
        structure.push_str(&format!(
            "# {} ({})\n\n",
            self.name,
            format_project_type(&self.project_type)
        ));

        // Group files by directory
        let mut by_dir: HashMap<String, Vec<&SourceFile>> = HashMap::new();
        for file in &self.files {
            let dir = Path::new(&file.relative)
                .parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| ".".to_string());
            by_dir.entry(dir).or_default().push(file);
        }

        // Sort and format
        let mut dirs: Vec<_> = by_dir.keys().collect();
        dirs.sort();

        for dir in dirs {
            let files = &by_dir[dir];
            structure.push_str(&format!("## {}/\n", dir));
            for file in files {
                structure.push_str(&format!(
                    "- {} ({} lines)\n",
                    Path::new(&file.relative)
                        .file_name()
                        .unwrap()
                        .to_string_lossy(),
                    file.lines
                ));
            }
            structure.push('\n');
        }

        // Summary stats
        let total_lines: usize = self.files.iter().map(|f| f.lines).sum();
        structure.push_str(&format!(
            "**Total:** {} files, {} lines\n",
            self.files.len(),
            total_lines
        ));

        self.structure = structure;
    }

    /// Get context string for LLM
    pub fn context_for_llm(&self) -> String {
        let mut ctx = String::new();

        ctx.push_str(&format!(
            "<project name=\"{}\" type=\"{}\">\n",
            self.name,
            format_project_type(&self.project_type)
        ));

        ctx.push_str("<structure>\n");
        ctx.push_str(&self.structure);
        ctx.push_str("</structure>\n");

        // Include key files content (Cargo.toml, README, etc)
        if let Some(manifest) = self.read_manifest() {
            ctx.push_str("<manifest>\n");
            ctx.push_str(&manifest);
            ctx.push_str("</manifest>\n");
        }

        ctx.push_str("</project>\n");
        ctx
    }

    /// Read project manifest (Cargo.toml, package.json, etc)
    fn read_manifest(&self) -> Option<String> {
        let manifest_path = match self.project_type {
            ProjectType::Rust => self.root.join("Cargo.toml"),
            ProjectType::Node => self.root.join("package.json"),
            ProjectType::Python => self.root.join("pyproject.toml"),
            ProjectType::Go => self.root.join("go.mod"),
            ProjectType::Unknown => return None,
        };

        fs::read_to_string(manifest_path).ok()
    }

    /// Get source file by relative path
    pub fn get_file(&self, relative: &str) -> Option<&SourceFile> {
        self.files.iter().find(|f| f.relative == relative)
    }

    /// Read file content
    pub fn read_file(&self, relative: &str) -> Option<String> {
        let path = self.root.join(relative);
        fs::read_to_string(path).ok()
    }

    /// Get files matching pattern
    pub fn files_matching(&self, pattern: &str) -> Vec<&SourceFile> {
        self.files
            .iter()
            .filter(|f| f.relative.contains(pattern))
            .collect()
    }

    /// Total lines of code
    pub fn total_lines(&self) -> usize {
        self.files.iter().map(|f| f.lines).sum()
    }
}

// ═══════════════════════════════════════════════════════════════
// HELPER FUNCTIONS
// ═══════════════════════════════════════════════════════════════

/// Find project root by walking up looking for markers
fn find_project_root(start: &Path) -> Option<PathBuf> {
    let markers = [
        "Cargo.toml",
        "package.json",
        "pyproject.toml",
        "go.mod",
        ".git",
    ];

    let mut current = start.to_path_buf();
    loop {
        for marker in &markers {
            if current.join(marker).exists() {
                return Some(current);
            }
        }

        if !current.pop() {
            return None;
        }
    }
}

/// Find git root
fn find_git_root(start: &Path) -> Option<PathBuf> {
    let mut current = start.to_path_buf();
    loop {
        if current.join(".git").exists() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

/// Detect project type from root directory
fn detect_project_type(root: &Path) -> ProjectType {
    if root.join("Cargo.toml").exists() {
        ProjectType::Rust
    } else if root.join("package.json").exists() {
        ProjectType::Node
    } else if root.join("pyproject.toml").exists() || root.join("setup.py").exists() {
        ProjectType::Python
    } else if root.join("go.mod").exists() {
        ProjectType::Go
    } else {
        ProjectType::Unknown
    }
}

fn format_project_type(pt: &ProjectType) -> &'static str {
    match pt {
        ProjectType::Rust => "Rust",
        ProjectType::Node => "Node.js",
        ProjectType::Python => "Python",
        ProjectType::Go => "Go",
        ProjectType::Unknown => "Unknown",
    }
}

/// Collect source files recursively
fn collect_source_files(root: &Path, extensions: &[&str]) -> Vec<SourceFile> {
    let mut files = Vec::new();
    collect_files_recursive(root, root, extensions, &mut files);
    files.sort_by(|a, b| a.relative.cmp(&b.relative));
    files
}

fn collect_files_recursive(
    root: &Path,
    current: &Path,
    extensions: &[&str],
    files: &mut Vec<SourceFile>,
) {
    let Ok(entries) = fs::read_dir(current) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();

        // Skip hidden and common ignored directories
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        if name.starts_with('.')
            || name == "target"
            || name == "node_modules"
            || name == "__pycache__"
        {
            continue;
        }

        if path.is_dir() {
            collect_files_recursive(root, &path, extensions, files);
        } else if path.is_file() {
            if let Some(ext) = path.extension() {
                let ext_str = ext.to_string_lossy();
                if extensions.iter().any(|e| *e == ext_str) {
                    let relative = path
                        .strip_prefix(root)
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_default();

                    let lines = fs::read_to_string(&path)
                        .map(|s| s.lines().count())
                        .unwrap_or(0);

                    files.push(SourceFile {
                        path: path.clone(),
                        relative,
                        lines,
                        language: ext_str.to_string(),
                    });
                }
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// SELF-AWARENESS (for hyle developing itself)
// ═══════════════════════════════════════════════════════════════

/// Get hyle's own project context
pub fn self_project() -> Option<Project> {
    // Try to find hyle's source from executable location
    if let Ok(exe) = std::env::current_exe() {
        // Development: exe is in target/release or target/debug
        if let Some(target) = exe.parent() {
            if target
                .file_name()
                .map(|n| n == "release" || n == "debug")
                .unwrap_or(false)
            {
                if let Some(target_dir) = target.parent() {
                    if let Some(project_root) = target_dir.parent() {
                        if project_root.join("Cargo.toml").exists() {
                            return Project::detect(project_root);
                        }
                    }
                }
            }
        }
    }

    // Fallback: look in current directory
    let cwd = std::env::current_dir().ok()?;
    Project::detect(&cwd)
}

/// Check if we're running in our own source tree
pub fn is_self_development() -> bool {
    self_project()
        .map(|p| p.name == "hyle" || p.name == "claude-replacement")
        .unwrap_or(false)
}

// ═══════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_detect_project_type_rust() {
        let temp = env::temp_dir().join("test_rust_project");
        let _ = fs::create_dir_all(&temp);
        let _ = fs::write(temp.join("Cargo.toml"), "[package]\nname = \"test\"");

        assert_eq!(detect_project_type(&temp), ProjectType::Rust);

        let _ = fs::remove_dir_all(&temp);
    }

    #[test]
    fn test_detect_project_type_node() {
        let temp = env::temp_dir().join("test_node_project");
        let _ = fs::create_dir_all(&temp);
        let _ = fs::write(temp.join("package.json"), "{}");

        assert_eq!(detect_project_type(&temp), ProjectType::Node);

        let _ = fs::remove_dir_all(&temp);
    }

    #[test]
    fn test_find_project_root() {
        let temp = env::temp_dir().join("test_find_root");
        let sub = temp.join("src").join("deep");
        let _ = fs::create_dir_all(&sub);
        let _ = fs::write(temp.join("Cargo.toml"), "[package]");

        let found = find_project_root(&sub);
        assert_eq!(found, Some(temp.clone()));

        let _ = fs::remove_dir_all(&temp);
    }

    #[test]
    fn test_project_detect() {
        let temp = env::temp_dir().join("test_project_detect");
        let src = temp.join("src");
        let _ = fs::create_dir_all(&src);
        let _ = fs::write(temp.join("Cargo.toml"), "[package]\nname = \"myproject\"");
        let _ = fs::write(
            src.join("main.rs"),
            "fn main() {\n    println!(\"Hello\");\n}",
        );
        let _ = fs::write(src.join("lib.rs"), "pub fn foo() {}");

        let project = Project::detect(&temp).unwrap();

        assert_eq!(project.project_type, ProjectType::Rust);
        assert_eq!(project.files.len(), 2);
        assert!(project.structure.contains("main.rs"));
        assert!(project.structure.contains("lib.rs"));

        let _ = fs::remove_dir_all(&temp);
    }

    #[test]
    fn test_context_for_llm() {
        let temp = env::temp_dir().join("test_llm_context");
        let _ = fs::create_dir_all(&temp);
        let _ = fs::write(temp.join("Cargo.toml"), "[package]\nname = \"test\"");
        let _ = fs::write(temp.join("src/main.rs"), "fn main() {}");
        let _ = fs::create_dir_all(temp.join("src"));
        let _ = fs::write(temp.join("src/main.rs"), "fn main() {}");

        let project = Project::detect(&temp).unwrap();
        let ctx = project.context_for_llm();

        assert!(ctx.contains("<project"));
        assert!(ctx.contains("</project>"));
        assert!(ctx.contains("<manifest>"));

        let _ = fs::remove_dir_all(&temp);
    }

    #[test]
    fn test_self_awareness() {
        // This should work when running from the hyle project
        let is_self = is_self_development();
        // Don't assert - might be true or false depending on where tests run
        println!("Is self-development: {}", is_self);
    }

    #[test]
    fn test_files_matching() {
        let temp = env::temp_dir().join("test_files_matching");
        let src = temp.join("src");
        let _ = fs::create_dir_all(&src);
        let _ = fs::write(temp.join("Cargo.toml"), "[package]");
        let _ = fs::write(src.join("main.rs"), "fn main() {}");
        let _ = fs::write(src.join("config.rs"), "// config");
        let _ = fs::write(src.join("utils.rs"), "// utils");

        let project = Project::detect(&temp).unwrap();

        let matches = project.files_matching("config");
        assert_eq!(matches.len(), 1);
        assert!(matches[0].relative.contains("config"));

        let _ = fs::remove_dir_all(&temp);
    }

    #[test]
    fn test_total_lines() {
        let temp = env::temp_dir().join("test_total_lines");
        let _ = fs::create_dir_all(&temp);
        let _ = fs::write(temp.join("Cargo.toml"), "[package]");
        let _ = fs::write(temp.join("main.rs"), "line1\nline2\nline3\n");
        let _ = fs::write(temp.join("lib.rs"), "line1\nline2\n");

        let project = Project::detect(&temp).unwrap();
        assert_eq!(project.total_lines(), 5);

        let _ = fs::remove_dir_all(&temp);
    }
}
