//! Project Orchestrator - spawn and manage hyle instances for project creation
//!
//! Accepts project sketches via web, scaffolds infrastructure, and dispatches
//! autonomous hyle instances to build out projects.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

// ═══════════════════════════════════════════════════════════════
// PROJECT TYPES
// ═══════════════════════════════════════════════════════════════

/// Project specification parsed from user sketch
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectSpec {
    pub name: String,
    pub project_type: ProjectType,
    pub description: String,
    pub sketch: String,
    pub subdomain: Option<String>,
    pub port: Option<u16>,
    pub features: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProjectType {
    Rust,
    Clojure,
    ClojureScript,
    Node,
    Static,
    Unknown,
}

impl ProjectType {
    pub fn detect(sketch: &str) -> Self {
        let lower = sketch.to_lowercase();
        if lower.contains("cargo.toml") || lower.contains("fn main") || lower.contains("use std::")
        {
            ProjectType::Rust
        } else if lower.contains("deps.edn") || lower.contains("(defn ") || lower.contains("(ns ") {
            ProjectType::Clojure
        } else if lower.contains("shadow-cljs")
            || lower.contains("reagent")
            || lower.contains("re-frame")
        {
            ProjectType::ClojureScript
        } else if lower.contains("package.json")
            || lower.contains("const ")
            || lower.contains("import ")
        {
            ProjectType::Node
        } else if lower.contains("<html") || lower.contains("<!doctype") {
            ProjectType::Static
        } else {
            ProjectType::Unknown
        }
    }
}

/// Project status tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub spec: ProjectSpec,
    pub status: ProjectStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub project_dir: PathBuf,
    pub log: Vec<ProjectEvent>,
    pub hyle_pid: Option<u32>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectStatus {
    Pending,
    Scaffolding,
    Building,
    Testing,
    Deploying,
    Running,
    Failed,
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectEvent {
    pub timestamp: DateTime<Utc>,
    pub kind: String,
    pub message: String,
}

// ═══════════════════════════════════════════════════════════════
// ORCHESTRATOR STATE
// ═══════════════════════════════════════════════════════════════

pub struct Orchestrator {
    pub projects: HashMap<String, Project>,
    pub projects_root: PathBuf,
    pub hyle_binary: PathBuf,
    pub domain: String,
}

impl Orchestrator {
    pub fn new(projects_root: PathBuf, hyle_binary: PathBuf, domain: String) -> Self {
        Self {
            projects: HashMap::new(),
            projects_root,
            hyle_binary,
            domain,
        }
    }

    /// Submit a new project from a sketch
    pub fn submit_project(&mut self, sketch: &str) -> Result<String> {
        let spec = parse_project_spec(sketch)?;
        let id = generate_project_id(&spec.name);
        let project_dir = self.projects_root.join(&id);

        let project = Project {
            id: id.clone(),
            spec,
            status: ProjectStatus::Pending,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            project_dir,
            log: vec![ProjectEvent {
                timestamp: Utc::now(),
                kind: "created".into(),
                message: "Project submitted".into(),
            }],
            hyle_pid: None,
            url: None,
        };

        self.projects.insert(id.clone(), project);
        Ok(id)
    }

    /// Get project status
    pub fn get_project(&self, id: &str) -> Option<&Project> {
        self.projects.get(id)
    }

    /// List all projects
    pub fn list_projects(&self) -> Vec<&Project> {
        let mut projects: Vec<_> = self.projects.values().collect();
        projects.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        projects
    }

    /// Add event to project log
    pub fn log_event(&mut self, id: &str, kind: &str, message: &str) {
        if let Some(project) = self.projects.get_mut(id) {
            project.log.push(ProjectEvent {
                timestamp: Utc::now(),
                kind: kind.into(),
                message: message.into(),
            });
            project.updated_at = Utc::now();
        }
    }

    /// Update project status
    pub fn set_status(&mut self, id: &str, status: ProjectStatus) {
        if let Some(project) = self.projects.get_mut(id) {
            project.status = status;
            project.updated_at = Utc::now();
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// SKETCH PARSING
// ═══════════════════════════════════════════════════════════════

/// Parse a project specification from a user sketch
pub fn parse_project_spec(sketch: &str) -> Result<ProjectSpec> {
    let lines: Vec<&str> = sketch.lines().collect();

    // Try to extract project name from common patterns
    let name =
        extract_project_name(sketch).unwrap_or_else(|| format!("project-{}", &generate_id()[..8]));

    let project_type = ProjectType::detect(sketch);

    // Extract description from first comment block or first paragraph
    let description =
        extract_description(sketch).unwrap_or_else(|| "Auto-generated project".into());

    // Extract subdomain if mentioned
    let subdomain = extract_subdomain(sketch);

    // Detect desired port
    let port = extract_port(sketch);

    // Extract feature keywords
    let features = extract_features(sketch);

    Ok(ProjectSpec {
        name,
        project_type,
        description,
        sketch: sketch.to_string(),
        subdomain,
        port,
        features,
    })
}

fn extract_project_name(sketch: &str) -> Option<String> {
    for line in sketch.lines().take(50) {
        let trimmed = line.trim();

        // Check for markdown header: # Project-Name
        if let Some(name) = trimmed.strip_prefix("# ") {
            let name = name.trim();
            if !name.is_empty() && name.len() <= 64 {
                let clean: String = name
                    .chars()
                    .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_' || *c == ' ')
                    .collect();
                return Some(clean.to_lowercase().replace(' ', "-"));
            }
        }

        // Check for name = "value" or name: "value"
        let lower = trimmed.to_lowercase();
        if lower.starts_with("name") {
            if let Some(idx) = trimmed.find('=').or_else(|| trimmed.find(':')) {
                let value = trimmed[idx + 1..].trim();
                let clean = value.trim_matches(|c| c == '"' || c == '\'' || c == ' ');
                if !clean.is_empty() && clean.len() <= 64 {
                    return Some(clean.to_lowercase().replace(' ', "-"));
                }
            }
        }
    }
    None
}

fn extract_description(sketch: &str) -> Option<String> {
    // Look for description in comments or first paragraph
    for line in sketch.lines().take(20) {
        let trimmed = line.trim();
        if trimmed.starts_with("//") || trimmed.starts_with("#") || trimmed.starts_with(";;") {
            let desc = trimmed.trim_start_matches(&['/', '#', ';', '!', ' '][..]);
            if desc.len() > 10 && desc.len() < 200 {
                return Some(desc.to_string());
            }
        }
    }
    None
}

fn extract_subdomain(sketch: &str) -> Option<String> {
    // Simple pattern: subdomain = "foo" or subdomain: foo
    for line in sketch.lines() {
        let lower = line.to_lowercase();
        if lower.contains("subdomain") {
            // Extract value after = or :
            if let Some(idx) = line.find('=').or_else(|| line.find(':')) {
                let value = line[idx + 1..].trim();
                let clean = value.trim_matches(|c| c == '"' || c == '\'' || c == ' ');
                if !clean.is_empty() && clean.chars().all(|c| c.is_alphanumeric() || c == '-') {
                    return Some(clean.to_string());
                }
            }
        }
    }
    None
}

/// Minimum allowed port (above privileged range)
const MIN_PORT: u16 = 1024;
/// Maximum allowed port
const MAX_PORT: u16 = 65535;

fn extract_port(sketch: &str) -> Option<u16> {
    // Simple pattern: port = 3000 or port: 3000
    for line in sketch.lines() {
        let lower = line.to_lowercase();
        if lower.contains("port") {
            if let Some(idx) = line.find('=').or_else(|| line.find(':')) {
                let value = line[idx + 1..].trim();
                if let Ok(port) = value.parse::<u16>() {
                    // INVARIANT: Only allow unprivileged ports
                    if (MIN_PORT..=MAX_PORT).contains(&port) {
                        return Some(port);
                    }
                }
            }
        }
    }
    None
}

/// Validate that a port is in the allowed range
pub fn validate_port(port: u16) -> Result<u16> {
    if port < MIN_PORT {
        anyhow::bail!("Port {} is privileged (must be >= {})", port, MIN_PORT);
    }
    // MAX_PORT == u16::MAX, so no upper bound check needed
    Ok(port)
}

fn extract_features(sketch: &str) -> Vec<String> {
    let mut features = Vec::new();
    let keywords = [
        "api",
        "rest",
        "graphql",
        "websocket",
        "auth",
        "database",
        "postgres",
        "sqlite",
        "redis",
        "docker",
        "kubernetes",
        "react",
        "vue",
        "svelte",
        "tailwind",
        "htmx",
    ];

    let lower = sketch.to_lowercase();
    for kw in keywords {
        if lower.contains(kw) {
            features.push(kw.to_string());
        }
    }
    features
}

// ═══════════════════════════════════════════════════════════════
// SCAFFOLDING
// ═══════════════════════════════════════════════════════════════

/// Scaffold a new project directory
///
/// INVARIANT: project_dir must be a direct child of a known projects_root.
/// This function validates that the path doesn't escape via traversal.
pub fn scaffold_project(project: &Project, projects_root: &Path) -> Result<()> {
    let dir = &project.project_dir;

    // SECURITY: Validate path is under projects_root
    let canonical_root = projects_root
        .canonicalize()
        .context("projects_root must exist")?;

    // Create the directory first so we can canonicalize it
    fs::create_dir_all(dir)?;

    let canonical_dir = dir
        .canonicalize()
        .context("Failed to canonicalize project directory")?;

    if !canonical_dir.starts_with(&canonical_root) {
        // Clean up the directory we just created
        let _ = fs::remove_dir_all(dir);
        anyhow::bail!(
            "Path traversal detected: {:?} is not under {:?}",
            canonical_dir,
            canonical_root
        );
    }

    match project.spec.project_type {
        ProjectType::Rust => scaffold_rust(dir, &project.spec)?,
        ProjectType::Clojure => scaffold_clojure(dir, &project.spec)?,
        ProjectType::ClojureScript => scaffold_clojurescript(dir, &project.spec)?,
        ProjectType::Node => scaffold_node(dir, &project.spec)?,
        ProjectType::Static => scaffold_static(dir, &project.spec)?,
        ProjectType::Unknown => scaffold_generic(dir, &project.spec)?,
    }

    // Initialize git repo
    Command::new("git")
        .args(["init"])
        .current_dir(dir)
        .output()?;

    // Write the original sketch as context
    fs::write(dir.join("SKETCH.md"), &project.spec.sketch)?;

    // Create initial .gitignore
    let gitignore = match project.spec.project_type {
        ProjectType::Rust => "target/\n*.swp\n.env\n",
        ProjectType::Clojure => ".cpcache/\ntarget/\n.nrepl-port\n*.swp\n",
        ProjectType::ClojureScript => "node_modules/\n.shadow-cljs/\npublic/js/\n",
        ProjectType::Node => "node_modules/\ndist/\n.env\n",
        _ => "*.swp\n.env\n",
    };
    fs::write(dir.join(".gitignore"), gitignore)?;

    Ok(())
}

fn scaffold_rust(dir: &Path, spec: &ProjectSpec) -> Result<()> {
    // Use cargo to create project structure
    Command::new("cargo")
        .args(["init", "--name", &spec.name])
        .current_dir(dir)
        .output()?;

    // If sketch contains Cargo.toml content, use it
    if spec.sketch.contains("[package]") {
        if let Some(cargo_toml) = extract_code_block(&spec.sketch, "toml") {
            fs::write(dir.join("Cargo.toml"), cargo_toml)?;
        }
    }

    // If sketch contains main.rs content, use it
    if let Some(main_rs) = extract_code_block(&spec.sketch, "rust") {
        fs::write(dir.join("src/main.rs"), main_rs)?;
    }

    Ok(())
}

fn scaffold_clojure(dir: &Path, spec: &ProjectSpec) -> Result<()> {
    fs::create_dir_all(dir.join("src"))?;
    fs::create_dir_all(dir.join("test"))?;

    // Create deps.edn
    let deps_edn = if let Some(deps) = extract_code_block(&spec.sketch, "edn") {
        deps
    } else {
        format!(
            r#"{{:paths ["src" "resources"]
 :deps {{org.clojure/clojure {{:mvn/version "1.11.1"}}}}
 :aliases {{:dev {{:extra-paths ["test"]}}
            :run {{:main-opts ["-m" "{}.core"]}}}}}}
"#,
            spec.name.replace('-', "_")
        )
    };
    fs::write(dir.join("deps.edn"), deps_edn)?;

    // Create main namespace
    let ns_name = spec.name.replace('-', "_");
    let src_dir = dir.join("src").join(&ns_name);
    fs::create_dir_all(&src_dir)?;

    let core_clj = if let Some(code) = extract_code_block(&spec.sketch, "clojure") {
        code
    } else {
        format!(
            r#"(ns {}.core)

(defn -main [& args]
  (println "Hello from {}!"))
"#,
            ns_name, spec.name
        )
    };
    fs::write(src_dir.join("core.clj"), core_clj)?;

    Ok(())
}

fn scaffold_clojurescript(dir: &Path, spec: &ProjectSpec) -> Result<()> {
    fs::create_dir_all(dir.join("src"))?;
    fs::create_dir_all(dir.join("public"))?;

    // Create shadow-cljs.edn
    let shadow_config = format!(
        r#"{{:source-paths ["src"]
 :dependencies [[reagent "1.2.0"]]
 :builds {{:app {{:target :browser
                 :output-dir "public/js"
                 :asset-path "/js"
                 :modules {{:main {{:init-fn {}.core/init}}}}}}}}}}
"#,
        spec.name.replace('-', "_")
    );
    let shadow_path = dir.join("shadow-cljs.edn");
    fs::write(&shadow_path, shadow_config)?;

    // Create package.json
    let package_json = format!(
        r#"{{
  "name": "{}",
  "scripts": {{
    "dev": "shadow-cljs watch app",
    "build": "shadow-cljs release app"
  }},
  "devDependencies": {{
    "shadow-cljs": "^2.26.0"
  }}
}}
"#,
        spec.name
    );
    let pkg_path = dir.join("package.json");
    fs::write(&pkg_path, package_json)?;

    // Create index.html
    let index_html = format!(
        r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <title>{}</title>
</head>
<body>
    <div id="app"></div>
    <script src="/js/main.js"></script>
</body>
</html>
"#,
        spec.name
    );
    let html_path = dir.join("public").join("index.html");
    fs::write(&html_path, index_html)?;

    Ok(())
}

