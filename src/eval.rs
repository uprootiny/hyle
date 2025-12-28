//! Model output evaluation and quality tracking
//!
//! Provides:
//! - Response quality scoring
//! - Model performance tracking
//! - Automatic model switching on degradation

#![allow(dead_code)] // Forward-looking module for self-bootstrapping

use std::collections::HashMap;
use serde::{Serialize, Deserialize};

use crate::agent::parse_tool_calls;

// ═══════════════════════════════════════════════════════════════
// QUALITY METRICS
// ═══════════════════════════════════════════════════════════════

/// Quality score for a single response (0.0 - 1.0)
#[derive(Debug, Clone, Default)]
pub struct QualityScore {
    pub coherence: f32,      // Does it make sense?
    pub completeness: f32,   // Did it answer the question?
    pub tool_validity: f32,  // Are tool calls valid?
    pub code_quality: f32,   // Does generated code look valid?
    pub relevance: f32,      // Is it on-topic?
    pub overall: f32,        // Weighted average
}

impl QualityScore {
    /// Calculate overall score from components
    pub fn calculate_overall(&mut self) {
        // Weights for different aspects
        const W_COHERENCE: f32 = 0.25;
        const W_COMPLETENESS: f32 = 0.25;
        const W_TOOL_VALIDITY: f32 = 0.20;
        const W_CODE_QUALITY: f32 = 0.15;
        const W_RELEVANCE: f32 = 0.15;

        self.overall = self.coherence * W_COHERENCE
            + self.completeness * W_COMPLETENESS
            + self.tool_validity * W_TOOL_VALIDITY
            + self.code_quality * W_CODE_QUALITY
            + self.relevance * W_RELEVANCE;
    }

    /// Is this score acceptable?
    pub fn is_acceptable(&self) -> bool {
        self.overall >= 0.5
    }

    /// Is this score good?
    pub fn is_good(&self) -> bool {
        self.overall >= 0.7
    }

    /// Format for display
    pub fn display(&self) -> String {
        format!(
            "{:.0}% (coh:{:.0} comp:{:.0} tool:{:.0} code:{:.0} rel:{:.0})",
            self.overall * 100.0,
            self.coherence * 100.0,
            self.completeness * 100.0,
            self.tool_validity * 100.0,
            self.code_quality * 100.0,
            self.relevance * 100.0,
        )
    }
}

// ═══════════════════════════════════════════════════════════════
// RESPONSE EVALUATOR
// ═══════════════════════════════════════════════════════════════

/// Evaluates model responses for quality
pub struct ResponseEvaluator {
    /// Minimum acceptable response length
    pub min_response_length: usize,
    /// Maximum repetition ratio allowed
    pub max_repetition_ratio: f32,
}

impl Default for ResponseEvaluator {
    fn default() -> Self {
        Self {
            min_response_length: 10,
            max_repetition_ratio: 0.5,
        }
    }
}

