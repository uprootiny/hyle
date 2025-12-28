//! Backburner mode - intelligent background maintenance with LLM analysis
//!
//! Features:
//! - Feature tree tracking and validation
//! - LLM-powered code analysis using free models
//! - Observability dashboard output
//! - Development suggestions

use anyhow::Result;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::time::Duration;

use crate::client;
use crate::config;
use crate::session;

/// Feature status tracking
#[derive(Debug, Clone)]
pub struct Feature {
    pub path: String,
    pub name: String,
    pub status: FeatureStatus,
    pub last_check: Option<Instant>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FeatureStatus {
    Untested,
    Passing,
    Failing,
    Partial,
}

impl FeatureStatus {
    pub fn symbol(&self) -> &'static str {
        match self {
            Self::Untested => "[ ]",
            Self::Passing => "[x]",
            Self::Failing => "[!]",
            Self::Partial => "[~]",
        }
    }
}

/// Backburner state
pub struct Backburner {
    work_dir: PathBuf,
    features: Vec<Feature>,
    api_key: Option<String>,
    model: String,
    running: Arc<AtomicBool>,
    cycle: u64,
    observations: Vec<String>,
}

impl Backburner {
    pub fn new(work_dir: PathBuf) -> Self {
        Self {
            work_dir,
            features: Self::default_features(),
            api_key: config::get_api_key().ok(),
            model: "meta-llama/llama-3.2-3b-instruct:free".to_string(),
            running: Arc::new(AtomicBool::new(true)),
            cycle: 0,
            observations: Vec::new(),
        }
    }

    fn default_features() -> Vec<Feature> {
        vec![
            Feature { path: "cli".into(), name: "--help".into(), status: FeatureStatus::Untested, last_check: None, notes: vec![] },
            Feature { path: "cli".into(), name: "--free".into(), status: FeatureStatus::Untested, last_check: None, notes: vec![] },
            Feature { path: "cli".into(), name: "--new".into(), status: FeatureStatus::Untested, last_check: None, notes: vec![] },
            Feature { path: "cli".into(), name: "--model".into(), status: FeatureStatus::Untested, last_check: None, notes: vec![] },
            Feature { path: "cli".into(), name: "--task".into(), status: FeatureStatus::Untested, last_check: None, notes: vec![] },
            Feature { path: "cli".into(), name: "--backburner".into(), status: FeatureStatus::Untested, last_check: None, notes: vec![] },
            Feature { path: "cmd".into(), name: "doctor".into(), status: FeatureStatus::Untested, last_check: None, notes: vec![] },
            Feature { path: "cmd".into(), name: "models".into(), status: FeatureStatus::Untested, last_check: None, notes: vec![] },
            Feature { path: "cmd".into(), name: "sessions".into(), status: FeatureStatus::Untested, last_check: None, notes: vec![] },
            Feature { path: "cmd".into(), name: "config".into(), status: FeatureStatus::Untested, last_check: None, notes: vec![] },
            Feature { path: "tui".into(), name: "model_picker".into(), status: FeatureStatus::Untested, last_check: None, notes: vec![] },
            Feature { path: "tui".into(), name: "chat_tab".into(), status: FeatureStatus::Untested, last_check: None, notes: vec![] },
            Feature { path: "tui".into(), name: "telemetry_tab".into(), status: FeatureStatus::Untested, last_check: None, notes: vec![] },
            Feature { path: "tui".into(), name: "log_tab".into(), status: FeatureStatus::Untested, last_check: None, notes: vec![] },
            Feature { path: "session".into(), name: "create".into(), status: FeatureStatus::Untested, last_check: None, notes: vec![] },
            Feature { path: "session".into(), name: "resume".into(), status: FeatureStatus::Untested, last_check: None, notes: vec![] },
            Feature { path: "session".into(), name: "persist_user".into(), status: FeatureStatus::Untested, last_check: None, notes: vec![] },
            Feature { path: "session".into(), name: "persist_assistant".into(), status: FeatureStatus::Untested, last_check: None, notes: vec![] },
            Feature { path: "telemetry".into(), name: "cpu_monitor".into(), status: FeatureStatus::Untested, last_check: None, notes: vec![] },
            Feature { path: "telemetry".into(), name: "mem_monitor".into(), status: FeatureStatus::Untested, last_check: None, notes: vec![] },
            Feature { path: "telemetry".into(), name: "pressure_detect".into(), status: FeatureStatus::Untested, last_check: None, notes: vec![] },
            Feature { path: "telemetry".into(), name: "auto_throttle".into(), status: FeatureStatus::Untested, last_check: None, notes: vec![] },
            Feature { path: "api".into(), name: "streaming".into(), status: FeatureStatus::Untested, last_check: None, notes: vec![] },
            Feature { path: "api".into(), name: "model_cache".into(), status: FeatureStatus::Untested, last_check: None, notes: vec![] },
            Feature { path: "api".into(), name: "free_filter".into(), status: FeatureStatus::Untested, last_check: None, notes: vec![] },
        ]
    }

