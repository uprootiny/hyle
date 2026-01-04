// ═══════════════════════════════════════════════════════════════
// BENCHMARK: Housekeeping & Profiling System
// ═══════════════════════════════════════════════════════════════
//
// Evaluates LLMs on repository hygiene and maintenance tasks.
// Scores models based on accuracy, efficiency, and code quality.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};

// ═══════════════════════════════════════════════════════════════
// PROMPT CATEGORIES
// ═══════════════════════════════════════════════════════════════

/// Categories of housekeeping tasks for LLM evaluation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TaskCategory {
    /// Code cleanup: dead code removal, unused imports
    CodeCleanup,
    /// Documentation: README, docstrings, comments
    Documentation,
    /// Dependency management: updates, audit, pruning
    Dependencies,
    /// Testing: coverage gaps, test quality
    Testing,
    /// Security: vulnerability detection, secrets scanning
    Security,
    /// Performance: profiling suggestions, optimization
    Performance,
    /// Structure: refactoring, organization
    Structure,
    /// Git hygiene: commit messages, branch cleanup
    GitHygiene,
}

impl TaskCategory {
    pub fn all() -> &'static [TaskCategory] {
        &[
            TaskCategory::CodeCleanup,
            TaskCategory::Documentation,
            TaskCategory::Dependencies,
            TaskCategory::Testing,
            TaskCategory::Security,
            TaskCategory::Performance,
            TaskCategory::Structure,
            TaskCategory::GitHygiene,
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            TaskCategory::CodeCleanup => "Code Cleanup",
            TaskCategory::Documentation => "Documentation",
            TaskCategory::Dependencies => "Dependencies",
            TaskCategory::Testing => "Testing",
            TaskCategory::Security => "Security",
            TaskCategory::Performance => "Performance",
            TaskCategory::Structure => "Structure",
            TaskCategory::GitHygiene => "Git Hygiene",
        }
    }

    pub fn weight(&self) -> f64 {
        match self {
            TaskCategory::Security => 1.5, // Security is critical
            TaskCategory::Testing => 1.3,  // Tests are important
            TaskCategory::CodeCleanup => 1.0,
            TaskCategory::Documentation => 0.8,
            TaskCategory::Dependencies => 1.2,
            TaskCategory::Performance => 1.1,
            TaskCategory::Structure => 1.0,
            TaskCategory::GitHygiene => 0.7,
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// PROMPT SET
// ═══════════════════════════════════════════════════════════════

/// A single benchmark prompt with expected evaluation criteria
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkPrompt {
    pub id: String,
    pub category: TaskCategory,
    pub prompt: String,
    pub context: Option<String>,
    pub expected_elements: Vec<String>,
    pub negative_elements: Vec<String>,
    pub max_tokens: u32,
    pub difficulty: Difficulty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Difficulty {
    Easy,
    Medium,
    Hard,
}

impl Difficulty {
    pub fn multiplier(&self) -> f64 {
        match self {
            Difficulty::Easy => 1.0,
            Difficulty::Medium => 1.5,
            Difficulty::Hard => 2.0,
        }
    }
}

/// The complete housekeeping prompt set
pub struct PromptSet {
    prompts: Vec<BenchmarkPrompt>,
}

impl PromptSet {
    pub fn new() -> Self {
        Self {
            prompts: Self::build_prompts(),
        }
    }

    pub fn by_category(&self, category: TaskCategory) -> Vec<&BenchmarkPrompt> {
        self.prompts
            .iter()
            .filter(|p| p.category == category)
            .collect()
    }

    pub fn all(&self) -> &[BenchmarkPrompt] {
        &self.prompts
    }

    pub fn count(&self) -> usize {
        self.prompts.len()
    }

    fn build_prompts() -> Vec<BenchmarkPrompt> {
        vec![
            // === Code Cleanup ===
            BenchmarkPrompt {
                id: "cleanup-dead-code".into(),
                category: TaskCategory::CodeCleanup,
                prompt: "Identify dead code in this Rust module. List functions that are never called, unused imports, and unreachable code paths.".into(),
                context: Some(SAMPLE_RUST_CODE.into()),
                expected_elements: vec![
                    "unused".into(), "dead".into(), "unreachable".into(),
                    "remove".into(), "delete".into(),
                ],
                negative_elements: vec!["add".into(), "implement".into()],
                max_tokens: 500,
                difficulty: Difficulty::Easy,
            },
            BenchmarkPrompt {
                id: "cleanup-simplify".into(),
                category: TaskCategory::CodeCleanup,
                prompt: "Simplify this code. Identify redundant logic, overly complex expressions, and opportunities for idiomatic improvements.".into(),
                context: Some(SAMPLE_COMPLEX_CODE.into()),
                expected_elements: vec![
                    "simplif".into(), "refactor".into(), "instead".into(),
                    "replace".into(),
                ],
                negative_elements: vec!["perfect".into(), "no changes".into()],
                max_tokens: 600,
                difficulty: Difficulty::Medium,
            },

            // === Documentation ===
            BenchmarkPrompt {
                id: "docs-missing".into(),
                category: TaskCategory::Documentation,
                prompt: "Identify documentation gaps. Which public functions lack docstrings? What's missing from the module documentation?".into(),
                context: Some(SAMPLE_UNDOCUMENTED.into()),
                expected_elements: vec![
                    "missing".into(), "add".into(), "document".into(),
                    "///".into(), "//!".into(),
                ],
                negative_elements: vec!["well documented".into()],
                max_tokens: 400,
                difficulty: Difficulty::Easy,
            },
            BenchmarkPrompt {
                id: "docs-improve".into(),
                category: TaskCategory::Documentation,
                prompt: "Improve these docstrings. They should include examples, error conditions, and parameter descriptions.".into(),
                context: Some(SAMPLE_POOR_DOCS.into()),
                expected_elements: vec![
                    "example".into(), "# Example".into(), "panic".into(),
                    "error".into(), "return".into(),
                ],
                negative_elements: vec![],
                max_tokens: 800,
                difficulty: Difficulty::Medium,
            },

            // === Dependencies ===
            BenchmarkPrompt {
                id: "deps-audit".into(),
                category: TaskCategory::Dependencies,
                prompt: "Audit this Cargo.toml. Identify potentially outdated dependencies, security concerns, and unused features.".into(),
                context: Some(SAMPLE_CARGO_TOML.into()),
                expected_elements: vec![
                    "version".into(), "update".into(), "feature".into(),
                    "audit".into(),
                ],
                negative_elements: vec![],
                max_tokens: 500,
                difficulty: Difficulty::Medium,
            },

            // === Testing ===
            BenchmarkPrompt {
                id: "test-coverage".into(),
                category: TaskCategory::Testing,
                prompt: "Analyze test coverage gaps. What edge cases are missing? What error paths are untested?".into(),
                context: Some(SAMPLE_TESTS.into()),
                expected_elements: vec![
                    "edge case".into(), "error".into(), "missing".into(),
                    "test".into(), "assert".into(),
                ],
                negative_elements: vec!["complete coverage".into()],
                max_tokens: 600,
                difficulty: Difficulty::Medium,
            },
            BenchmarkPrompt {
                id: "test-quality".into(),
                category: TaskCategory::Testing,
                prompt: "Evaluate test quality. Are tests isolated? Do they test behavior or implementation? Are assertions meaningful?".into(),
                context: Some(SAMPLE_TESTS.into()),
                expected_elements: vec![
                    "isolat".into(), "behavior".into(), "assert".into(),
                    "mock".into(),
                ],
                negative_elements: vec![],
                max_tokens: 500,
                difficulty: Difficulty::Hard,
            },

            // === Security ===
            BenchmarkPrompt {
                id: "security-scan".into(),
                category: TaskCategory::Security,
                prompt: "Scan for security issues. Check for: SQL injection, command injection, path traversal, hardcoded secrets, unsafe operations.".into(),
                context: Some(SAMPLE_VULNERABLE.into()),
                expected_elements: vec![
                    "injection".into(), "unsafe".into(), "sanitize".into(),
                    "validate".into(), "escape".into(),
                ],
                negative_elements: vec!["secure".into(), "no issues".into()],
                max_tokens: 700,
                difficulty: Difficulty::Hard,
            },

            // === Performance ===
            BenchmarkPrompt {
                id: "perf-hotspots".into(),
                category: TaskCategory::Performance,
                prompt: "Identify performance hotspots. Look for: unnecessary allocations, O(n^2) loops, blocking operations, missing caching.".into(),
                context: Some(SAMPLE_SLOW_CODE.into()),
                expected_elements: vec![
                    "O(n".into(), "allocat".into(), "clone".into(),
                    "cache".into(), "optimi".into(),
                ],
                negative_elements: vec![],
                max_tokens: 600,
                difficulty: Difficulty::Hard,
            },

            // === Structure ===
            BenchmarkPrompt {
                id: "struct-organize".into(),
                category: TaskCategory::Structure,
                prompt: "Suggest structural improvements. How should this code be organized into modules? What abstractions are missing?".into(),
                context: Some(SAMPLE_MESSY_CODE.into()),
                expected_elements: vec![
                    "module".into(), "struct".into(), "trait".into(),
                    "separate".into(), "extract".into(),
                ],
                negative_elements: vec![],
                max_tokens: 600,
                difficulty: Difficulty::Medium,
            },

            // === Git Hygiene ===
            BenchmarkPrompt {
                id: "git-commits".into(),
                category: TaskCategory::GitHygiene,
                prompt: "Evaluate these commit messages. Do they follow conventional commits? Are they descriptive? What should be improved?".into(),
                context: Some(SAMPLE_COMMITS.into()),
                expected_elements: vec![
                    "conventional".into(), "descriptive".into(), "feat:".into(),
                    "fix:".into(), "why".into(),
                ],
                negative_elements: vec![],
                max_tokens: 400,
                difficulty: Difficulty::Easy,
            },
        ]
    }
}

impl Default for PromptSet {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════════
// SCORING SYSTEM
// ═══════════════════════════════════════════════════════════════

/// Result of evaluating a single response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseScore {
    pub prompt_id: String,
    pub model: String,
    pub relevance: f64,    // 0-1: contains expected elements
    pub precision: f64,    // 0-1: avoids negative elements
    pub completeness: f64, // 0-1: covers all aspects
    pub efficiency: f64,   // 0-1: token efficiency
    pub latency_ms: u64,
    pub tokens_used: u32,
    pub raw_score: f64,
    pub weighted_score: f64,
}

impl ResponseScore {
    pub fn compute(
        prompt: &BenchmarkPrompt,
        model: &str,
        response: &str,
        latency: Duration,
        tokens: u32,
    ) -> Self {
        let response_lower = response.to_lowercase();

        // Relevance: how many expected elements are present
        let expected_hits = prompt
            .expected_elements
            .iter()
            .filter(|e| response_lower.contains(&e.to_lowercase()))
            .count();
        let relevance = if prompt.expected_elements.is_empty() {
            1.0
        } else {
            expected_hits as f64 / prompt.expected_elements.len() as f64
        };

        // Precision: avoids negative elements
        let negative_hits = prompt
            .negative_elements
            .iter()
            .filter(|e| response_lower.contains(&e.to_lowercase()))
            .count();
        let precision = if prompt.negative_elements.is_empty() {
            1.0
        } else {
            1.0 - (negative_hits as f64 / prompt.negative_elements.len() as f64)
        };

        // Completeness: response length relative to max_tokens
        let length_ratio = tokens as f64 / prompt.max_tokens as f64;
        let completeness = if length_ratio < 0.3 {
            length_ratio / 0.3 // Penalize very short responses
        } else if length_ratio > 0.9 {
            0.9 + 0.1 * (1.0 - (length_ratio - 0.9) / 0.1).max(0.0) // Slight penalty for hitting limit
        } else {
            1.0
        };

        // Efficiency: reward concise, relevant responses
        let efficiency = if tokens == 0 {
            0.0
        } else {
            (relevance * 1000.0 / tokens as f64).min(1.0)
        };

        // Raw score: weighted combination
        let raw_score = relevance * 0.4 + precision * 0.3 + completeness * 0.2 + efficiency * 0.1;

        // Apply difficulty and category weights
        let weighted_score = raw_score * prompt.difficulty.multiplier() * prompt.category.weight();

        Self {
            prompt_id: prompt.id.clone(),
            model: model.to_string(),
            relevance,
            precision,
            completeness,
            efficiency,
            latency_ms: latency.as_millis() as u64,
            tokens_used: tokens,
            raw_score,
            weighted_score,
        }
    }
}

/// Aggregate scores for a model across all prompts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelProfile {
    pub model: String,
    pub scores: Vec<ResponseScore>,
    pub category_scores: HashMap<TaskCategory, f64>,
    pub total_score: f64,
    pub avg_latency_ms: u64,
    pub total_tokens: u32,
    pub cost_estimate: f64,
}

impl ModelProfile {
    pub fn from_scores(model: &str, scores: Vec<ResponseScore>) -> Self {
        let total_score: f64 = scores.iter().map(|s| s.weighted_score).sum();
        let avg_latency_ms = if scores.is_empty() {
            0
        } else {
            scores.iter().map(|s| s.latency_ms).sum::<u64>() / scores.len() as u64
        };
        let total_tokens: u32 = scores.iter().map(|s| s.tokens_used).sum();

        // Aggregate by category
        let mut category_totals: HashMap<TaskCategory, (f64, usize)> = HashMap::new();
        for score in &scores {
            // Find category from prompt_id (this is a simplification)
            for cat in TaskCategory::all() {
                if score
                    .prompt_id
                    .starts_with(&cat.name().to_lowercase().replace(' ', "-")[..4])
                {
                    let entry = category_totals.entry(*cat).or_insert((0.0, 0));
                    entry.0 += score.weighted_score;
                    entry.1 += 1;
                    break;
                }
            }
        }

        let category_scores: HashMap<TaskCategory, f64> = category_totals
            .into_iter()
            .map(|(cat, (total, count))| (cat, if count > 0 { total / count as f64 } else { 0.0 }))
            .collect();

        // Rough cost estimate (assuming average pricing)
        let cost_estimate = total_tokens as f64 * 0.000002; // ~$2/M tokens average

        Self {
            model: model.to_string(),
            scores,
            category_scores,
            total_score,
            avg_latency_ms,
            total_tokens,
            cost_estimate,
        }
    }

    pub fn grade(&self) -> &'static str {
        let avg = self.total_score / self.scores.len().max(1) as f64;
        match avg {
            x if x >= 2.5 => "A+",
            x if x >= 2.0 => "A",
            x if x >= 1.7 => "B+",
            x if x >= 1.4 => "B",
            x if x >= 1.1 => "C+",
            x if x >= 0.8 => "C",
            x if x >= 0.5 => "D",
            _ => "F",
        }
    }

    pub fn render_report(&self) -> String {
        let mut report = String::new();
        report.push_str(&format!("═══ {} ═══\n", self.model));
        report.push_str(&format!(
            "Grade: {} (score: {:.2})\n",
            self.grade(),
            self.total_score
        ));
        report.push_str(&format!(
            "Avg latency: {}ms | Tokens: {} | Est. cost: ${:.4}\n\n",
            self.avg_latency_ms, self.total_tokens, self.cost_estimate
        ));

        report.push_str("Category Scores:\n");
        for cat in TaskCategory::all() {
            if let Some(score) = self.category_scores.get(cat) {
                let bar = "█".repeat((score * 10.0) as usize);
                report.push_str(&format!("  {:12} [{:10}] {:.2}\n", cat.name(), bar, score));
            }
        }

        report.push_str("\nDetailed Scores:\n");
        for score in &self.scores {
            report.push_str(&format!(
                "  {} | rel:{:.2} prec:{:.2} comp:{:.2} eff:{:.2} → {:.2}\n",
                score.prompt_id,
                score.relevance,
                score.precision,
                score.completeness,
                score.efficiency,
                score.weighted_score
            ));
        }

        report
    }
}

// ═══════════════════════════════════════════════════════════════
// BENCHMARK RUNNER
// ═══════════════════════════════════════════════════════════════

/// Configuration for running benchmarks
#[derive(Debug, Clone)]
pub struct BenchmarkConfig {
    pub categories: Vec<TaskCategory>,
    pub max_concurrent: usize,
    pub timeout: Duration,
    pub free_only: bool,
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        Self {
            categories: TaskCategory::all().to_vec(),
            max_concurrent: 3,
            timeout: Duration::from_secs(60),
            free_only: true,
        }
    }
}