impl ResponseEvaluator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Evaluate a response given the prompt
    pub fn evaluate(&self, prompt: &str, response: &str) -> QualityScore {
        let mut score = QualityScore::default();

        score.coherence = self.score_coherence(response);
        score.completeness = self.score_completeness(prompt, response);
        score.tool_validity = self.score_tool_validity(response);
        score.code_quality = self.score_code_quality(response);
        score.relevance = self.score_relevance(prompt, response);

        score.calculate_overall();
        score
    }

    /// Score coherence (makes grammatical/logical sense)
    fn score_coherence(&self, response: &str) -> f32 {
        let mut score: f32 = 1.0;

        // Penalize very short responses
        if response.len() < self.min_response_length {
            score -= 0.5;
        }

        // Penalize empty responses
        if response.trim().is_empty() {
            return 0.0;
        }

        // Check for excessive repetition
        let repetition = self.calculate_repetition(response);
        if repetition > self.max_repetition_ratio {
            score -= 0.4;
        }

        // Check for incomplete sentences (ends mid-word)
        if response.ends_with("...") || response.ends_with("—") {
            score -= 0.1;
        }

        // Check for garbled text (high ratio of special chars)
        let special_ratio = response.chars()
            .filter(|c| !c.is_alphanumeric() && !c.is_whitespace() && !".,:;!?-'\"()[]{}".contains(*c))
            .count() as f32 / response.len().max(1) as f32;
        if special_ratio > 0.1 {
            score -= 0.3;
        }

        score.max(0.0)
    }

    /// Calculate repetition ratio (repeated n-grams)
    fn calculate_repetition(&self, text: &str) -> f32 {
        let words: Vec<&str> = text.split_whitespace().collect();
        if words.len() < 10 {
            return 0.0;
        }

        // Check for repeated 3-grams
        let mut trigrams: HashMap<String, usize> = HashMap::new();
        for window in words.windows(3) {
            let trigram = window.join(" ");
            *trigrams.entry(trigram).or_insert(0) += 1;
        }

        let repeated = trigrams.values().filter(|&&c| c > 2).count();
        repeated as f32 / trigrams.len().max(1) as f32
    }

    /// Score completeness (answered the question)
    fn score_completeness(&self, prompt: &str, response: &str) -> f32 {
        let mut score = 0.5; // Start neutral

        // Check if response mentions key terms from prompt
        let prompt_words: Vec<&str> = prompt.split_whitespace()
            .filter(|w| w.len() > 3)
            .collect();

        let response_lower = response.to_lowercase();
        let matched = prompt_words.iter()
            .filter(|w| response_lower.contains(&w.to_lowercase()))
            .count();

        let match_ratio = matched as f32 / prompt_words.len().max(1) as f32;
        score += match_ratio * 0.3;

        // Bonus for acknowledgment phrases
        let acknowledgments = ["here", "let me", "i'll", "i will", "sure", "certainly"];
        if acknowledgments.iter().any(|a| response_lower.starts_with(a)) {
            score += 0.1;
        }

        // Bonus for conclusion phrases
        let conclusions = ["done", "complete", "finished", "let me know", "hope this helps"];
        if conclusions.iter().any(|c| response_lower.contains(c)) {
            score += 0.1;
        }

        score.min(1.0)
    }

    /// Score tool validity (if tool calls present, are they valid?)
    fn score_tool_validity(&self, response: &str) -> f32 {
        // Empty response = no valid tool output
        if response.trim().is_empty() {
            return 0.0;
        }

        let calls = parse_tool_calls(response);

        if calls.is_empty() {
            // No tool calls - check if response looks like it should have them
            let should_have_tools = response.contains("```") ||
                response.to_lowercase().contains("let me read") ||
                response.to_lowercase().contains("i'll check");

            if should_have_tools {
                return 0.3; // Probably should have had tool calls
            }
            return 1.0; // No tools expected, no tools found
        }

        // Check tool call quality
        let mut valid_count = 0;
        for call in &calls {
            // Check if tool name is known
            let known_tools = ["read", "write", "bash", "glob", "grep", "edit", "search"];
            if known_tools.contains(&call.name.as_str()) {
                valid_count += 1;
            }

            // Check if args are non-empty
            if !call.args.is_null() && call.args != serde_json::json!({}) {
                valid_count += 1;
            }
        }

        let max_points = calls.len() * 2;
        valid_count as f32 / max_points as f32
    }

    /// Score code quality (if code blocks present)
    fn score_code_quality(&self, response: &str) -> f32 {
        // Empty response = no valid code
        if response.trim().is_empty() {
            return 0.0;
        }

        // Find code blocks
        let code_blocks: Vec<&str> = response
            .split("```")
            .enumerate()
            .filter(|(i, _)| i % 2 == 1) // Odd indices are inside code blocks
            .map(|(_, s)| s)
            .collect();

        if code_blocks.is_empty() {
            return 1.0; // No code, no problem
        }

        let mut total_score = 0.0;
        for block in &code_blocks {
            let mut block_score = 0.5;

            // Skip language identifier line
            let code = block.lines().skip(1).collect::<Vec<_>>().join("\n");

            // Check for balanced braces/brackets
            let open_braces = code.matches('{').count();
            let close_braces = code.matches('}').count();
            let open_parens = code.matches('(').count();
            let close_parens = code.matches(')').count();

            if open_braces == close_braces && open_parens == close_parens {
                block_score += 0.3;
            }

            // Check for common syntax patterns
            if code.contains("fn ") || code.contains("def ") ||
               code.contains("function ") || code.contains("class ") {
                block_score += 0.2;
            }

            total_score += block_score;
        }

        (total_score / code_blocks.len() as f32).min(1.0)
    }

    /// Score relevance (is response on-topic)
    fn score_relevance(&self, prompt: &str, response: &str) -> f32 {
        // Empty response = not relevant
        if response.trim().is_empty() {
            return 0.0;
        }

        let prompt_lower = prompt.to_lowercase();
        let response_lower = response.to_lowercase();

        // Extract key terms, stripping punctuation
        let key_terms: Vec<String> = prompt_lower
            .split_whitespace()
            .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_string())
            .filter(|w| w.len() > 3) // Lowered from 4 to catch more terms
            .take(10)
            .collect();

        if key_terms.is_empty() {
            return 0.7; // Can't assess without key terms
        }

        let matched = key_terms.iter()
            .filter(|t| response_lower.contains(t.as_str()))
            .count();

        0.4 + (matched as f32 / key_terms.len() as f32) * 0.6
    }
}