    pub async fn run(&mut self) -> Result<()> {
        let running = self.running.clone();

        ctrlc::set_handler(move || {
            running.store(false, Ordering::SeqCst);
        }).ok();

        self.print_header();

        while self.running.load(Ordering::SeqCst) {
            self.cycle += 1;

            match self.cycle % 10 {
                1 => self.run_cli_tests().await,
                2 => self.run_session_cleanup(),
                3 => self.run_git_check(),
                4 => self.run_git_hygiene(),
                5 => self.analyze_code_quality().await,
                6 => self.run_cargo_checks(),
                7 => self.suggest_atomic_commit().await,
                8 => self.print_feature_dashboard(),
                9 => self.generate_suggestions().await,
                _ => self.print_heartbeat(),
            }

            // Sleep between tasks (30 seconds for faster feedback during dev)
            self.interruptible_sleep(30).await;
        }

        self.print_summary();
        Ok(())
    }

    fn print_header(&self) {
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
        println!("\n{}", "=".repeat(60));
        println!("HYLE BACKBURNER - Intelligent Maintenance Daemon");
        println!("{}", "=".repeat(60));
        println!("Started: {}", now);
        println!("Working dir: {}", self.work_dir.display());
        println!("Model: {}", self.model);
        println!("API Key: {}", if self.api_key.is_some() { "configured" } else { "missing" });
        println!("{}", "-".repeat(60));
        println!("Press Ctrl-C to stop\n");
    }

    async fn run_cli_tests(&mut self) {
        let now = self.timestamp();
        println!("[{}] Running CLI tests...", now);

        let tests = [
            ("--help", vec!["--help"]),
            ("doctor", vec!["doctor"]),
            ("sessions --list", vec!["sessions", "--list"]),
        ];

        for (name, args) in tests {
            let result = std::process::Command::new(&self.work_dir.join("target/release/hyle"))
                .args(&args)
                .output();

            match result {
                Ok(output) if output.status.success() => {
                    self.update_feature_status(&format!("cli.{}", name.split_whitespace().next().unwrap()), FeatureStatus::Passing);
                    println!("  {} {} PASS", FeatureStatus::Passing.symbol(), name);
                }
                Ok(output) => {
                    self.update_feature_status(&format!("cli.{}", name.split_whitespace().next().unwrap()), FeatureStatus::Failing);
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    println!("  {} {} FAIL: {}", FeatureStatus::Failing.symbol(), name, stderr.lines().next().unwrap_or(""));
                }
                Err(e) => {
                    println!("  [!] {} ERROR: {}", name, e);
                }
            }
        }
    }

    fn run_session_cleanup(&mut self) {
        let now = self.timestamp();
        print!("[{}] Session cleanup... ", now);

        match session::cleanup_sessions(10) {
            Ok(n) if n > 0 => {
                println!("removed {} old sessions", n);
                self.observe(format!("Cleaned {} sessions", n));
            }
            Ok(_) => println!("nothing to clean"),
            Err(e) => println!("error: {}", e),
        }
    }

