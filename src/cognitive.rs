//! Multi-LLM Cognitive Architecture
//!
//! Uses multiple models as specialized "cognitive processes":
//! - Executor: Main reasoning (user's chosen model)
//! - Summarizer: Compresses context (free model)
//! - Sanity: Validates progress (free model)
//!
//! Active components:
//! - Momentum: Tracks tool success rate
//! - StuckDetector: Detects repeated failures
//! - SalienceContext: Prioritizes context by relevance
//!
//! Forward-looking (not yet wired):
//! - ContextLayers: Multi-tier context management
//! - Multi-LLM coordination prompts

#![allow(dead_code)] // Forward-looking architecture

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

// ═══════════════════════════════════════════════════════════════
// CONFIGURATION
// ═══════════════════════════════════════════════════════════════

/// Free models available on OpenRouter for cognitive tasks
pub const FREE_MODELS: &[&str] = &[
    "mistralai/mistral-7b-instruct:free",
    "meta-llama/llama-3.2-3b-instruct:free",
    "google/gemma-2-9b-it:free",
    "qwen/qwen-2-7b-instruct:free",
];

/// Configuration for the cognitive system
#[derive(Debug, Clone)]
pub struct CognitiveConfig {
    pub executor_model: String,
    pub summarizer_model: String,
    pub sanity_model: String,
    pub sanity_interval: u8,    // Check every N iterations
    pub context_budget: usize,  // Max tokens for context
    pub summary_trigger: usize, // Compress after N messages
}