/// Result of a complete benchmark run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkResult {
    pub timestamp: String,
    pub profiles: Vec<ModelProfile>,
    pub winner: String,
    pub summary: String,
}

impl BenchmarkResult {
    pub fn new(profiles: Vec<ModelProfile>) -> Self {
        let winner = profiles
            .iter()
            .max_by(|a, b| a.total_score.partial_cmp(&b.total_score).unwrap())
            .map(|p| p.model.clone())
            .unwrap_or_default();

        let summary = Self::build_summary(&profiles, &winner);

        Self {
            timestamp: chrono::Utc::now().to_rfc3339(),
            profiles,
            winner,
            summary,
        }
    }

    fn build_summary(profiles: &[ModelProfile], winner: &str) -> String {
        let mut s = String::new();
        s.push_str("╔══════════════════════════════════════════════════════════════╗\n");
        s.push_str("║           HOUSEKEEPING BENCHMARK RESULTS                     ║\n");
        s.push_str("╠══════════════════════════════════════════════════════════════╣\n");

        for profile in profiles {
            let crown = if profile.model == winner {
                "👑"
            } else {
                "  "
            };
            s.push_str(&format!(
                "║ {} {:30} {:4} score:{:6.2} ║\n",
                crown,
                profile.model,
                profile.grade(),
                profile.total_score
            ));
        }

        s.push_str("╚══════════════════════════════════════════════════════════════╝\n");
        s
    }