fn scaffold_node(dir: &Path, spec: &ProjectSpec) -> Result<()> {
    fs::create_dir_all(dir.join("src"))?;

    // Create package.json
    let package_json = if let Some(pkg) = extract_code_block(&spec.sketch, "json") {
        pkg
    } else {
        format!(
            r#"{{
  "name": "{}",
  "version": "0.1.0",
  "description": "{}",
  "main": "src/index.js",
  "scripts": {{
    "start": "node src/index.js",
    "dev": "node --watch src/index.js",
    "test": "node --test"
  }}
}}
"#,
            spec.name, spec.description
        )
    };
    let pkg_path = dir.join("package.json");
    fs::write(&pkg_path, package_json)?;

    // Create main file
    let index_js = if let Some(code) = extract_code_block(&spec.sketch, "javascript") {
        code
    } else {
        format!("console.log(\"Hello from {}!\");\n", spec.name)
    };
    let index_path = dir.join("src").join("index.js");
    fs::write(&index_path, index_js)?;

    Ok(())
}

fn scaffold_static(dir: &Path, spec: &ProjectSpec) -> Result<()> {
    // Just create public directory and extract HTML
    fs::create_dir_all(dir.join("public"))?;

    let index_html = if let Some(html) = extract_code_block(&spec.sketch, "html") {
        html
    } else {
        format!(
            r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <title>{}</title>
</head>
<body>
    <h1>{}</h1>
    <p>{}</p>
</body>
</html>
"#,
            spec.name, spec.name, spec.description
        )
    };
    let html_path = dir.join("public").join("index.html");
    fs::write(&html_path, index_html)?;

    Ok(())
}