impl Default for CognitiveConfig {
    fn default() -> Self {
        Self {
            executor_model: "mistralai/devstral-small:free".into(),
            summarizer_model: FREE_MODELS[0].into(),
            sanity_model: FREE_MODELS[0].into(),
            sanity_interval: 3,
            context_budget: 8000,
            summary_trigger: 6,
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// CONTEXT LAYERS
// ═══════════════════════════════════════════════════════════════

/// Tiered context representation
#[derive(Debug, Clone, Default)]
pub struct ContextLayers {
    /// Full detail - last 2 exchanges
    pub working_memory: Vec<ContextItem>,

    /// Compressed summaries of older exchanges
    pub summary_memory: Vec<Summary>,

    /// Key facts extracted across session
    pub facts: Vec<Fact>,

    /// Current task/goal
    pub current_goal: Option<String>,

    /// Progress tracking
    pub progress: Progress,
}

#[derive(Debug, Clone)]
pub struct ContextItem {
    pub role: String,
    pub content: String,
    pub tool_calls: Vec<String>,
    pub tool_results: Vec<ToolOutcome>,
}

#[derive(Debug, Clone)]
pub struct Summary {
    pub iteration_range: (u32, u32),
    pub summary: String,
    pub key_actions: Vec<String>,
    pub files_touched: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fact {
    pub category: FactCategory,
    pub content: String,
    pub confidence: f32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FactCategory {
    UserIntent, // What user wants
    FileState,  // State of a file
    Error,      // An error that occurred
    Decision,   // A decision that was made
    Constraint, // A constraint or requirement
}

#[derive(Debug, Clone, Default)]
pub struct Progress {
    pub iteration: u32,
    pub estimated_completion: f32, // 0.0 to 1.0
    pub momentum: Momentum,
    pub stuck_detector: StuckDetector,
}

// ═══════════════════════════════════════════════════════════════
// MOMENTUM TRACKING
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
pub struct Momentum {
    window: VecDeque<ToolOutcome>,
    window_size: usize,
}

impl Default for Momentum {
    fn default() -> Self {
        Self {
            window: VecDeque::new(),
            window_size: 10,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ToolOutcome {
    pub tool_name: String,
    pub success: bool,
    pub was_useful: bool, // Did it provide useful information?
}

impl Momentum {
    pub fn record(&mut self, outcome: ToolOutcome) {
        if self.window.len() >= self.window_size {
            self.window.pop_front();
        }
        self.window.push_back(outcome);
    }

    pub fn score(&self) -> f32 {
        if self.window.is_empty() {
            return 1.0;
        }
        let successes = self.window.iter().filter(|o| o.success).count();
        successes as f32 / self.window.len() as f32
    }

    pub fn should_slow_down(&self) -> bool {
        self.score() < 0.5
    }

    pub fn should_pause(&self) -> bool {
        self.score() < 0.3
    }

    pub fn recent_failures(&self) -> usize {
        self.window.iter().rev().take_while(|o| !o.success).count()
    }
}

// ═══════════════════════════════════════════════════════════════
// STUCK DETECTION
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Default)]
pub struct StuckDetector {
    recent_actions: VecDeque<u64>, // Hashes of recent actions
    error_counts: std::collections::HashMap<String, u8>,
    no_change_count: u8,
}

impl StuckDetector {
    pub fn new() -> Self {
        Self {
            recent_actions: VecDeque::with_capacity(10),
            error_counts: std::collections::HashMap::new(),
            no_change_count: 0,
        }
    }

    pub fn record_action(&mut self, action_hash: u64) {
        if self.recent_actions.len() >= 10 {
            self.recent_actions.pop_front();
        }
        self.recent_actions.push_back(action_hash);
    }

    pub fn record_error(&mut self, error_type: &str) {
        *self.error_counts.entry(error_type.to_string()).or_default() += 1;
    }

    pub fn record_no_change(&mut self) {
        self.no_change_count += 1;
    }

    pub fn record_change(&mut self) {
        self.no_change_count = 0;
    }

    pub fn is_stuck(&self) -> bool {
        // Same action repeated 3+ times
        if self.has_repeated_action(3) {
            return true;
        }
        // Same error 3+ times
        if self.error_counts.values().any(|&c| c >= 3) {
            return true;
        }
        // No changes in 5 iterations
        if self.no_change_count >= 5 {
            return true;
        }
        false
    }

    fn has_repeated_action(&self, threshold: usize) -> bool {
        if self.recent_actions.len() < threshold {
            return false;
        }
        let last = self.recent_actions.back();
        let count = self
            .recent_actions
            .iter()
            .filter(|&h| Some(h) == last)
            .count();
        count >= threshold
    }

    pub fn clear(&mut self) {
        self.recent_actions.clear();
        self.error_counts.clear();
        self.no_change_count = 0;
    }
}

// ═══════════════════════════════════════════════════════════════
// SANITY CHECK
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
pub struct SanityResult {
    pub on_track: bool,
    pub confidence: f32,
    pub progress_estimate: f32,
    pub concerns: Vec<String>,
    pub suggestions: Vec<String>,
    pub should_pause: bool,
    pub should_abort: bool,
}

impl Default for SanityResult {
    fn default() -> Self {
        Self {
            on_track: true,
            confidence: 1.0,
            progress_estimate: 0.0,
            concerns: vec![],
            suggestions: vec![],
            should_pause: false,
            should_abort: false,
        }
    }
}

/// Triggers for running a sanity check
#[derive(Debug, Clone)]
pub enum SanityTrigger {
    Interval(u32),      // Every N iterations
    ToolFailure,        // After any tool fails
    LargeOutput(usize), // Output exceeded N chars
    ContextThreshold,   // Context approaching limit
    MomentumDrop,       // Momentum fell below threshold
    Explicit,           // User requested via /sanity
}

// ═══════════════════════════════════════════════════════════════
// LOOP DECISIONS
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
pub enum LoopDecision {
    /// Continue automatically
    Continue,

    /// Pause for user confirmation (risky operation)
    ConfirmRisk {
        operation: String,
        risk_level: ToolRisk,
    },

    /// Pause because something seems wrong
    PauseConcern { reason: String },

    /// Task appears complete
    Complete { summary: String },

    /// Agent is stuck
    Stuck {
        reason: String,
        suggestions: Vec<String>,
    },

    /// Need user input
    NeedInput { question: String },

    /// Hit iteration limit
    MaxIterations,

    /// Error state
    Error { message: String },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum ToolRisk {
    Safe,      // read, glob, grep
    Cautious,  // write, edit (reversible)
    Confirm,   // delete, move, shell commands
    Dangerous, // sudo, force flags, etc.
}

impl ToolRisk {
    pub fn from_tool_call(tool: &str, args: &str) -> Self {
        match tool {
            "read" | "glob" | "grep" => ToolRisk::Safe,
            "write" | "edit" => ToolRisk::Cautious,
            "bash" | "shell" => {
                // Analyze command for danger signals
                let lower = args.to_lowercase();
                if lower.contains("sudo") || lower.contains("--force") || lower.contains("-rf") {
                    ToolRisk::Dangerous
                } else if lower.contains("rm ") || lower.contains("mv ") || lower.contains("chmod")
                {
                    ToolRisk::Confirm
                } else {
                    ToolRisk::Cautious
                }
            }
            _ => ToolRisk::Cautious,
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// SUMMARIZER PROMPTS
// ═══════════════════════════════════════════════════════════════

pub fn summarizer_prompt(exchange: &str) -> String {
    format!(
        r#"Summarize this exchange concisely. Extract:
1. What was attempted
2. What succeeded or failed
3. Key facts learned
4. Files modified

Exchange:
{}

Respond in this format:
SUMMARY: <one sentence>
ACTIONS: <bullet list>
FACTS: <bullet list>
FILES: <comma-separated list>"#,
        exchange
    )
}

pub fn sanity_check_prompt(goal: &str, actions: &[String], current_state: &str) -> String {
    format!(
        r#"You are a sanity checker. Analyze if this agent is on track.

GOAL: {}

ACTIONS TAKEN:
{}

CURRENT STATE:
{}

Respond in JSON:
{{
  "on_track": true/false,
  "confidence": 0.0-1.0,
  "progress": 0.0-1.0,
  "concerns": ["list", "of", "concerns"],
  "suggestions": ["list", "of", "suggestions"],
  "should_pause": true/false
}}"#,
        goal,
        actions
            .iter()
            .map(|a| format!("- {}", a))
            .collect::<Vec<_>>()
            .join("\n"),
        current_state
    )
}

// ═══════════════════════════════════════════════════════════════
// CONTINUATION PROMPT GENERATION
// ═══════════════════════════════════════════════════════════════

/// Generate a natural continuation prompt based on context
pub fn continuation_prompt(
    last_response: &str,
    tool_results: &[ToolOutcome],
    momentum: &Momentum,
) -> String {
    // Check if LLM stated next steps
    if let Some(next_step) = extract_stated_intent(last_response) {
        return format!("Proceed: {}", next_step);
    }

    // All tools succeeded - minimal prompt
    if tool_results.iter().all(|r| r.success) {
        if momentum.score() > 0.8 {
            return "Continue.".into();
        }
        return "Continue. Verify progress.".into();
    }

    // Some failures
    let failures: Vec<_> = tool_results
        .iter()
        .filter(|r| !r.success)
        .map(|r| r.tool_name.as_str())
        .collect();

    if !failures.is_empty() {
        return format!(
            "Tools failed: {}. Assess the error and try a different approach.",
            failures.join(", ")
        );
    }

    // Default
    "Continue with the task.".into()
}

fn extract_stated_intent(response: &str) -> Option<String> {
    // Look for patterns like "Next, I'll...", "I'll now...", "Let me..."
    let patterns = [
        "next, i'll ",
        "i'll now ",
        "let me ",
        "now i'll ",
        "i will now ",
    ];

    let lower = response.to_lowercase();
    for pattern in patterns {
        if let Some(pos) = lower.find(pattern) {
            let start = pos + pattern.len();
            let end = lower[start..].find(['.', '\n']).unwrap_or(50).min(100);
            return Some(response[pos..pos + pattern.len() + end].to_string());
        }
    }
    None
}

// ═══════════════════════════════════════════════════════════════
// CONTEXT BUILDING
// ═══════════════════════════════════════════════════════════════

impl ContextLayers {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build context for the executor, respecting token budget
    pub fn build_executor_context(&self, budget: usize) -> String {
        let mut parts = Vec::new();
        let mut used = 0;

        // 1. Current goal (always include)
        if let Some(ref goal) = self.current_goal {
            let goal_section = format!("<goal>{}</goal>\n", goal);
            used += estimate_tokens(&goal_section);
            parts.push(goal_section);
        }

        // 2. Key facts (high priority)
        if !self.facts.is_empty() {
            let facts_section = format!(
                "<facts>\n{}\n</facts>\n",
                self.facts
                    .iter()
                    .map(|f| format!("- [{:?}] {}", f.category, f.content))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
            let tokens = estimate_tokens(&facts_section);
            if used + tokens < budget {
                used += tokens;
                parts.push(facts_section);
            }
        }

        // 3. Working memory (full detail, most recent)
        for item in self.working_memory.iter().rev() {
            let item_text = format!(
                "<exchange role=\"{}\">\n{}\n</exchange>\n",
                item.role, item.content
            );
            let tokens = estimate_tokens(&item_text);
            if used + tokens < budget {
                used += tokens;
                parts.push(item_text);
            } else {
                break;
            }
        }

        // 4. Summaries (fill remaining budget)
        for summary in self.summary_memory.iter().rev() {
            let summary_text = format!(
                "<summary iterations=\"{}-{}\">\n{}\n</summary>\n",
                summary.iteration_range.0, summary.iteration_range.1, summary.summary
            );
            let tokens = estimate_tokens(&summary_text);
            if used + tokens < budget {
                used += tokens;
                parts.push(summary_text);
            } else {
                break;
            }
        }

        parts.join("")
    }

    /// Add a new exchange to working memory
    pub fn add_exchange(&mut self, item: ContextItem) {
        self.working_memory.push(item);

        // Trigger compression if working memory is large
        // (actual compression happens asynchronously)
    }

    /// Record a fact
    pub fn add_fact(&mut self, fact: Fact) {
        // Deduplicate or update existing facts
        if let Some(existing) = self
            .facts
            .iter_mut()
            .find(|f| f.category == fact.category && similar(&f.content, &fact.content))
        {
            if fact.confidence > existing.confidence {
                *existing = fact;
            }
        } else {
            self.facts.push(fact);
        }
    }
}

fn estimate_tokens(text: &str) -> usize {
    // Rough estimate: ~4 chars per token
    text.len() / 4
}

fn similar(a: &str, b: &str) -> bool {
    // Simple similarity check
    let a_words: std::collections::HashSet<_> = a.split_whitespace().collect();
    let b_words: std::collections::HashSet<_> = b.split_whitespace().collect();
    let intersection = a_words.intersection(&b_words).count();
    let union = a_words.union(&b_words).count();
    if union == 0 {
        return true;
    }
    (intersection as f32 / union as f32) > 0.5
}

// ═══════════════════════════════════════════════════════════════
// SALIENCE SYSTEM
// ═══════════════════════════════════════════════════════════════

/// Salience tier - determines how much detail to include
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SalienceTier {
    /// Full detail - current focus, last 1-2 tool results
    Focus = 4,
    /// High detail - recent exchanges, active task context
    Recent = 3,
    /// Summarized - older exchanges, background context
    Summary = 2,
    /// Minimal - project facts, conventions, constraints
    Background = 1,
}

/// A scored context item for salience-aware rendering
#[derive(Debug, Clone)]
pub struct ScoredContext {
    pub content: String,
    pub tier: SalienceTier,
    pub score: f32, // 0.0 to 1.0, higher = more salient
    pub tokens: usize,
    pub category: ContextCategory,
}

/// Categories of context for salience scoring
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextCategory {
    SystemPrompt,      // Always included
    UserMessage,       // User's input
    AssistantResponse, // LLM's response
    ToolCall,          // Tool invocation
    ToolResult,        // Tool output
    Error,             // Errors are highly salient
    Summary,           // Compressed previous context
    Fact,              // Extracted key facts
    Intent,            // User's goals
    Constraint,        // Constraints/requirements
}

impl ContextCategory {
    /// Base salience weight for this category
    pub fn base_weight(&self) -> f32 {
        match self {
            ContextCategory::SystemPrompt => 1.0,
            ContextCategory::UserMessage => 0.9,
            ContextCategory::Error => 0.95, // Errors are very salient
            ContextCategory::Intent => 0.85,
            ContextCategory::ToolResult => 0.8,
            ContextCategory::ToolCall => 0.7,
            ContextCategory::AssistantResponse => 0.6,
            ContextCategory::Constraint => 0.5,
            ContextCategory::Fact => 0.4,
            ContextCategory::Summary => 0.3,
        }
    }
}

/// Factors that affect salience
#[derive(Debug, Clone, Default)]
pub struct SalienceFactors {
    /// Age in iterations (0 = current)
    pub age: u32,
    /// Contains keywords matching current task
    pub keyword_match: f32,
    /// Referenced by more recent content
    pub reference_count: u32,
    /// Contains error or warning
    pub has_error: bool,
    /// Contains decision or confirmation
    pub has_decision: bool,
    /// File path relevance to current focus
    pub file_relevance: f32,
}

impl SalienceFactors {
    /// Calculate overall salience score
    pub fn score(&self, category: ContextCategory) -> f32 {
        let mut score = category.base_weight();

        // Age decay: exponential falloff
        let age_factor = 1.0 / (1.0 + self.age as f32 * 0.3);
        score *= age_factor;

        // Keyword boost
        score += self.keyword_match * 0.2;

        // Reference boost (being mentioned increases salience)
        let ref_boost = (self.reference_count as f32 * 0.1).min(0.3);
        score += ref_boost;

        // Error boost
        if self.has_error {
            score = (score + 0.3).min(1.0);
        }

        // Decision boost
        if self.has_decision {
            score += 0.15;
        }

        // File relevance
        score += self.file_relevance * 0.1;

        score.clamp(0.0, 1.0)
    }
}

/// Salience-aware context builder
#[derive(Debug)]
pub struct SalienceContext {
    items: Vec<ScoredContext>,
    token_budget: usize,
    current_keywords: Vec<String>,
    focus_files: Vec<String>,
}

impl SalienceContext {
    pub fn new(token_budget: usize) -> Self {
        Self {
            items: Vec::new(),
            token_budget,
            current_keywords: Vec::new(),
            focus_files: Vec::new(),
        }
    }

    /// Set keywords that indicate relevance to current task
    pub fn set_keywords(&mut self, keywords: Vec<String>) {
        self.current_keywords = keywords;
    }

    /// Set files currently being worked on
    pub fn set_focus_files(&mut self, files: Vec<String>) {
        self.focus_files = files;
    }

    /// Add a context item with automatic salience scoring
    pub fn add(&mut self, content: String, category: ContextCategory, age: u32) {
        let factors = self.calculate_factors(&content, age);
        let score = factors.score(category);
        let tier = self.score_to_tier(score);
        let tokens = estimate_tokens(&content);

        self.items.push(ScoredContext {
            content,
            tier,
            score,
            tokens,
            category,
        });
    }

    /// Add with explicit tier override
    pub fn add_with_tier(
        &mut self,
        content: String,
        category: ContextCategory,
        tier: SalienceTier,
    ) {
        let tokens = estimate_tokens(&content);
        let score = match tier {
            SalienceTier::Focus => 1.0,
            SalienceTier::Recent => 0.75,
            SalienceTier::Summary => 0.5,
            SalienceTier::Background => 0.25,
        };

        self.items.push(ScoredContext {
            content,
            tier,
            score,
            tokens,
            category,
        });
    }

    fn calculate_factors(&self, content: &str, age: u32) -> SalienceFactors {
        let content_lower = content.to_lowercase();

        // Keyword matching
        let keyword_match = if self.current_keywords.is_empty() {
            0.0
        } else {
            let matches = self
                .current_keywords
                .iter()
                .filter(|kw| content_lower.contains(&kw.to_lowercase()))
                .count();
            (matches as f32 / self.current_keywords.len() as f32).min(1.0)
        };

        // File relevance
        let file_relevance = if self.focus_files.is_empty() {
            0.0
        } else {
            let matches = self
                .focus_files
                .iter()
                .filter(|f| content.contains(f.as_str()))
                .count();
            (matches as f32 / self.focus_files.len() as f32).min(1.0)
        };

        // Error detection
        let has_error = content_lower.contains("error")
            || content_lower.contains("failed")
            || content_lower.contains("exception")
            || content_lower.contains("panic");

        // Decision detection
        let has_decision = content_lower.contains("decided")
            || content_lower.contains("will ")
            || content_lower.contains("should ")
            || content_lower.contains("let's ");

        SalienceFactors {
            age,
            keyword_match,
            reference_count: 0,
            has_error,
            has_decision,
            file_relevance,
        }
    }

    fn score_to_tier(&self, score: f32) -> SalienceTier {
        if score >= 0.8 {
            SalienceTier::Focus
        } else if score >= 0.5 {
            SalienceTier::Recent
        } else if score >= 0.25 {
            SalienceTier::Summary
        } else {
            SalienceTier::Background
        }
    }

    /// Build the final context string, respecting token budget
    pub fn build(&self) -> String {
        let mut output = String::new();
        let mut used_tokens = 0;

        // Sort by tier (highest first), then by score within tier
        let mut sorted: Vec<_> = self.items.iter().collect();
        sorted.sort_by(|a, b| match b.tier.cmp(&a.tier) {
            std::cmp::Ordering::Equal => b
                .score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal),
            other => other,
        });

        // Allocate budget by tier
        let tier_budgets = self.allocate_budgets();

        let mut tier_usage: std::collections::HashMap<SalienceTier, usize> =
            std::collections::HashMap::new();

        for item in sorted {
            let tier_budget = tier_budgets.get(&item.tier).copied().unwrap_or(0);
            let current_usage = tier_usage.get(&item.tier).copied().unwrap_or(0);

            // Check if we can fit this item
            if used_tokens + item.tokens > self.token_budget {
                // Try to compress
                let compressed = self.compress_content(&item.content, item.tokens / 2);
                let compressed_tokens = estimate_tokens(&compressed);
                if used_tokens + compressed_tokens <= self.token_budget {
                    output.push_str(&compressed);
                    output.push('\n');
                    used_tokens += compressed_tokens;
                }
                continue;
            }

            if current_usage + item.tokens > tier_budget && item.tier != SalienceTier::Focus {
                // Compress items that exceed tier budget
                let compressed = self.compress_content(&item.content, item.tokens / 2);
                output.push_str(&compressed);
                output.push('\n');
                used_tokens += estimate_tokens(&compressed);
                *tier_usage.entry(item.tier).or_insert(0) += estimate_tokens(&compressed);
            } else {
                output.push_str(&item.content);
                output.push('\n');
                used_tokens += item.tokens;
                *tier_usage.entry(item.tier).or_insert(0) += item.tokens;
            }
        }

        output
    }

    fn allocate_budgets(&self) -> std::collections::HashMap<SalienceTier, usize> {
        let mut budgets = std::collections::HashMap::new();
        // Focus: 40%, Recent: 30%, Summary: 20%, Background: 10%
        budgets.insert(SalienceTier::Focus, self.token_budget * 40 / 100);
        budgets.insert(SalienceTier::Recent, self.token_budget * 30 / 100);
        budgets.insert(SalienceTier::Summary, self.token_budget * 20 / 100);
        budgets.insert(SalienceTier::Background, self.token_budget * 10 / 100);
        budgets
    }

    fn compress_content(&self, content: &str, target_tokens: usize) -> String {
        let lines: Vec<&str> = content.lines().collect();
        if lines.len() <= 3 {
            return content.to_string();
        }

        // Keep first line (often a summary/heading) and key lines
        let mut kept: Vec<String> = vec![lines[0].to_string()];

        // Find lines with important markers
        for line in &lines[1..] {
            let lower = line.to_lowercase();
            if lower.contains("error")
                || lower.contains("success")
                || lower.contains("result:")
                || lower.contains("file:")
                || line.starts_with("- ")
                || line.starts_with("* ")
            {
                kept.push(line.to_string());
                if estimate_tokens(&kept.join("\n")) >= target_tokens {
                    break;
                }
            }
        }

        if kept.len() < lines.len() {
            kept.push(format!("... ({} lines omitted)", lines.len() - kept.len()));
        }

        kept.join("\n")
    }

    /// Get statistics about current context
    pub fn stats(&self) -> ContextStats {
        let mut stats = ContextStats::default();

        for item in &self.items {
            stats.total_items += 1;
            stats.total_tokens += item.tokens;

            match item.tier {
                SalienceTier::Focus => stats.focus_items += 1,
                SalienceTier::Recent => stats.recent_items += 1,
                SalienceTier::Summary => stats.summary_items += 1,
                SalienceTier::Background => stats.background_items += 1,
            }
        }

        stats.budget_used = (stats.total_tokens as f32 / self.token_budget as f32).min(1.0);
        stats
    }
}

/// Statistics about context usage
#[derive(Debug, Clone, Default)]
pub struct ContextStats {
    pub total_items: usize,
    pub total_tokens: usize,
    pub focus_items: usize,
    pub recent_items: usize,
    pub summary_items: usize,
    pub background_items: usize,
    pub budget_used: f32,
}

impl std::fmt::Display for ContextStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Context: {} items, {} tokens ({:.0}% budget), focus:{} recent:{} summary:{} bg:{}",
            self.total_items,
            self.total_tokens,
            self.budget_used * 100.0,
            self.focus_items,
            self.recent_items,
            self.summary_items,
            self.background_items
        )
    }
}

/// Extract keywords from user intent for salience matching
pub fn extract_keywords(text: &str) -> Vec<String> {
    let stopwords: std::collections::HashSet<&str> = [
        "the", "a", "an", "is", "are", "was", "were", "be", "been", "being", "have", "has", "had",
        "do", "does", "did", "will", "would", "could", "should", "may", "might", "must", "shall",
        "can", "need", "dare", "to", "of", "in", "for", "on", "with", "at", "by", "from", "as",
        "into", "through", "during", "before", "after", "above", "below", "between", "under",
        "again", "further", "then", "once", "here", "there", "when", "where", "why", "how", "all",
        "each", "few", "more", "most", "other", "some", "such", "no", "nor", "not", "only", "own",
        "same", "so", "than", "too", "very", "just", "and", "but", "if", "or", "because", "until",
        "while", "this", "that", "these", "those", "what", "which", "who", "whom", "i", "you",
        "he", "she", "it", "we", "they", "me", "him", "her", "us", "them", "my", "your", "his",
        "its", "our", "their", "please", "help", "want", "like", "make", "get", "let",
    ]
    .iter()
    .cloned()
    .collect();

    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|w| w.len() > 2 && !stopwords.contains(w))
        .map(String::from)
        .collect()
}

// ═══════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_momentum() {
        let mut m = Momentum::default();
        assert_eq!(m.score(), 1.0);

        m.record(ToolOutcome {
            tool_name: "read".into(),
            success: true,
            was_useful: true,
        });
        m.record(ToolOutcome {
            tool_name: "read".into(),
            success: true,
            was_useful: true,
        });
        assert_eq!(m.score(), 1.0);

        m.record(ToolOutcome {
            tool_name: "write".into(),
            success: false,
            was_useful: false,
        });
        assert!(m.score() < 1.0);
    }

    #[test]
    fn test_stuck_detector() {
        let mut s = StuckDetector::default();
        assert!(!s.is_stuck());

        // Same action 3 times
        s.record_action(12345);
        s.record_action(12345);
        s.record_action(12345);
        assert!(s.is_stuck());
    }

    #[test]
    fn test_tool_risk() {
        assert_eq!(ToolRisk::from_tool_call("read", "file.txt"), ToolRisk::Safe);
        assert_eq!(
            ToolRisk::from_tool_call("bash", "ls -la"),
            ToolRisk::Cautious
        );
        assert_eq!(
            ToolRisk::from_tool_call("bash", "rm -rf /"),
            ToolRisk::Dangerous
        );
    }

    #[test]
    fn test_extract_intent() {
        let response = "I see the issue. Next, I'll update the config file to fix the bug.";
        let intent = extract_stated_intent(response);
        assert!(intent.is_some());
        assert!(intent.unwrap().contains("update the config"));
    }

    #[test]
    fn test_salience_scoring() {
        let factors = SalienceFactors {
            age: 0,
            keyword_match: 0.5,
            reference_count: 2,
            has_error: true,
            has_decision: false,
            file_relevance: 0.3,
        };

        let score = factors.score(ContextCategory::ToolResult);
        assert!(score > 0.8, "Error + keyword match should boost score high");
    }

    #[test]
    fn test_salience_age_decay() {
        let young = SalienceFactors {
            age: 0,
            ..Default::default()
        };
        let old = SalienceFactors {
            age: 10,
            ..Default::default()
        };

        let young_score = young.score(ContextCategory::UserMessage);
        let old_score = old.score(ContextCategory::UserMessage);

        assert!(
            young_score > old_score,
            "Older content should have lower salience"
        );
    }

    #[test]
    fn test_salience_context_build() {
        let mut ctx = SalienceContext::new(1000);
        ctx.set_keywords(vec!["auth".into(), "login".into()]);

        ctx.add(
            "User message about auth".into(),
            ContextCategory::UserMessage,
            0,
        );
        ctx.add(
            "Old unrelated message".into(),
            ContextCategory::UserMessage,
            5,
        );
        ctx.add("Error in login module".into(), ContextCategory::Error, 1);

        let stats = ctx.stats();
        assert_eq!(stats.total_items, 3);
        assert!(stats.focus_items >= 1, "Error should be in focus tier");
    }

    #[test]
    fn test_extract_keywords() {
        let text = "Please help me fix the authentication bug in the login module";
        let keywords = extract_keywords(text);

        assert!(keywords.contains(&"fix".into()));
        assert!(keywords.contains(&"authentication".into()));
        assert!(keywords.contains(&"login".into()));
        assert!(!keywords.contains(&"the".into())); // stopword
        assert!(!keywords.contains(&"please".into())); // stopword
    }

    #[test]
    fn test_tier_ordering() {
        assert!(SalienceTier::Focus > SalienceTier::Recent);
        assert!(SalienceTier::Recent > SalienceTier::Summary);
        assert!(SalienceTier::Summary > SalienceTier::Background);
    }
}