    pub fn render_full_report(&self) -> String {
        let mut report = self.summary.clone();
        report.push('\n');
        for profile in &self.profiles {
            report.push_str(&profile.render_report());
            report.push('\n');
        }
        report
    }
}

// ═══════════════════════════════════════════════════════════════
// SAMPLE CODE FOR PROMPTS
// ═══════════════════════════════════════════════════════════════

const SAMPLE_RUST_CODE: &str = r#"
use std::collections::HashMap;
use std::io::{Read, Write}; // Write is never used
use serde::{Serialize, Deserialize};

pub fn process_data(data: &str) -> String {
    data.to_uppercase()
}

fn unused_helper() -> i32 {
    42
}

pub fn main_logic(input: Vec<String>) -> Vec<String> {
    let mut results = Vec::new();
    for item in input {
        results.push(process_data(&item));
    }
    results
}

fn another_unused() {
    println!("This is never called");
}
"#;

const SAMPLE_COMPLEX_CODE: &str = r#"
fn complex_logic(x: i32, y: i32) -> i32 {
    if x > 0 {
        if y > 0 {
            if x > y {
                return x - y;
            } else {
                return y - x;
            }
        } else {
            return x + (-y);
        }
    } else {
        if y > 0 {
            return (-x) + y;
        } else {
            return (-x) + (-y);
        }
    }
}