    fn run_git_check(&mut self) {
        let now = self.timestamp();

        if !self.work_dir.join(".git").exists() {
            println!("[{}] Not a git repository", now);
            return;
        }

        print!("[{}] Git status... ", now);
        match std::process::Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(&self.work_dir)
            .output()
        {
            Ok(output) => {
                let changes = String::from_utf8_lossy(&output.stdout);
                let count = changes.lines().count();
                if count > 0 {
                    println!("{} uncommitted changes", count);
                    self.observe(format!("Git: {} uncommitted changes", count));
                    // Show first few changes
                    for line in changes.lines().take(5) {
                        println!("    {}", line);
                    }
                    if count > 5 {
                        println!("    ... and {} more", count - 5);
                    }
                } else {
                    println!("clean");
                }
            }
            Err(e) => println!("error: {}", e),
        }
    }

    fn run_git_hygiene(&mut self) {
        let now = self.timestamp();

        if !self.work_dir.join(".git").exists() {
            return;
        }

        println!("[{}] Git hygiene check...", now);

        // Check for uncommitted changes that could be committed atomically
        let status = std::process::Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(&self.work_dir)
            .output();

        if let Ok(output) = status {
            let changes = String::from_utf8_lossy(&output.stdout);
            let lines: Vec<&str> = changes.lines().collect();

            if lines.is_empty() {
                println!("  [x] Working tree clean");
                return;
            }

            // Group changes by directory/module
            let mut by_dir: std::collections::HashMap<String, Vec<&str>> = std::collections::HashMap::new();
            for line in &lines {
                if line.len() < 3 { continue; }
                let path = &line[3..];
                let dir = path.split('/').next().unwrap_or("root");
                by_dir.entry(dir.to_string()).or_default().push(line);
            }

            // Suggest atomic commits
            if by_dir.len() == 1 {
                println!("  [~] {} changes in single area - good for atomic commit", lines.len());
            } else {
                println!("  [!] Changes span {} areas - consider separate commits:", by_dir.len());
                for (dir, files) in &by_dir {
                    println!("      {}: {} files", dir, files.len());
                }
                self.observe(format!("Git hygiene: changes span {} areas", by_dir.len()));
            }
        }

        // Check last commit message quality
        let log = std::process::Command::new("git")
            .args(["log", "-1", "--format=%s"])
            .current_dir(&self.work_dir)
            .output();

        if let Ok(output) = log {
            let msg = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !msg.is_empty() {
                let issues = self.analyze_commit_message(&msg);
                if issues.is_empty() {
                    println!("  [x] Last commit message OK: {}", &msg[..msg.len().min(50)]);
                } else {
                    println!("  [!] Last commit message issues:");
                    for issue in &issues {
                        println!("      - {}", issue);
                    }
                }
            }
        }

        // Run git fsck occasionally
        if self.cycle % 20 == 0 {
            print!("  Running git fsck... ");
            match std::process::Command::new("git")
                .args(["fsck", "--quick"])
                .current_dir(&self.work_dir)
                .output()
            {
                Ok(output) if output.status.success() => println!("OK"),
                Ok(output) => {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    println!("issues found");
                    for line in stderr.lines().take(3) {
                        println!("      {}", line);
                    }
                }
                Err(e) => println!("error: {}", e),
            }
        }
    }

    fn analyze_commit_message(&self, msg: &str) -> Vec<String> {
        let mut issues = Vec::new();

        // Check length
        if msg.len() < 10 {
            issues.push("Too short - should describe the change".into());
        }
        if msg.len() > 72 {
            issues.push("Subject line over 72 chars".into());
        }

        // Check capitalization
        if msg.chars().next().map(|c| c.is_lowercase()).unwrap_or(false) {
            issues.push("Should start with capital letter".into());
        }

        // Check for period at end
        if msg.ends_with('.') {
            issues.push("Subject line should not end with period".into());
        }

        // Check for imperative mood indicators
        let past_tense = ["added", "fixed", "changed", "updated", "removed", "deleted"];
        let first_word = msg.split_whitespace().next().unwrap_or("").to_lowercase();
        if past_tense.contains(&first_word.as_str()) {
            issues.push("Use imperative mood (e.g., 'Add' not 'Added')".into());
        }

        // Check for common bad patterns
        if msg.to_lowercase().starts_with("wip") {
            issues.push("WIP commits should be squashed before merge".into());
        }
        if msg == "fix" || msg == "update" || msg == "changes" {
            issues.push("Commit message is too vague".into());
        }

        issues
    }