fn scaffold_generic(dir: &Path, spec: &ProjectSpec) -> Result<()> {
    // Create a basic structure with the sketch
    fs::create_dir_all(dir.join("src"))?;
    fs::write(
        dir.join("README.md"),
        format!("# {}\n\n{}\n", spec.name, spec.description),
    )?;
    Ok(())
}

/// Extract a code block of a specific language from markdown-style sketch
fn extract_code_block(sketch: &str, lang: &str) -> Option<String> {
    let start_marker = format!("```{}", lang);
    let end_marker = "```";

    let mut in_block = false;
    let mut content = Vec::new();

    for line in sketch.lines() {
        if !in_block {
            if line.trim().starts_with(&start_marker) {
                in_block = true;
                continue;
            }
        } else {
            if line.trim() == end_marker {
                break;
            }
            content.push(line);
        }
    }

    if content.is_empty() {
        None
    } else {
        Some(content.join("\n"))
    }
}

// ═══════════════════════════════════════════════════════════════
// INFRASTRUCTURE AUTOMATION
// ═══════════════════════════════════════════════════════════════

/// Generate nginx config for a subdomain
pub fn generate_nginx_config(subdomain: &str, domain: &str, port: u16) -> String {
    format!(
        r#"server {{
    listen 80;
    listen [::]:80;
    server_name {subdomain}.{domain};
    return 301 https://$server_name$request_uri;
}}