fn redundant_checks(s: &str) -> bool {
    if s.is_empty() == true {
        return false;
    }
    if s.len() > 0 {
        return true;
    }
    false
}
"#;

const SAMPLE_UNDOCUMENTED: &str = r#"
pub struct Config {
    pub timeout: u64,
    pub retries: u32,
    pub endpoint: String,
}

impl Config {
    pub fn new() -> Self {
        Self {
            timeout: 30,
            retries: 3,
            endpoint: "http://localhost".into(),
        }
    }

    pub fn with_timeout(mut self, t: u64) -> Self {
        self.timeout = t;
        self
    }
}

pub fn connect(config: &Config) -> Result<Connection, Error> {
    // implementation
}

pub fn disconnect(conn: Connection) {
    // implementation
}
"#;

const SAMPLE_POOR_DOCS: &str = r#"
/// Does stuff
pub fn process(input: &str) -> Result<Output, Error> {
    // ...
}

/// Helper
fn validate(data: &[u8]) -> bool {
    // ...
}

/// Config struct
pub struct Config {
    /// The value
    pub value: i32,
}
"#;

const SAMPLE_CARGO_TOML: &str = r#"
[package]
name = "myapp"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = { version = "1.0", features = ["derive"] }
tokio = { version = "1.0", features = ["full"] }
reqwest = { version = "0.11", features = ["json", "cookies", "gzip", "brotli", "deflate", "multipart", "stream"] }
chrono = "0.4"
log = "0.4"
env_logger = "0.9"
rand = "0.8"
base64 = "0.13"
"#;

