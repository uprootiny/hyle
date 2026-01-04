//! Intent and context management for robust conversation handling
//!
//! Provides:
//! - Intent hierarchy (main goal → subtasks → asides)
//! - Context segmentation (main vs tangent)
//! - Smart summarization for long conversations
//! - Session forking/merging

#![allow(dead_code)] // Forward-looking module for context management

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

// ═══════════════════════════════════════════════════════════════
// INTENT HIERARCHY
// ═══════════════════════════════════════════════════════════════

/// An intent represents a goal at some level of the hierarchy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Intent {
    pub id: String,
    pub description: String,
    pub kind: IntentKind,
    pub status: IntentStatus,
    pub created_at: DateTime<Utc>,
    pub parent_id: Option<String>,
    pub children: Vec<String>,
    pub context_tokens: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum IntentKind {
    Primary, // Main development goal
    Subtask, // Part of primary
    Aside,   // Tangent/exploration
    Fix,     // Bug fix during development
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum IntentStatus {
    Active,
    Paused,
    Completed,
    Abandoned,
}

impl Intent {
    pub fn new(description: &str, kind: IntentKind) -> Self {
        Self {
            id: nanoid(),
            description: description.to_string(),
            kind,
            status: IntentStatus::Active,
            created_at: Utc::now(),
            parent_id: None,
            children: Vec::new(),
            context_tokens: 0,
        }
    }

    pub fn primary(description: &str) -> Self {
        Self::new(description, IntentKind::Primary)
    }

    pub fn subtask(description: &str, parent: &str) -> Self {
        let mut intent = Self::new(description, IntentKind::Subtask);
        intent.parent_id = Some(parent.to_string());
        intent
    }

    pub fn aside(description: &str, parent: &str) -> Self {
        let mut intent = Self::new(description, IntentKind::Aside);
        intent.parent_id = Some(parent.to_string());
        intent
    }
}

// ═══════════════════════════════════════════════════════════════
// INTENT STACK
// ═══════════════════════════════════════════════════════════════

/// Manages active intents in a stack-like structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentStack {
    intents: Vec<Intent>,
    active_id: Option<String>,
    history: Vec<IntentTransition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct IntentTransition {
    from: Option<String>,
    to: Option<String>,
    reason: String,
    at: DateTime<Utc>,
}

impl IntentStack {
    pub fn new() -> Self {
        Self {
            intents: Vec::new(),
            active_id: None,
            history: Vec::new(),
        }
    }

    /// Set the primary intent (clears stack if different)
    pub fn set_primary(&mut self, description: &str) -> &Intent {
        let intent = Intent::primary(description);
        let id = intent.id.clone();

        self.transition(Some(&id), "Set primary intent");
        self.intents.push(intent);
        self.active_id = Some(id.clone());

        self.get(&id).unwrap()
    }

    /// Push a subtask onto the stack
    pub fn push_subtask(&mut self, description: &str) -> &Intent {
        let parent_id = self.active_id.clone().unwrap_or_default();
        let intent = Intent::subtask(description, &parent_id);
        let id = intent.id.clone();

        // Add as child of parent
        if let Some(parent) = self.get_mut(&parent_id) {
            parent.children.push(id.clone());
        }

        self.transition(Some(&id), "Push subtask");
        self.intents.push(intent);
        self.active_id = Some(id.clone());

        self.get(&id).unwrap()
    }

    /// Push an aside (tangent)
    pub fn push_aside(&mut self, description: &str) -> &Intent {
        let parent_id = self.active_id.clone().unwrap_or_default();
        let mut intent = Intent::aside(description, &parent_id);
        intent.status = IntentStatus::Active;
        let id = intent.id.clone();

        // Pause parent
        if let Some(parent) = self.get_mut(&parent_id) {
            parent.status = IntentStatus::Paused;
            parent.children.push(id.clone());
        }

        self.transition(Some(&id), "Push aside");
        self.intents.push(intent);
        self.active_id = Some(id.clone());

        self.get(&id).unwrap()
    }

    /// Pop current intent, return to parent
    pub fn pop(&mut self) -> Option<&Intent> {
        let current_id = self.active_id.take()?;

        // Mark current as completed
        if let Some(current) = self.get_mut(&current_id) {
            current.status = IntentStatus::Completed;
        }

        // Find parent and make it active
        let parent_id = self.get(&current_id).and_then(|i| i.parent_id.clone());

        if let Some(ref pid) = parent_id {
            if let Some(parent) = self.get_mut(pid) {
                parent.status = IntentStatus::Active;
            }
            self.active_id = Some(pid.clone());
            let active_id = self.active_id.clone();
            self.transition(active_id.as_deref(), "Pop completed");
        }

        self.active()
    }

    /// Abandon current intent without completing
    pub fn abandon(&mut self) -> Option<&Intent> {
        let current_id = self.active_id.take()?;

        if let Some(current) = self.get_mut(&current_id) {
            current.status = IntentStatus::Abandoned;
        }

        let parent_id = self.get(&current_id).and_then(|i| i.parent_id.clone());

        if let Some(ref pid) = parent_id {
            if let Some(parent) = self.get_mut(pid) {
                parent.status = IntentStatus::Active;
            }
            self.active_id = Some(pid.clone());
            let active_id = self.active_id.clone();
            self.transition(active_id.as_deref(), "Abandon");
        }

        self.active()
    }

    /// Get active intent
    pub fn active(&self) -> Option<&Intent> {
        self.active_id.as_ref().and_then(|id| self.get(id))
    }

    /// Get primary intent (root of stack)
    pub fn primary(&self) -> Option<&Intent> {
        self.intents.iter().find(|i| i.kind == IntentKind::Primary)
    }

    /// Get intent by ID
    pub fn get(&self, id: &str) -> Option<&Intent> {
        self.intents.iter().find(|i| i.id == id)
    }

    fn get_mut(&mut self, id: &str) -> Option<&mut Intent> {
        self.intents.iter_mut().find(|i| i.id == id)
    }

    fn transition(&mut self, to: Option<&str>, reason: &str) {
        self.history.push(IntentTransition {
            from: self.active_id.clone(),
            to: to.map(String::from),
            reason: reason.to_string(),
            at: Utc::now(),
        });
    }

    /// Get breadcrumb path from primary to active
    pub fn breadcrumb(&self) -> Vec<&Intent> {
        let mut path = Vec::new();
        let mut current = self.active();

        while let Some(intent) = current {
            path.push(intent);
            current = intent.parent_id.as_ref().and_then(|pid| self.get(pid));
        }

        path.reverse();
        path
    }

    /// Format as status line
    pub fn status_line(&self) -> String {
        let crumbs = self.breadcrumb();
        if crumbs.is_empty() {
            return "No active intent".to_string();
        }

        crumbs
            .iter()
            .map(|i| {
                let icon = match i.kind {
                    IntentKind::Primary => "◉",
                    IntentKind::Subtask => "○",
                    IntentKind::Aside => "◇",
                    IntentKind::Fix => "⚡",
                };
                format!("{} {}", icon, truncate(&i.description, 30))
            })
            .collect::<Vec<_>>()
            .join(" → ")
    }

    /// Count asides in current path
    pub fn aside_depth(&self) -> usize {
        self.breadcrumb()
            .iter()
            .filter(|i| i.kind == IntentKind::Aside)
            .count()
    }

    /// Check if stack is empty (no intents)
    pub fn is_empty(&self) -> bool {
        self.intents.is_empty()
    }

    /// Push an intent (auto-routes based on kind and current state)
    pub fn push(&mut self, intent: Intent) {
        let id = intent.id.clone();
        let kind = intent.kind;

        match kind {
            IntentKind::Primary => {
                // Clear stack and set as new primary
                self.intents.clear();
                self.history.clear();
                self.transition(Some(&id), "New primary intent");
                self.intents.push(intent);
                self.active_id = Some(id);
            }
            IntentKind::Subtask | IntentKind::Fix => {
                // Add as child of current active (or root if none)
                if let Some(ref parent_id) = self.active_id.clone() {
                    if let Some(parent) = self.get_mut(parent_id) {
                        parent.children.push(id.clone());
                    }
                }
                self.transition(Some(&id), "Push subtask/fix");
                self.intents.push(intent);
                self.active_id = Some(id);
            }
            IntentKind::Aside => {
                // Pause current, push aside
                if let Some(ref parent_id) = self.active_id.clone() {
                    if let Some(parent) = self.get_mut(parent_id) {
                        parent.status = IntentStatus::Paused;
                        parent.children.push(id.clone());
                    }
                }
                self.transition(Some(&id), "Push aside");
                self.intents.push(intent);
                self.active_id = Some(id);
            }
        }
    }
}

impl Default for IntentStack {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════════
// CONTEXT SEGMENTS
// ═══════════════════════════════════════════════════════════════

/// A segment of conversation with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextSegment {
    pub id: String,
    pub intent_id: String,
    pub kind: SegmentKind,
    pub messages: Vec<ContextMessage>,
    pub token_count: usize,
    pub summary: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum SegmentKind {
    Main,     // Main development flow
    Tangent,  // Exploration/aside
    Fix,      // Bug fix
    Research, // Information gathering
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextMessage {
    pub role: String,
    pub content: String,
    pub tokens: usize,
}

impl ContextSegment {
    pub fn new(intent_id: &str, kind: SegmentKind) -> Self {
        Self {
            id: nanoid(),
            intent_id: intent_id.to_string(),
            kind,
            messages: Vec::new(),
            token_count: 0,
            summary: None,
            created_at: Utc::now(),
        }
    }

    pub fn add_message(&mut self, role: &str, content: &str, tokens: usize) {
        self.messages.push(ContextMessage {
            role: role.to_string(),
            content: content.to_string(),
            tokens,
        });
        self.token_count += tokens;
    }

    pub fn set_summary(&mut self, summary: &str) {
        self.summary = Some(summary.to_string());
    }
}

// ═══════════════════════════════════════════════════════════════
// CONTEXT MANAGER
// ═══════════════════════════════════════════════════════════════

/// Manages conversation context with smart windowing
pub struct ContextManager {
    pub intents: IntentStack,
    segments: VecDeque<ContextSegment>,
    current_segment: Option<ContextSegment>,
    max_context_tokens: usize,
    summarize_threshold: usize,
}

impl ContextManager {
    pub fn new(max_tokens: usize) -> Self {
        Self {
            intents: IntentStack::new(),
            segments: VecDeque::new(),
            current_segment: None,
            max_context_tokens: max_tokens,
            summarize_threshold: max_tokens / 2,
        }
    }

    /// Start a new primary task
    pub fn start_task(&mut self, description: &str) {
        // Complete current segment if any
        self.close_segment();

        // Set primary intent
        let intent = self.intents.set_primary(description);
        let intent_id = intent.id.clone();

        // Start new main segment
        self.current_segment = Some(ContextSegment::new(&intent_id, SegmentKind::Main));
    }

    /// Start an aside/tangent
    pub fn start_aside(&mut self, description: &str) {
        self.close_segment();

        let intent = self.intents.push_aside(description);
        let intent_id = intent.id.clone();

        self.current_segment = Some(ContextSegment::new(&intent_id, SegmentKind::Tangent));
    }

    /// Return from aside to main task
    pub fn return_from_aside(&mut self) {
        self.close_segment();
        self.intents.pop();

        if let Some(intent) = self.intents.active() {
            let intent_id = intent.id.clone();
            let kind = match intent.kind {
                IntentKind::Aside => SegmentKind::Tangent,
                _ => SegmentKind::Main,
            };
            self.current_segment = Some(ContextSegment::new(&intent_id, kind));
        }
    }

    /// Add message to current segment
    pub fn add_message(&mut self, role: &str, content: &str, tokens: usize) {
        if let Some(ref mut segment) = self.current_segment {
            segment.add_message(role, content, tokens);
        }

        // Check if we need to summarize
        if self.total_tokens() > self.summarize_threshold {
            self.compact();
        }
    }

    /// Get context for LLM (main + relevant tangent summaries)
    pub fn context_for_llm(&self) -> String {
        let mut context = String::new();

        // Add intent breadcrumb
        context.push_str(&format!("Current: {}\n\n", self.intents.status_line()));

        // Add main segments (full or summarized)
        for segment in &self.segments {
            if segment.kind == SegmentKind::Main {
                if let Some(ref summary) = segment.summary {
                    context.push_str(&format!("[Summary: {}]\n\n", summary));
                } else {
                    for msg in &segment.messages {
                        context.push_str(&format!("{}: {}\n", msg.role, msg.content));
                    }
                }
            } else if segment.kind == SegmentKind::Tangent {
                // Only include tangent summaries
                if let Some(ref summary) = segment.summary {
                    context.push_str(&format!("[Aside: {}]\n", summary));
                }
            }
        }

        // Add current segment
        if let Some(ref segment) = self.current_segment {
            for msg in &segment.messages {
                context.push_str(&format!("{}: {}\n", msg.role, msg.content));
            }
        }

        context
    }

    /// Total tokens across all segments
    pub fn total_tokens(&self) -> usize {
        let archived: usize = self.segments.iter().map(|s| s.token_count).sum();
        let current = self
            .current_segment
            .as_ref()
            .map(|s| s.token_count)
            .unwrap_or(0);
        archived + current
    }

    /// Compact context by summarizing old segments
    fn compact(&mut self) {
        // Mark old tangent segments for summarization
        for segment in &mut self.segments {
            if segment.kind == SegmentKind::Tangent && segment.summary.is_none() {
                // Generate summary placeholder (real impl would use LLM)
                let msg_count = segment.messages.len();
                segment.set_summary(&format!(
                    "{} messages about {}",
                    msg_count,
                    self.intents
                        .get(&segment.intent_id)
                        .map(|i| i.description.as_str())
                        .unwrap_or("tangent")
                ));
            }
        }

        // Remove very old segments if still over limit
        while self.total_tokens() > self.max_context_tokens && self.segments.len() > 2 {
            self.segments.pop_front();
        }
    }

    fn close_segment(&mut self) {
        if let Some(segment) = self.current_segment.take() {
            if !segment.messages.is_empty() {
                self.segments.push_back(segment);
            }
        }
    }

    /// Get aside depth warning
    pub fn aside_warning(&self) -> Option<String> {
        let depth = self.intents.aside_depth();
        if depth >= 2 {
            Some(format!(
                "Warning: {} levels deep in asides. Consider returning to main task.",
                depth
            ))
        } else {
            None
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// CONSTRAINTS
// ═══════════════════════════════════════════════════════════════

/// Constraints apply at different levels
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Constraint {
    pub level: ConstraintLevel,
    pub kind: ConstraintKind,
    pub description: String,
    pub source: String, // Where this came from
    pub active: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConstraintLevel {
    Project,   // Applies to entire project (from CLAUDE.md, conventions)
    Session,   // Applies to this session
    Task,      // Applies to current task
    Immediate, // Applies to current action
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConstraintKind {
    MustDo,    // Required action/behavior
    MustNotDo, // Prohibited action/behavior
    Prefer,    // Preference if possible
    Avoid,     // Avoid if possible
    Style,     // Code style constraint
    Safety,    // Safety-related constraint
}

impl Constraint {
    pub fn new(
        level: ConstraintLevel,
        kind: ConstraintKind,
        description: &str,
        source: &str,
    ) -> Self {
        Self {
            level,
            kind,
            description: description.into(),
            source: source.into(),
            active: true,
        }
    }

    pub fn project(description: &str, source: &str) -> Self {
        Self::new(
            ConstraintLevel::Project,
            ConstraintKind::MustDo,
            description,
            source,
        )
    }

    pub fn safety(description: &str) -> Self {
        Self::new(
            ConstraintLevel::Project,
            ConstraintKind::Safety,
            description,
            "system",
        )
    }
}

/// Tracks constraints at all levels
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConstraintSet {
    constraints: Vec<Constraint>,
}

impl ConstraintSet {
    pub fn new() -> Self {
        Self {
            constraints: Vec::new(),
        }
    }

    pub fn add(&mut self, constraint: Constraint) {
        self.constraints.push(constraint);
    }

    pub fn at_level(&self, level: ConstraintLevel) -> Vec<&Constraint> {
        self.constraints
            .iter()
            .filter(|c| c.level == level && c.active)
            .collect()
    }

    pub fn must_do(&self) -> Vec<&Constraint> {
        self.constraints
            .iter()
            .filter(|c| c.kind == ConstraintKind::MustDo && c.active)
            .collect()
    }

    pub fn must_not(&self) -> Vec<&Constraint> {
        self.constraints
            .iter()
            .filter(|c| c.kind == ConstraintKind::MustNotDo && c.active)
            .collect()
    }

    /// Format constraints for LLM context
    pub fn for_llm(&self) -> String {
        let mut out = String::new();

        // Group by level
        for level in [
            ConstraintLevel::Project,
            ConstraintLevel::Session,
            ConstraintLevel::Task,
            ConstraintLevel::Immediate,
        ] {
            let at_level = self.at_level(level);
            if !at_level.is_empty() {
                out.push_str(&format!("\n{:?} Constraints:\n", level));
                for c in at_level {
                    let prefix = match c.kind {
                        ConstraintKind::MustDo => "✓ MUST:",
                        ConstraintKind::MustNotDo => "✗ MUST NOT:",
                        ConstraintKind::Prefer => "→ Prefer:",
                        ConstraintKind::Avoid => "← Avoid:",
                        ConstraintKind::Style => "◆ Style:",
                        ConstraintKind::Safety => "⚠ Safety:",
                    };
                    out.push_str(&format!("  {} {}\n", prefix, c.description));
                }
            }
        }

        out
    }
}

// ═══════════════════════════════════════════════════════════════
// MULTI-GRANULARITY VIEW
// ═══════════════════════════════════════════════════════════════

/// Different granularity levels for viewing intent
#[derive(Debug, Clone)]
pub struct IntentView {
    /// High-level: What is the user ultimately trying to achieve?
    pub high_level: String,

    /// Mid-level: What are the current subtasks/phases?
    pub mid_level: Vec<String>,

    /// Low-level: What's the immediate next action?
    pub low_level: String,

    /// Constraints that apply
    pub constraints: ConstraintSet,
}

impl IntentView {
    /// Build from IntentStack
    pub fn from_stack(stack: &IntentStack) -> Self {
        let primary = stack
            .primary()
            .map(|i| i.description.clone())
            .unwrap_or_else(|| "No primary goal set".into());

        let mid_level: Vec<String> = stack
            .breadcrumb()
            .iter()
            .skip(1) // Skip primary
            .filter(|i| i.kind == IntentKind::Subtask)
            .map(|i| i.description.clone())
            .collect();

        let low_level = stack
            .active()
            .map(|i| {
                format!(
                    "{}: {}",
                    match i.kind {
                        IntentKind::Primary => "Main",
                        IntentKind::Subtask => "Subtask",
                        IntentKind::Aside => "Aside",
                        IntentKind::Fix => "Fix",
                    },
                    i.description
                )
            })
            .unwrap_or_else(|| "No active task".into());

        Self {
            high_level: primary,
            mid_level,
            low_level,
            constraints: ConstraintSet::new(),
        }
    }

    /// Format for display
    pub fn display(&self) -> String {
        let mut out = String::new();

        out.push_str(&format!("🎯 Goal: {}\n", self.high_level));

        if !self.mid_level.is_empty() {
            out.push_str("📋 Tasks:\n");
            for (i, task) in self.mid_level.iter().enumerate() {
                out.push_str(&format!("   {}. {}\n", i + 1, task));
            }
        }

        out.push_str(&format!("▶ Now: {}\n", self.low_level));

        out
    }

    /// Format for LLM context at different verbosity
    pub fn for_llm(&self, verbosity: Verbosity) -> String {
        match verbosity {
            Verbosity::Minimal => {
                format!("Goal: {}. Now: {}", self.high_level, self.low_level)
            }
            Verbosity::Normal => {
                let mut out = format!("Goal: {}\n", self.high_level);
                if !self.mid_level.is_empty() {
                    out.push_str(&format!("Tasks: {}\n", self.mid_level.join(" → ")));
                }
                out.push_str(&format!("Current: {}", self.low_level));
                out
            }
            Verbosity::Full => {
                let mut out = self.display();
                out.push_str(&self.constraints.for_llm());
                out
            }
        }
    }
}

impl Default for IntentView {
    fn default() -> Self {
        Self {
            high_level: "No goal set".into(),
            mid_level: vec![],
            low_level: "Awaiting user input".into(),
            constraints: ConstraintSet::new(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Verbosity {
    Minimal, // One line
    Normal,  // Few lines
    Full,    // Complete with constraints
}

// ═══════════════════════════════════════════════════════════════
// PROMPTS FOR FREE LLM MAINTENANCE
// ═══════════════════════════════════════════════════════════════

/// Prompt for a free LLM to extract/update high-level goal
pub fn goal_extraction_prompt(conversation: &str) -> String {
    format!(
        r#"Extract the user's high-level goal from this conversation.
Respond with a single sentence describing what they ultimately want to achieve.

Conversation:
{}

Goal:"#,
        conversation
    )
}

/// Prompt to identify current subtasks
pub fn subtask_extraction_prompt(goal: &str, recent_context: &str) -> String {
    format!(
        r#"Given this goal and recent context, list the current subtasks.
Respond with a bullet list of 2-5 subtasks.

Goal: {}

Recent context:
{}

Subtasks:"#,
        goal, recent_context
    )
}

/// Prompt to extract constraints from user messages
pub fn constraint_extraction_prompt(user_messages: &str) -> String {
    format!(
        r#"Extract any constraints or requirements from these user messages.
Look for: style preferences, things to avoid, required behaviors, safety concerns.

Messages:
{}

Respond in this format:
MUST: [things that must be done]
MUST_NOT: [things to avoid]
PREFER: [preferences]
STYLE: [style requirements]"#,
        user_messages
    )
}

// ═══════════════════════════════════════════════════════════════
// HELPERS
// ═══════════════════════════════════════════════════════════════

fn nanoid() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{:x}", nanos)
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max - 3])
    }
}

// ═══════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_intent_creation() {
        let intent = Intent::primary("Build feature X");
        assert_eq!(intent.kind, IntentKind::Primary);
        assert!(intent.parent_id.is_none());
    }

    #[test]
    fn test_intent_stack_basic() {
        let mut stack = IntentStack::new();

        stack.set_primary("Build hyle");
        assert_eq!(stack.active().unwrap().description, "Build hyle");

        stack.push_subtask("Add model eval");
        assert_eq!(stack.active().unwrap().description, "Add model eval");
        assert_eq!(stack.primary().unwrap().description, "Build hyle");
    }

    #[test]
    fn test_intent_stack_aside() {
        let mut stack = IntentStack::new();

        stack.set_primary("Build feature");
        stack.push_aside("Research tangent");

        assert_eq!(stack.aside_depth(), 1);
        assert_eq!(stack.active().unwrap().kind, IntentKind::Aside);

        // Primary should be paused
        assert_eq!(stack.primary().unwrap().status, IntentStatus::Paused);
    }

    #[test]
    fn test_intent_stack_pop() {
        let mut stack = IntentStack::new();

        stack.set_primary("Main task");
        stack.push_subtask("Subtask 1");
        stack.push_subtask("Subtask 2");

        assert_eq!(stack.breadcrumb().len(), 3);

        stack.pop();
        assert_eq!(stack.active().unwrap().description, "Subtask 1");

        stack.pop();
        assert_eq!(stack.active().unwrap().description, "Main task");
    }

    #[test]
    fn test_breadcrumb() {
        let mut stack = IntentStack::new();

        stack.set_primary("Build app");
        stack.push_subtask("Add auth");
        stack.push_aside("Research OAuth");

        let crumbs = stack.breadcrumb();
        assert_eq!(crumbs.len(), 3);
        assert_eq!(crumbs[0].description, "Build app");
        assert_eq!(crumbs[1].description, "Add auth");
        assert_eq!(crumbs[2].description, "Research OAuth");
    }

    #[test]
    fn test_status_line() {
        let mut stack = IntentStack::new();

        stack.set_primary("Build app");
        stack.push_subtask("Add feature");

        let line = stack.status_line();
        assert!(line.contains("Build app"));
        assert!(line.contains("Add feature"));
        assert!(line.contains("→"));
    }

    #[test]
    fn test_context_segment() {
        let mut segment = ContextSegment::new("intent-1", SegmentKind::Main);

        segment.add_message("user", "Hello", 5);
        segment.add_message("assistant", "Hi there", 10);

        assert_eq!(segment.messages.len(), 2);
        assert_eq!(segment.token_count, 15);
    }

    #[test]
    fn test_context_manager_basic() {
        let mut cm = ContextManager::new(10000);

        cm.start_task("Build feature X");
        cm.add_message("user", "Let's start", 10);
        cm.add_message("assistant", "Sure, let's begin", 20);

        assert_eq!(cm.total_tokens(), 30);
        assert!(cm.intents.active().is_some());
    }

    #[test]
    fn test_context_manager_aside() {
        let mut cm = ContextManager::new(10000);

        cm.start_task("Main task");
        cm.add_message("user", "Do main thing", 10);

        cm.start_aside("Quick tangent");
        cm.add_message("user", "Tangent question", 10);
        cm.add_message("assistant", "Tangent answer", 20);

        assert_eq!(cm.intents.aside_depth(), 1);

        cm.return_from_aside();
        assert_eq!(cm.intents.aside_depth(), 0);
        assert_eq!(cm.intents.active().unwrap().description, "Main task");
    }

    #[test]
    fn test_aside_warning() {
        let mut cm = ContextManager::new(10000);

        cm.start_task("Main");
        assert!(cm.aside_warning().is_none());

        cm.start_aside("Aside 1");
        assert!(cm.aside_warning().is_none()); // Only 1 deep

        cm.start_aside("Aside 2");
        assert!(cm.aside_warning().is_some()); // 2 deep - warn
    }

    #[test]
    fn test_context_for_llm() {
        let mut cm = ContextManager::new(10000);

        cm.start_task("Build feature");
        cm.add_message("user", "Start building", 10);
        cm.add_message("assistant", "On it", 5);

        let ctx = cm.context_for_llm();
        assert!(ctx.contains("Build feature"));
        assert!(ctx.contains("Start building"));
    }
}