server {{
    listen 443 ssl http2;
    listen [::]:443 ssl http2;
    server_name {subdomain}.{domain};

    ssl_certificate /etc/letsencrypt/live/{subdomain}.{domain}/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/{subdomain}.{domain}/privkey.pem;

    location / {{
        proxy_pass http://127.0.0.1:{port};
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection 'upgrade';
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_cache_bypass $http_upgrade;
    }}
}}
"#
    )
}

/// Generate systemd service for a project
pub fn generate_systemd_service(project: &Project) -> String {
    let exec_start = match project.spec.project_type {
        ProjectType::Rust => format!(
            "{}/target/release/{}",
            project.project_dir.display(),
            project.spec.name
        ),
        ProjectType::Clojure => "clj -M:run".to_string(),
        ProjectType::ClojureScript => "npx shadow-cljs server".into(),
        ProjectType::Node => "node src/index.js".into(),
        _ => "echo 'No start command'".into(),
    };

    format!(
        r#"[Unit]
Description={name} - {desc}
After=network.target

[Service]
Type=simple
User=www-data
WorkingDirectory={dir}
ExecStart={exec_start}
Restart=on-failure
RestartSec=5
StandardOutput=journal
StandardError=journal
Environment=PORT={port}

[Install]
WantedBy=multi-user.target
"#,
        name = project.spec.name,
        desc = project.spec.description,
        dir = project.project_dir.display(),
        exec_start = exec_start,
        port = project.spec.port.unwrap_or(3000),
    )
}