const SAMPLE_TESTS: &str = r#"
#[test]
fn test_add() {
    assert_eq!(add(2, 2), 4);
}

#[test]
fn test_subtract() {
    assert_eq!(subtract(5, 3), 2);
}

fn add(a: i32, b: i32) -> i32 { a + b }
fn subtract(a: i32, b: i32) -> i32 { a - b }
fn divide(a: i32, b: i32) -> Option<i32> {
    if b == 0 { None } else { Some(a / b) }
}
fn multiply(a: i32, b: i32) -> i32 { a * b }
"#;

const SAMPLE_VULNERABLE: &str = r#"
fn execute_query(user_input: &str) -> Result<Data> {
    let query = format!("SELECT * FROM users WHERE name = '{}'", user_input);
    db.execute(&query)
}

fn run_command(cmd: &str) -> String {
    std::process::Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .output()
        .unwrap()
        .stdout
}

fn read_file(path: &str) -> String {
    std::fs::read_to_string(path).unwrap()
}

const API_KEY: &str = "sk-1234567890abcdef";
"#;

const SAMPLE_SLOW_CODE: &str = r#"
fn find_duplicates(items: &[String]) -> Vec<String> {
    let mut duplicates = Vec::new();
    for i in 0..items.len() {
        for j in (i+1)..items.len() {
            if items[i] == items[j] && !duplicates.contains(&items[i]) {
                duplicates.push(items[i].clone());
            }
        }
    }
    duplicates
}