// ═══════════════════════════════════════════════════════════════
// MODEL PERFORMANCE TRACKER
// ═══════════════════════════════════════════════════════════════

/// Tracks performance of a single model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelStats {
    pub model_id: String,
    pub total_requests: usize,
    pub successful_requests: usize,
    pub failed_requests: usize,
    pub total_tokens: u64,
    pub average_quality: f32,
    pub recent_scores: Vec<f32>, // Last N scores
    pub last_used: Option<i64>,  // Unix timestamp
    pub consecutive_failures: usize,
}

impl ModelStats {
    pub fn new(model_id: &str) -> Self {
        Self {
            model_id: model_id.to_string(),
            total_requests: 0,
            successful_requests: 0,
            failed_requests: 0,
            total_tokens: 0,
            average_quality: 0.5,
            recent_scores: Vec::new(),
            last_used: None,
            consecutive_failures: 0,
        }
    }

    /// Record a successful response
    pub fn record_success(&mut self, quality: f32, tokens: u64) {
        self.total_requests += 1;
        self.successful_requests += 1;
        self.total_tokens += tokens;
        self.consecutive_failures = 0;
        self.last_used = Some(chrono::Utc::now().timestamp());

        // Update recent scores (keep last 20)
        self.recent_scores.push(quality);
        if self.recent_scores.len() > 20 {
            self.recent_scores.remove(0);
        }

        // Update rolling average
        self.average_quality = self.recent_scores.iter().sum::<f32>()
            / self.recent_scores.len() as f32;
    }

    /// Record a failed response
    pub fn record_failure(&mut self) {
        self.total_requests += 1;
        self.failed_requests += 1;
        self.consecutive_failures += 1;
        self.last_used = Some(chrono::Utc::now().timestamp());

        // Add low score for failure
        self.recent_scores.push(0.0);
        if self.recent_scores.len() > 20 {
            self.recent_scores.remove(0);
        }

        self.average_quality = self.recent_scores.iter().sum::<f32>()
            / self.recent_scores.len() as f32;
    }

    /// Should we switch away from this model?
    pub fn should_switch(&self) -> bool {
        // Switch if: 3+ consecutive failures, or average quality < 0.4
        self.consecutive_failures >= 3 || self.average_quality < 0.4
    }