    async fn suggest_atomic_commit(&mut self) {
        let now = self.timestamp();

        let api_key = match &self.api_key {
            Some(k) => k.clone(),
            None => return,
        };

        // Get staged diff
        let diff = std::process::Command::new("git")
            .args(["diff", "--cached", "--stat"])
            .current_dir(&self.work_dir)
            .output();

        let diff_stat = match diff {
            Ok(output) => String::from_utf8_lossy(&output.stdout).to_string(),
            Err(_) => return,
        };

        if diff_stat.trim().is_empty() {
            return;
        }

        println!("[{}] Suggesting commit message...", now);

        let prompt = format!(
            "Based on this git diff stat, suggest a concise commit message in imperative mood. \
            The message should be one line, under 72 chars, no period at end, start with capital.\n\n\
            Diff stat:\n{}\n\nSuggested commit message:",
            diff_stat
        );

        match client::stream_completion(&api_key, &self.model, &prompt).await {
            Ok(mut stream) => {
                print!("  Suggestion: ");
                while let Some(event) = stream.recv().await {
                    match event {
                        client::StreamEvent::Token(t) => print!("{}", t),
                        client::StreamEvent::Done(_) => println!(),
                        client::StreamEvent::Error(e) => {
                            println!("Error: {}", e);
                            break;
                        }
                    }
                }
            }
            Err(e) => println!("  Error: {}", e),
        }
    }