fn process_all(data: &[Data]) -> Vec<Result> {
    let mut results = Vec::new();
    for item in data {
        let cloned = item.clone();
        let processed = expensive_operation(cloned);
        results.push(processed);
    }
    results
}
"#;

const SAMPLE_MESSY_CODE: &str = r#"
// Everything in one file

struct User { name: String, email: String, age: u32 }
struct Product { id: u64, name: String, price: f64 }
struct Order { user_id: u64, products: Vec<u64>, total: f64 }

fn create_user(name: &str, email: &str, age: u32) -> User {
    User { name: name.into(), email: email.into(), age }
}

fn validate_email(email: &str) -> bool { email.contains('@') }
fn validate_age(age: u32) -> bool { age >= 18 }

fn create_product(id: u64, name: &str, price: f64) -> Product {
    Product { id, name: name.into(), price }
}

fn calculate_total(products: &[Product]) -> f64 {
    products.iter().map(|p| p.price).sum()
}

fn create_order(user: &User, products: Vec<Product>) -> Order {
    let total = calculate_total(&products);
    Order { user_id: 0, products: products.iter().map(|p| p.id).collect(), total }
}

fn send_email(to: &str, subject: &str, body: &str) { /* ... */ }
fn log_event(event: &str) { println!("{}", event); }
"#;

const SAMPLE_COMMITS: &str = r#"
commit abc123: fixed stuff
commit def456: wip
commit ghi789: updated code
commit jkl012: bug fix
commit mno345: changes
commit pqr678: refactored some things
commit stu901: final fix (hopefully)
"#;