    /// Success rate
    pub fn success_rate(&self) -> f32 {
        if self.total_requests == 0 {
            return 1.0;
        }
        self.successful_requests as f32 / self.total_requests as f32
    }
}

/// Tracks performance across multiple models
pub struct ModelTracker {
    stats: HashMap<String, ModelStats>,
    current_model: Option<String>,
    evaluator: ResponseEvaluator,
}

impl Default for ModelTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl ModelTracker {
    pub fn new() -> Self {
        Self {
            stats: HashMap::new(),
            current_model: None,
            evaluator: ResponseEvaluator::new(),
        }
    }

    /// Set current model
    pub fn set_model(&mut self, model_id: &str) {
        self.current_model = Some(model_id.to_string());
        self.stats.entry(model_id.to_string())
            .or_insert_with(|| ModelStats::new(model_id));
    }

    /// Evaluate and record a response
    pub fn record_response(&mut self, prompt: &str, response: &str, tokens: u64) -> QualityScore {
        let score = self.evaluator.evaluate(prompt, response);

        if let Some(model_id) = &self.current_model {
            if let Some(stats) = self.stats.get_mut(model_id) {
                if score.is_acceptable() {
                    stats.record_success(score.overall, tokens);
                } else {
                    stats.record_failure();
                }
            }
        }

        score
    }

    /// Record a failure (e.g., API error)
    pub fn record_failure(&mut self) {
        if let Some(model_id) = &self.current_model {
            if let Some(stats) = self.stats.get_mut(model_id) {
                stats.record_failure();
            }
        }
    }

    /// Should we switch from current model?
    pub fn should_switch(&self) -> bool {
        if let Some(model_id) = &self.current_model {
            if let Some(stats) = self.stats.get(model_id) {
                return stats.should_switch();
            }
        }
        false
    }

    /// Get current model stats
    pub fn current_stats(&self) -> Option<&ModelStats> {
        self.current_model.as_ref()
            .and_then(|id| self.stats.get(id))
    }

    /// Get best performing model from tracked models
    pub fn best_model(&self) -> Option<&str> {
        self.stats.iter()
            .filter(|(_, s)| s.total_requests >= 3) // Need enough data
            .max_by(|(_, a), (_, b)| {
                a.average_quality.partial_cmp(&b.average_quality).unwrap()
            })
            .map(|(id, _)| id.as_str())
    }

    /// Get stats summary for display
    pub fn summary(&self) -> String {
        let mut lines = Vec::new();
        for (id, stats) in &self.stats {
            let short_id = id.split('/').next_back().unwrap_or(id);
            lines.push(format!(
                "{}: {:.0}% quality, {:.0}% success, {} reqs",
                short_id,
                stats.average_quality * 100.0,
                stats.success_rate() * 100.0,
                stats.total_requests
            ));
        }
        lines.join("\n")
    }
}

// ═══════════════════════════════════════════════════════════════
// MODEL SWITCHER
// ═══════════════════════════════════════════════════════════════

/// Automatic model switching strategy
pub struct ModelSwitcher {
    /// Available models to switch between
    available_models: Vec<String>,
    /// Current model index
    current_index: usize,
    /// Tracker for performance
    tracker: ModelTracker,
    /// Minimum requests before considering switch
    min_requests_before_switch: usize,
}

impl ModelSwitcher {
    pub fn new(models: Vec<String>) -> Self {
        let mut switcher = Self {
            available_models: models.clone(),
            current_index: 0,
            tracker: ModelTracker::new(),
            min_requests_before_switch: 3,
        };

        if let Some(model) = models.first() {
            switcher.tracker.set_model(model);
        }

        switcher
    }

    /// Get current model
    pub fn current_model(&self) -> Option<&str> {
        self.available_models.get(self.current_index).map(|s| s.as_str())
    }