// ═══════════════════════════════════════════════════════════════
// HYLE DISPATCH
// ═══════════════════════════════════════════════════════════════

/// Build prompt for dispatched hyle instance
pub fn build_dispatch_prompt(project: &Project) -> String {
    format!(
        r#"You are building project "{name}" from the following sketch.

PROJECT SKETCH:
{sketch}

YOUR TASK:
1. Read and understand the full sketch
2. Implement all described functionality
3. Write tests for critical paths
4. Ensure the project builds and runs
5. Create a README.md with usage instructions

PROJECT TYPE: {ptype:?}
FEATURES: {features}

Work autonomously. Use /build, /test, /check commands to verify your work.
When complete, ensure all tests pass and the project is ready to deploy.

Begin implementation now.
"#,
        name = project.spec.name,
        sketch = project.spec.sketch,
        ptype = project.spec.project_type,
        features = project.spec.features.join(", "),
    )
}

/// Dispatch a hyle instance to build a project
pub fn dispatch_hyle(
    hyle_binary: &Path,
    project_dir: &Path,
    prompt: &str,
) -> Result<std::process::Child> {
    let child = Command::new(hyle_binary)
        .arg("--trust") // Auto-approve tool calls
        .arg(prompt)
        .current_dir(project_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Failed to spawn hyle instance")?;

    Ok(child)
}

// ═══════════════════════════════════════════════════════════════
// UTILITIES
// ═══════════════════════════════════════════════════════════════

fn generate_project_id(name: &str) -> String {
    let timestamp = Utc::now().format("%Y%m%d-%H%M%S");
    let clean_name: String = name
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-')
        .take(32)
        .collect();
    format!("{}-{}", clean_name, timestamp)
}

fn generate_id() -> String {
    let now = Utc::now();
    let nanos = now.timestamp_subsec_nanos();
    let pid = std::process::id();
    format!("{:08x}{:08x}", nanos, pid)
}

// ═══════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_project_type_detection() {
        assert_eq!(ProjectType::detect("fn main() { }"), ProjectType::Rust);
        assert_eq!(
            ProjectType::detect("(defn foo [] 42)"),
            ProjectType::Clojure
        );
        assert_eq!(
            ProjectType::detect("shadow-cljs.edn"),
            ProjectType::ClojureScript
        );
        assert_eq!(ProjectType::detect("package.json"), ProjectType::Node);
        assert_eq!(ProjectType::detect("<html>"), ProjectType::Static);
    }

    #[test]
    fn test_parse_project_spec() {
        let sketch = r#"# my-awesome-app

A cool application.

```rust
fn main() {
    println!("Hello!");
}
```
"#;
        let spec = parse_project_spec(sketch).unwrap();
        assert_eq!(spec.name, "my-awesome-app");
        assert_eq!(spec.project_type, ProjectType::Rust);
    }

    #[test]
    fn test_extract_code_block() {
        let sketch = "```rust\nfn main() {}\n```";
        let code = extract_code_block(sketch, "rust").unwrap();
        // Note: extract_code_block joins lines without trailing newline
        assert_eq!(code, "fn main() {}");

        // Multi-line code block
        let sketch2 = "```rust\nfn main() {\n    println!(\"hello\");\n}\n```";
        let code2 = extract_code_block(sketch2, "rust").unwrap();
        assert_eq!(code2, "fn main() {\n    println!(\"hello\");\n}");
    }

    #[test]
    fn test_generate_nginx_config() {
        let config = generate_nginx_config("myapp", "example.com", 3000);
        assert!(config.contains("server_name myapp.example.com"));
        assert!(config.contains("proxy_pass http://127.0.0.1:3000"));
    }

    #[test]
    fn test_port_validation() {
        // Valid ports
        assert!(validate_port(3000).is_ok());
        assert!(validate_port(8080).is_ok());
        assert!(validate_port(1024).is_ok()); // minimum
        assert!(validate_port(65535).is_ok()); // maximum

        // Invalid ports
        assert!(validate_port(80).is_err()); // privileged
        assert!(validate_port(443).is_err()); // privileged
        assert!(validate_port(1023).is_err()); // just below minimum
    }

    #[test]
    fn test_extract_port_rejects_privileged() {
        // Should NOT extract privileged port
        assert_eq!(extract_port("port = 80"), None);
        assert_eq!(extract_port("port = 443"), None);

        // Should extract valid port
        assert_eq!(extract_port("port = 3000"), Some(3000));
        assert_eq!(extract_port("port: 8080"), Some(8080));
    }

    #[test]
    fn test_subdomain_validation() {
        // Valid subdomains
        assert_eq!(extract_subdomain("subdomain = foo"), Some("foo".into()));
        assert_eq!(
            extract_subdomain("subdomain = my-app"),
            Some("my-app".into())
        );

        // Reject path traversal attempts in subdomain
        assert_eq!(extract_subdomain("subdomain = ../etc"), None);
        assert_eq!(extract_subdomain("subdomain = foo/bar"), None);
    }
}