// ═══════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════
// BENCHMARK RUNNER - Execute benchmarks against LLM APIs
// ═══════════════════════════════════════════════════════════════

use std::path::Path;

/// Runner for executing benchmarks against a model
pub struct BenchmarkRunner<'a> {
    api_key: &'a str,
    model: &'a str,
    work_dir: &'a Path,
    config: BenchmarkConfig,
}

impl<'a> BenchmarkRunner<'a> {
    pub fn new(api_key: &'a str, model: &'a str, work_dir: &'a Path) -> Self {
        Self {
            api_key,
            model,
            work_dir,
            config: BenchmarkConfig::default(),
        }
    }

    pub fn with_config(mut self, config: BenchmarkConfig) -> Self {
        self.config = config;
        self
    }

    /// Run the full benchmark suite and return a profile
    pub async fn run_full_suite(&mut self) -> anyhow::Result<ModelProfileWithMeta> {
        let prompts = PromptSet::default();
        let mut scores = Vec::new();

        println!(
            "Running {} prompts across {} categories...",
            prompts.prompts.len(),
            self.config.categories.len()
        );

        for prompt in &prompts.prompts {
            if !self.config.categories.contains(&prompt.category) {
                continue;
            }

            print!("  {} ({:?})... ", prompt.id, prompt.category);

            let start = std::time::Instant::now();
            match self.run_single_prompt(prompt).await {
                Ok(response) => {
                    let elapsed = start.elapsed();
                    let tokens = estimate_tokens(&response);
                    let score =
                        ResponseScore::compute(prompt, self.model, &response, elapsed, tokens);
                    println!("score: {:.2}", score.weighted_score);
                    scores.push(score);
                }
                Err(e) => {
                    println!("error: {}", e);
                }
            }
        }

        let profile = ModelProfile::from_scores(self.model, scores);
        Ok(ModelProfileWithMeta {
            model: profile.model,
            scores: profile.scores,
            category_scores: profile.category_scores,
            total_score: profile.total_score,
            avg_latency_ms: profile.avg_latency_ms,
            total_tokens: profile.total_tokens,
            cost_estimate: profile.cost_estimate,
            total_prompt_tokens: 0, // TODO: track separately
            total_completion_tokens: 0,
            total_time_secs: 0.0,
        })
    }

    async fn run_single_prompt(&self, prompt: &BenchmarkPrompt) -> anyhow::Result<String> {
        use crate::client;

        let full_prompt = if let Some(ctx) = &prompt.context {
            format!(
                "You are a code assistant helping with housekeeping tasks. \
                     Be concise and specific.\n\n{}\n\nContext:\n{}",
                prompt.prompt, ctx
            )
        } else {
            format!(
                "You are a code assistant helping with housekeeping tasks. \
                     Be concise and specific.\n\n{}",
                prompt.prompt
            )
        };

        let response = client::chat_completion_simple(
            self.api_key,
            self.model,
            &full_prompt,
            prompt.max_tokens,
        )
        .await?;

        Ok(response)
    }
}

/// Extended profile with metadata about the run
pub struct ModelProfileWithMeta {
    pub model: String,
    pub scores: Vec<ResponseScore>,
    pub category_scores: HashMap<TaskCategory, f64>,
    pub total_score: f64,
    pub avg_latency_ms: u64,
    pub total_tokens: u32,
    pub cost_estimate: f64,
    pub total_prompt_tokens: u32,
    pub total_completion_tokens: u32,
    pub total_time_secs: f64,
}

impl ModelProfileWithMeta {
    pub fn grade(&self) -> &'static str {
        let avg = self.total_score / self.scores.len().max(1) as f64;
        match avg {
            x if x >= 2.5 => "A+",
            x if x >= 2.0 => "A",
            x if x >= 1.7 => "B+",
            x if x >= 1.4 => "B",
            x if x >= 1.1 => "C+",
            x if x >= 0.8 => "C",
            x if x >= 0.5 => "D",
            _ => "F",
        }
    }
}