    /// Record response and possibly trigger switch
    pub fn record_and_maybe_switch(&mut self, prompt: &str, response: &str, tokens: u64) -> (QualityScore, bool) {
        let score = self.tracker.record_response(prompt, response, tokens);

        let switched = if self.should_switch() {
            self.switch_to_next();
            true
        } else {
            false
        };

        (score, switched)
    }

    /// Record failure and possibly trigger switch
    pub fn record_failure_and_maybe_switch(&mut self) -> bool {
        self.tracker.record_failure();

        if self.should_switch() {
            self.switch_to_next();
            true
        } else {
            false
        }
    }

    /// Check if we should switch
    fn should_switch(&self) -> bool {
        if let Some(stats) = self.tracker.current_stats() {
            stats.total_requests >= self.min_requests_before_switch && stats.should_switch()
        } else {
            false
        }
    }

    /// Switch to next model in rotation
    fn switch_to_next(&mut self) {
        if self.available_models.len() <= 1 {
            return;
        }

        self.current_index = (self.current_index + 1) % self.available_models.len();

        if let Some(model) = self.available_models.get(self.current_index) {
            self.tracker.set_model(model);
        }
    }

    /// Switch to best known model
    pub fn switch_to_best(&mut self) {
        let best = self.tracker.best_model().map(|s| s.to_string());
        if let Some(best) = best {
            if let Some(idx) = self.available_models.iter().position(|m| m == &best) {
                self.current_index = idx;
                self.tracker.set_model(&best);
            }
        }
    }

    /// Get performance summary
    pub fn summary(&self) -> String {
        format!(
            "Current: {}\n{}",
            self.current_model().unwrap_or("none"),
            self.tracker.summary()
        )
    }
}