    fn run_cargo_checks(&mut self) {
        let now = self.timestamp();

        if !self.work_dir.join("Cargo.toml").exists() {
            return;
        }

        print!("[{}] Cargo check... ", now);
        match std::process::Command::new("cargo")
            .args(["check", "--message-format=short"])
            .current_dir(&self.work_dir)
            .output()
        {
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let warnings: Vec<_> = stderr.lines().filter(|l| l.contains("warning")).collect();
                let errors: Vec<_> = stderr.lines().filter(|l| l.contains("error")).collect();

                if errors.is_empty() && warnings.is_empty() {
                    println!("OK");
                } else if errors.is_empty() {
                    println!("{} warnings", warnings.len());
                    self.observe(format!("Cargo: {} warnings", warnings.len()));
                } else {
                    println!("{} errors, {} warnings", errors.len(), warnings.len());
                    self.observe(format!("Cargo: {} errors!", errors.len()));
                }
            }
            Err(e) => println!("error: {}", e),
        }
    }

    async fn analyze_code_quality(&mut self) {
        let now = self.timestamp();

        if self.api_key.is_none() {
            println!("[{}] Code analysis skipped (no API key)", now);
            return;
        }

        // Count lines of code
        let mut total_lines = 0;
        let mut file_count = 0;

        if let Ok(entries) = std::fs::read_dir(self.work_dir.join("src")) {
            for entry in entries.filter_map(|e| e.ok()) {
                if entry.path().extension().map(|e| e == "rs").unwrap_or(false) {
                    if let Ok(content) = std::fs::read_to_string(entry.path()) {
                        total_lines += content.lines().count();
                        file_count += 1;
                    }
                }
            }
        }

        println!("[{}] Code stats: {} files, {} lines", now, file_count, total_lines);
        self.observe(format!("Codebase: {} files, {} lines", file_count, total_lines));
    }

    async fn generate_suggestions(&mut self) {
        let now = self.timestamp();

        let api_key = match &self.api_key {
            Some(k) => k.clone(),
            None => {
                println!("[{}] Suggestions skipped (no API key)", now);
                return;
            }
        };

        println!("[{}] Generating improvement suggestions...", now);

        // Build context from observations
        let context = if self.observations.is_empty() {
            "No observations yet.".to_string()
        } else {
            self.observations.join("\n")
        };

        let prompt = format!(
            "You are analyzing a Rust CLI project called 'hyle' (a code assistant). \
            Based on these recent observations:\n{}\n\n\
            Give 1-2 brief, actionable suggestions for improvement. Be concise (2-3 sentences max).",
            context
        );

        match client::stream_completion(&api_key, &self.model, &prompt).await {
            Ok(mut stream) => {
                print!("  > ");
                while let Some(event) = stream.recv().await {
                    match event {
                        client::StreamEvent::Token(t) => print!("{}", t),
                        client::StreamEvent::Done(_) => println!(),
                        client::StreamEvent::Error(e) => {
                            println!("\n  Error: {}", e);
                            break;
                        }
                    }
                }
                std::io::Write::flush(&mut std::io::stdout()).ok();
            }
            Err(e) => println!("  Error: {}", e),
        }
    }

    fn print_feature_dashboard(&self) {
        let now = self.timestamp();
        println!("\n[{}] Feature Dashboard", now);
        println!("{}", "-".repeat(50));

        // Group by path
        let mut by_path: std::collections::HashMap<&str, Vec<&Feature>> = std::collections::HashMap::new();
        for f in &self.features {
            by_path.entry(&f.path).or_default().push(f);
        }

        for (path, features) in &by_path {
            let passing = features.iter().filter(|f| f.status == FeatureStatus::Passing).count();
            let total = features.len();
            let pct = (passing * 100) / total.max(1);

            println!("{}: {}/{} ({:>3}%)", path, passing, total, pct);
            for f in features.iter().take(3) {
                println!("  {} {}", f.status.symbol(), f.name);
            }
            if features.len() > 3 {
                println!("  ... and {} more", features.len() - 3);
            }
        }

        let total_passing = self.features.iter().filter(|f| f.status == FeatureStatus::Passing).count();
        let total = self.features.len();
        println!("{}", "-".repeat(50));
        println!("Total: {}/{} ({:.0}%)", total_passing, total, (total_passing as f64 / total as f64) * 100.0);
        println!();
    }

    fn print_heartbeat(&self) {
        let now = self.timestamp();
        let uptime = self.cycle * 30; // seconds
        println!("[{}] Heartbeat (cycle {}, uptime {}s)", now, self.cycle, uptime);
    }

    fn print_summary(&self) {
        println!("\n{}", "=".repeat(60));
        println!("BACKBURNER SESSION SUMMARY");
        println!("{}", "=".repeat(60));
        println!("Total cycles: {}", self.cycle);
        println!("Observations: {}", self.observations.len());

        let passing = self.features.iter().filter(|f| f.status == FeatureStatus::Passing).count();
        println!("Features passing: {}/{}", passing, self.features.len());

        if !self.observations.is_empty() {
            println!("\nKey observations:");
            for obs in self.observations.iter().take(10) {
                println!("  - {}", obs);
            }
        }
        println!();
    }

    fn update_feature_status(&mut self, path: &str, status: FeatureStatus) {
        // Simple matching - could be improved
        for f in &mut self.features {
            if path.contains(&f.name) {
                f.status = status;
                f.last_check = Some(Instant::now());
            }
        }
    }

    fn observe(&mut self, msg: String) {
        let now = chrono::Local::now().format("%H:%M:%S");
        self.observations.push(format!("[{}] {}", now, msg));
        // Keep last 50 observations
        if self.observations.len() > 50 {
            self.observations.remove(0);
        }
    }

    fn timestamp(&self) -> String {
        chrono::Local::now().format("%H:%M:%S").to_string()
    }

    async fn interruptible_sleep(&self, seconds: u64) {
        for _ in 0..seconds {
            if !self.running.load(Ordering::SeqCst) {
                break;
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }
}