/// Estimate token count from text (rough approximation)
fn estimate_tokens(text: &str) -> u32 {
    // Rough estimate: ~4 chars per token for English
    (text.len() as f64 / 4.0).ceil() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_set_has_all_categories() {
        let set = PromptSet::new();
        for cat in TaskCategory::all() {
            assert!(
                !set.by_category(*cat).is_empty(),
                "Missing prompts for {:?}",
                cat
            );
        }
    }

    #[test]
    fn test_prompt_set_count() {
        let set = PromptSet::new();
        assert!(set.count() >= 10, "Should have at least 10 prompts");
    }

    #[test]
    fn test_response_score_relevance() {
        let prompt = BenchmarkPrompt {
            id: "test".into(),
            category: TaskCategory::CodeCleanup,
            prompt: "test".into(),
            context: None,
            expected_elements: vec!["foo".into(), "bar".into(), "baz".into()],
            negative_elements: vec![],
            max_tokens: 100,
            difficulty: Difficulty::Easy,
        };

        let score = ResponseScore::compute(
            &prompt,
            "test-model",
            "foo bar",
            Duration::from_millis(100),
            10,
        );
        assert!(
            (score.relevance - 0.666).abs() < 0.01,
            "Expected ~0.67 relevance"
        );
    }

    #[test]
    fn test_response_score_precision() {
        let prompt = BenchmarkPrompt {
            id: "test".into(),
            category: TaskCategory::CodeCleanup,
            prompt: "test".into(),
            context: None,
            expected_elements: vec![],
            negative_elements: vec!["bad".into(), "wrong".into()],
            max_tokens: 100,
            difficulty: Difficulty::Easy,
        };

        let score = ResponseScore::compute(
            &prompt,
            "test-model",
            "this is bad",
            Duration::from_millis(100),
            10,
        );
        assert!(
            (score.precision - 0.5).abs() < 0.01,
            "Expected 0.5 precision"
        );
    }

    #[test]
    fn test_difficulty_multipliers() {
        assert_eq!(Difficulty::Easy.multiplier(), 1.0);
        assert_eq!(Difficulty::Medium.multiplier(), 1.5);
        assert_eq!(Difficulty::Hard.multiplier(), 2.0);
    }

    #[test]
    fn test_category_weights() {
        assert!(TaskCategory::Security.weight() > TaskCategory::Documentation.weight());
        assert!(TaskCategory::Testing.weight() > TaskCategory::GitHygiene.weight());
    }

    #[test]
    fn test_model_profile_grade() {
        let scores = vec![ResponseScore {
            prompt_id: "test".into(),
            model: "model".into(),
            relevance: 1.0,
            precision: 1.0,
            completeness: 1.0,
            efficiency: 1.0,
            latency_ms: 100,
            tokens_used: 50,
            raw_score: 1.0,
            weighted_score: 2.5,
        }];
        let profile = ModelProfile::from_scores("model", scores);
        assert_eq!(profile.grade(), "A+");
    }

    #[test]
    fn test_benchmark_result_winner() {
        let profiles = vec![
            ModelProfile::from_scores(
                "model-a",
                vec![ResponseScore {
                    prompt_id: "t1".into(),
                    model: "model-a".into(),
                    relevance: 0.5,
                    precision: 0.5,
                    completeness: 0.5,
                    efficiency: 0.5,
                    latency_ms: 100,
                    tokens_used: 50,
                    raw_score: 0.5,
                    weighted_score: 1.0,
                }],
            ),
            ModelProfile::from_scores(
                "model-b",
                vec![ResponseScore {
                    prompt_id: "t1".into(),
                    model: "model-b".into(),
                    relevance: 0.9,
                    precision: 0.9,
                    completeness: 0.9,
                    efficiency: 0.9,
                    latency_ms: 100,
                    tokens_used: 50,
                    raw_score: 0.9,
                    weighted_score: 2.0,
                }],
            ),
        ];
        let result = BenchmarkResult::new(profiles);
        assert_eq!(result.winner, "model-b");
    }
}