// ═══════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quality_score_overall() {
        let mut score = QualityScore {
            coherence: 0.8,
            completeness: 0.7,
            tool_validity: 0.9,
            code_quality: 0.6,
            relevance: 0.8,
            overall: 0.0,
        };
        score.calculate_overall();

        assert!(score.overall > 0.7);
        assert!(score.is_acceptable());
        assert!(score.is_good());
    }

    #[test]
    fn test_quality_score_low() {
        let mut score = QualityScore {
            coherence: 0.3,
            completeness: 0.2,
            tool_validity: 0.4,
            code_quality: 0.3,
            relevance: 0.2,
            overall: 0.0,
        };
        score.calculate_overall();

        assert!(score.overall < 0.5);
        assert!(!score.is_acceptable());
    }

    #[test]
    fn test_evaluator_empty_response() {
        let eval = ResponseEvaluator::new();
        let score = eval.evaluate("What is Rust?", "");

        assert_eq!(score.coherence, 0.0);
    }

    #[test]
    fn test_evaluator_good_response() {
        let eval = ResponseEvaluator::new();
        let score = eval.evaluate(
            "What is Rust?",
            "Rust is a systems programming language focused on safety and performance. \
             It provides memory safety without garbage collection."
        );

        assert!(score.coherence > 0.7);
        assert!(score.relevance > 0.5);
    }

    #[test]
    fn test_evaluator_repetitive_response() {
        let eval = ResponseEvaluator::new();
        let response = "test test test test test test test test test test \
                       test test test test test test test test test test";
        let score = eval.evaluate("What is Rust?", response);

        assert!(score.coherence < 0.7); // Penalized for repetition
    }

    #[test]
    fn test_evaluator_with_tool_calls() {
        let eval = ResponseEvaluator::new();
        let response = r#"Let me read the file.

```json
{"tool": "read", "args": {"path": "src/main.rs"}}
```"#;

        let score = eval.evaluate("Read the main.rs file", response);
        assert!(score.tool_validity > 0.5);
    }

    #[test]
    fn test_evaluator_with_code() {
        let eval = ResponseEvaluator::new();
        let response = r#"Here's the implementation:

```rust
fn main() {
    println!("Hello, world!");
}
```"#;

        let score = eval.evaluate("Write hello world", response);
        assert!(score.code_quality > 0.7);
    }

    #[test]
    fn test_evaluator_unbalanced_code() {
        let eval = ResponseEvaluator::new();
        let response = r#"Here's some code:

```rust
fn main() {
    println!("incomplete
```"#;

        let score = eval.evaluate("Write code", response);
        assert!(score.code_quality < 0.8); // Penalized for unbalanced
    }

    #[test]
    fn test_model_stats_success() {
        let mut stats = ModelStats::new("test-model");

        stats.record_success(0.8, 100);
        stats.record_success(0.7, 150);

        assert_eq!(stats.total_requests, 2);
        assert_eq!(stats.successful_requests, 2);
        assert_eq!(stats.consecutive_failures, 0);
        assert!(stats.average_quality > 0.7);
    }

    #[test]
    fn test_model_stats_failure() {
        let mut stats = ModelStats::new("test-model");

        stats.record_failure();
        stats.record_failure();
        stats.record_failure();

        assert_eq!(stats.consecutive_failures, 3);
        assert!(stats.should_switch());
    }

    #[test]
    fn test_model_stats_mixed() {
        let mut stats = ModelStats::new("test-model");

        stats.record_success(0.8, 100);
        stats.record_failure();
        stats.record_success(0.7, 100);

        assert_eq!(stats.consecutive_failures, 0);
        assert!(!stats.should_switch());
    }

    #[test]
    fn test_model_tracker() {
        let mut tracker = ModelTracker::new();
        tracker.set_model("model-a");

        let score = tracker.record_response(
            "What is Rust?",
            "Rust is a programming language.",
            50
        );

        assert!(score.overall > 0.0);
        assert!(!tracker.should_switch());
    }

    #[test]
    fn test_model_tracker_low_quality() {
        let mut tracker = ModelTracker::new();
        tracker.set_model("bad-model");

        // Record several low-quality responses
        for _ in 0..5 {
            tracker.record_response("What?", "", 0);
        }

        assert!(tracker.should_switch());
    }

    #[test]
    fn test_model_switcher() {
        let models = vec![
            "model-a".to_string(),
            "model-b".to_string(),
            "model-c".to_string(),
        ];
        let mut switcher = ModelSwitcher::new(models);

        assert_eq!(switcher.current_model(), Some("model-a"));

        // Force failures to trigger switch
        for _ in 0..5 {
            switcher.record_failure_and_maybe_switch();
        }

        assert_eq!(switcher.current_model(), Some("model-b"));
    }

    #[test]
    fn test_model_switcher_good_responses() {
        let models = vec!["model-a".to_string(), "model-b".to_string()];
        let mut switcher = ModelSwitcher::new(models);

        for _ in 0..5 {
            let (score, switched) = switcher.record_and_maybe_switch(
                "What is Rust?",
                "Rust is a systems programming language.",
                100
            );
            assert!(score.overall > 0.5);
            assert!(!switched);
        }

        assert_eq!(switcher.current_model(), Some("model-a"));
    }

    #[test]
    fn test_quality_score_display() {
        let mut score = QualityScore {
            coherence: 0.8,
            completeness: 0.7,
            tool_validity: 0.9,
            code_quality: 0.6,
            relevance: 0.8,
            overall: 0.0,
        };
        score.calculate_overall();

        let display = score.display();
        assert!(display.contains("%"));
        assert!(display.contains("coh:"));
    }

    #[test]
    fn test_calculate_repetition() {
        let eval = ResponseEvaluator::new();

        // Low repetition
        let low_rep = eval.calculate_repetition("The quick brown fox jumps over the lazy dog repeatedly in this sentence structure.");
        assert!(low_rep < 0.3);

        // High repetition
        let high_rep = eval.calculate_repetition("the same words the same words the same words the same words the same words");
        assert!(high_rep > 0.3);
    }
}
