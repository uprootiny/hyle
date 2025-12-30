//! TUI and interactive components
//!
//! Features:
//! - API key prompting
//! - Fuzzy model picker
//! - Interactive chat loop
//! - Telemetry display
//! - Kill/throttle/fullspeed controls

#![allow(dead_code)] // UI has forward-looking features

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap, Tabs},
};
use std::io::{self, Write};
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::mpsc;

use crate::client::{self, StreamEvent};
use crate::models::Model;
use crate::project::{Project, ProjectType};
use crate::session::Session;
use crate::skills::{is_slash_command, execute_slash_command_with_context, SlashContext};
use crate::telemetry::{Telemetry, ThrottleMode, PressureLevel};
use crate::traces::Traces;
use crate::tools::{ToolCallTracker, ToolExecutor, ToolCallDisplay};
use crate::agent::{parse_tool_calls, execute_tool_calls, format_tool_results};
use crate::eval::ModelTracker;
use crate::intent::{IntentStack, IntentView, Verbosity};
use crate::cognitive::{
    CognitiveConfig, LoopDecision, Momentum, StuckDetector,
    SalienceContext, SalienceTier, ContextCategory, extract_keywords,
};

// ═══════════════════════════════════════════════════════════════
// API KEY PROMPT
// ═══════════════════════════════════════════════════════════════

/// Prompt user for API key (masked input)
pub fn prompt_api_key() -> Result<String> {
    print!("Enter OpenRouter API key: ");
    io::stdout().flush()?;

    // Read with echo disabled
    enable_raw_mode()?;
    let mut key = String::new();

    loop {
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(k) = event::read()? {
                if k.kind != KeyEventKind::Press {
                    continue;
                }
                match k.code {
                    KeyCode::Enter => break,
                    KeyCode::Char(c) => {
                        key.push(c);
                        print!("*");
                        io::stdout().flush()?;
                    }
                    KeyCode::Backspace => {
                        if key.pop().is_some() {
                            print!("\x08 \x08");
                            io::stdout().flush()?;
                        }
                    }
                    KeyCode::Esc => {
                        disable_raw_mode()?;
                        anyhow::bail!("Cancelled");
                    }
                    _ => {}
                }
            }
        }
    }

    disable_raw_mode()?;
    println!();

    if key.is_empty() {
        anyhow::bail!("No API key entered");
    }

    Ok(key)
}

// ═══════════════════════════════════════════════════════════════
// MODEL PICKER
// ═══════════════════════════════════════════════════════════════

/// Pick a model from list with fuzzy search
pub fn pick_model(models: &[Model]) -> Result<String> {
    let mut terminal = setup_terminal()?;
    let result = run_picker(&mut terminal, models);
    restore_terminal(terminal)?;
    result
}

fn run_picker(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, models: &[Model]) -> Result<String> {
    let matcher = SkimMatcherV2::default();
    let mut filter = String::new();
    let mut list_state = ListState::default();
    list_state.select(Some(0));

    loop {
        // Filter models
        let filtered: Vec<_> = if filter.is_empty() {
            models.iter().collect()
        } else {
            let mut scored: Vec<_> = models.iter()
                .filter_map(|m| {
                    matcher.fuzzy_match(&m.id, &filter).map(|score| (m, score))
                })
                .collect();
            scored.sort_by(|a, b| b.1.cmp(&a.1));
            scored.into_iter().map(|(m, _)| m).collect()
        };

        // Clamp selection
        if let Some(selected) = list_state.selected() {
            if selected >= filtered.len() {
                list_state.select(Some(filtered.len().saturating_sub(1)));
            }
        }

        // Render
        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Min(5),
                    Constraint::Length(1),
                ])
                .split(f.size());

            // Filter input
            let input = Paragraph::new(filter.as_str())
                .block(Block::default().borders(Borders::ALL).title("Search models"));
            f.render_widget(input, chunks[0]);

            // Model list
            let items: Vec<ListItem> = filtered.iter().map(|m| {
                let ctx = format!("{}k", m.context_length / 1000);
                let free = if m.is_free() { " [FREE]" } else { "" };
                ListItem::new(format!("{} ({}){}", m.id, ctx, free))
            }).collect();

            let list = List::new(items)
                .block(Block::default().borders(Borders::ALL).title(format!("Models ({}/{})", filtered.len(), models.len())))
                .highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD))
                .highlight_symbol("> ");
            f.render_stateful_widget(list, chunks[1], &mut list_state);

            // Help
            let help = Paragraph::new("Enter: select | Esc: cancel | Type to filter")
                .style(Style::default().fg(Color::DarkGray));
            f.render_widget(help, chunks[2]);
        })?;

        // Handle input
        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }

                match key.code {
                    KeyCode::Esc => anyhow::bail!("Cancelled"),
                    KeyCode::Enter => {
                        if let Some(idx) = list_state.selected() {
                            if let Some(model) = filtered.get(idx) {
                                return Ok(model.id.clone());
                            }
                        }
                    }
                    KeyCode::Up => {
                        let i = list_state.selected().unwrap_or(0);
                        list_state.select(Some(i.saturating_sub(1)));
                    }
                    KeyCode::Down => {
                        let i = list_state.selected().unwrap_or(0);
                        list_state.select(Some((i + 1).min(filtered.len().saturating_sub(1))));
                    }
                    KeyCode::Char(c) => {
                        filter.push(c);
                        list_state.select(Some(0));
                    }
                    KeyCode::Backspace => {
                        filter.pop();
                        list_state.select(Some(0));
                    }
                    _ => {}
                }
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// MAIN TUI
// ═══════════════════════════════════════════════════════════════

/// Main view selection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum View {
    Chat,
    Telemetry,
    Log,
    Sessions,   // Backburners + foreign sessions
    Prompts,    // Prompt inventory
    Git,        // Git navigation
    Artifacts,  // Generated files, diffs
    Plans,      // Task plans
}

impl View {
    fn all() -> &'static [View] {
        &[View::Chat, View::Telemetry, View::Log, View::Sessions, View::Prompts, View::Git, View::Artifacts, View::Plans]
    }

    fn main_views() -> &'static [View] {
        &[View::Chat, View::Telemetry, View::Log, View::Sessions]
    }

    fn name(&self) -> &'static str {
        match self {
            View::Chat => "Chat",
            View::Telemetry => "Telem",
            View::Log => "Log",
            View::Sessions => "Sessions",
            View::Prompts => "Prompts",
            View::Git => "Git",
            View::Artifacts => "Artifacts",
            View::Plans => "Plans",
        }
    }

    fn is_overlay(&self) -> bool {
        matches!(self, View::Prompts | View::Git | View::Artifacts | View::Plans)
    }
}

/// Exit state for Ctrl-C handling
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExitState {
    Running,
    WarnOnce,  // First Ctrl-C pressed, show warning
    Exiting,   // Second Ctrl-C, actually exit
}

// Keep Tab as alias for backward compatibility in rendering
type Tab = View;

/// TUI messages from background tasks
enum TuiMsg {
    Token(String),
    Done(client::TokenUsage),
    Error(String),
    /// Continue agentic loop with tool results
    ContinueLoop { results: String, iteration: u8 },
    /// Agent mode events
    AgentStatus(String),
    AgentToolExecuting { name: String },
    AgentToolDone { name: String, success: bool, output: String },
    AgentIterationDone { iteration: usize, tools: usize },
    AgentComplete { iterations: usize, success: bool },
    /// Tool execution completed (non-blocking path)
    ToolsComplete { feedback: String },
}

/// Main TUI state
struct TuiState {
    tab: Tab,
    input: String,
    cursor_pos: usize, // Cursor position within input
    output: Vec<String>,
    log: Vec<String>,
    telemetry: Telemetry,
    traces: Traces,
    throttle: ThrottleMode,
    is_generating: bool,
    tick: usize,
    request_start: std::time::Instant,

    // Token stats
    prompt_tokens: u32,
    completion_tokens: u32,
    tokens_per_sec: f32,
    last_token_time: std::time::Instant,
    ttft: Option<Duration>, // Time to first token

    // Current response for session saving
    current_response: String,

    // Scroll state for long conversations
    scroll_offset: u16,
    auto_scroll: bool,
    output_line_count: usize, // Cached line count for efficiency
    output_cache: String,     // Cached joined output for rendering
    output_dirty: bool,       // Flag to rebuild cache

    // Prompt history (separate from conversation)
    prompt_history: Vec<String>,
    history_index: Option<usize>,
    saved_input: String, // Save current input when browsing history

    // Exit state for Ctrl-C handling
    exit_state: ExitState,
    exit_warn_time: Option<std::time::Instant>,

    // View navigation stack (for Esc zoom-out)
    view_stack: Vec<View>,

    // Data for overlay views
    git_status: Vec<String>,
    git_selected: usize,
    artifacts: Vec<Artifact>,
    artifact_selected: usize,
    plans: Vec<Plan>,
    plan_selected: usize,
    prompt_selected: usize,

    // Sessions view data
    detected_sessions: Vec<DetectedSession>,
    session_selected: usize,

    // Tool execution
    tool_tracker: ToolCallTracker,
    tool_executor: ToolExecutor,
    executing_tools: bool,

    // Model quality tracking
    model_tracker: ModelTracker,
    last_prompt: String,

    // Project context for LLM
    project: Option<Project>,

    // Agentic loop state
    loop_iteration: u8,
    max_iterations: u8,

    // Multi-granularity intent tracking
    intent_stack: IntentStack,
    intent_view: IntentView,

    // Cognitive architecture state
    cognitive_config: CognitiveConfig,
    momentum: Momentum,
    stuck_detector: StuckDetector,

    // Salience-aware context
    salience_keywords: Vec<String>,
    focus_files: Vec<String>,

    // Model management for auto-switch on rate limit
    current_model: String,
    rate_limited_models: Vec<String>,
    api_key: String,
    rate_limit_pending: bool, // True when we hit rate limit - ESC should offer model switch
    pending_retry: bool,      // True when we should retry last prompt with new model
    session_cost: f64,        // Running cost for this session (in $)

    // Agent mode - autonomous tool chaining like Claude Code
    agent_mode: bool,
    agent_running: bool,
}

/// An artifact (file, diff, etc.)
#[derive(Debug, Clone)]
struct Artifact {
    name: String,
    kind: String, // "file", "diff", "log"
    path: Option<String>,
    preview: String,
}

/// A plan/task
#[derive(Debug, Clone)]
struct Plan {
    name: String,
    status: String, // "pending", "done", "in_progress"
    steps: Vec<String>,
}

/// A detected session (hyle or foreign)
#[derive(Debug, Clone)]
struct DetectedSession {
    id: String,
    tool: String,           // "hyle", "claude-code", "aider", "codex", etc.
    status: SessionStatus,
    age: String,
    tokens: u64,
    messages: usize,
    integration: Integration,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum SessionStatus {
    Active,      // Currently running
    Backburner,  // Running in background
    Cold,        // Stale, can be revived
    Foreign,     // Different tool
}

#[derive(Debug, Clone, Copy)]
enum Integration {
    Full,     // Full hyle integration
    Partial,  // Some features work
    ReadOnly, // Can view but not control
}

/// Free models to fall back to on rate limit
const FREE_MODEL_FALLBACKS: &[&str] = &[
    "meta-llama/llama-3.2-3b-instruct:free",
    "google/gemma-2-9b-it:free",
    "qwen/qwen-2-7b-instruct:free",
    "mistralai/mistral-7b-instruct:free",
    "microsoft/phi-3-mini-128k-instruct:free",
];

impl TuiState {
    fn new(context_window: u32, project: Option<Project>, model: &str, api_key: &str) -> Self {
        let welcome = if let Some(ref p) = project {
            format!("hyle: {} ({} files, {} lines). Ctrl-C×2 quit, Esc zoom-out.",
                p.name, p.files.len(), p.total_lines())
        } else {
            "Welcome to hyle. Press Ctrl-C twice to quit, Esc to zoom out.".into()
        };

        Self {
            tab: Tab::Chat,
            input: String::new(),
            cursor_pos: 0,
            output: vec![welcome],
            log: Vec::new(),
            telemetry: Telemetry::new(60, 4), // 60 second window, 4Hz
            traces: Traces::new(context_window),
            throttle: ThrottleMode::Normal,
            is_generating: false,
            tick: 0,
            request_start: std::time::Instant::now(),
            prompt_tokens: 0,
            completion_tokens: 0,
            tokens_per_sec: 0.0,
            last_token_time: std::time::Instant::now(),
            ttft: None,
            current_response: String::new(),
            scroll_offset: 0,
            auto_scroll: true,
            output_line_count: 1,
            output_cache: String::new(),
            output_dirty: true,
            prompt_history: Vec::new(),
            history_index: None,
            saved_input: String::new(),
            exit_state: ExitState::Running,
            exit_warn_time: None,
            view_stack: vec![],
            git_status: vec![],
            git_selected: 0,
            artifacts: vec![],
            artifact_selected: 0,
            plans: vec![],
            plan_selected: 0,
            prompt_selected: 0,
            detected_sessions: vec![],
            session_selected: 0,
            tool_tracker: ToolCallTracker::new(),
            tool_executor: ToolExecutor::new(),
            executing_tools: false,
            model_tracker: ModelTracker::new(),
            last_prompt: String::new(),
            project,
            loop_iteration: 0,
            max_iterations: 10, // Prevent runaway loops
            // Multi-granularity intent tracking
            intent_stack: IntentStack::new(),
            intent_view: IntentView::default(),
            // Cognitive architecture
            cognitive_config: CognitiveConfig::default(),
            momentum: Momentum::default(),
            stuck_detector: StuckDetector::default(),
            // Salience tracking
            salience_keywords: Vec::new(),
            focus_files: Vec::new(),
            // Model management
            current_model: model.to_string(),
            rate_limited_models: Vec::new(),
            api_key: api_key.to_string(),
            rate_limit_pending: false,
            pending_retry: false,
            session_cost: 0.0,
            // Agent mode
            agent_mode: true, // Enable by default - this is what makes hyle like Claude Code
            agent_running: false,
        }
    }

    /// Switch to the next available free model
    fn switch_to_next_model(&mut self) -> Option<String> {
        // Add current model to rate-limited list
        if !self.rate_limited_models.contains(&self.current_model) {
            self.rate_limited_models.push(self.current_model.clone());
        }

        // Find next available model
        for model in FREE_MODEL_FALLBACKS {
            if !self.rate_limited_models.contains(&model.to_string()) {
                let old = self.current_model.clone();
                self.current_model = model.to_string();
                self.log(format!("Rate limited on {}. Switching to {}", old, model));
                return Some(model.to_string());
            }
        }

        None // All models exhausted
    }

    /// Check if error is a rate limit and handle it
    /// Returns (handled, should_retry)
    fn handle_rate_limit_error(&mut self, error: &str) -> (bool, bool) {
        if error.contains("429") || error.to_lowercase().contains("rate") ||
           error.to_lowercase().contains("too many requests") {
            self.rate_limit_pending = true;

            if let Some(new_model) = self.switch_to_next_model() {
                self.output.push(format!("\n[Rate limited on {}. Auto-switching to {}]",
                    self.rate_limited_models.last().unwrap_or(&"?".to_string()),
                    new_model));
                self.output.push("[Press ESC to select a different model, or wait to retry...]".into());
                self.rate_limit_pending = false; // Switched, no longer pending
                return (true, true); // Handled, should retry
            } else {
                self.output.push("\n[All free models rate limited. Press ESC to pick a different model.]".into());
                return (true, false); // Handled, but no retry - user must pick
            }
        }
        (false, false)
    }

    /// Clear rate limit state (when user selects new model or succeeds)
    fn clear_rate_limit(&mut self) {
        self.rate_limit_pending = false;
    }

    /// Scan for sessions (hyle and foreign)
    fn refresh_sessions(&mut self) {
        self.detected_sessions.clear();

        // Scan hyle sessions
        if let Ok(sessions) = crate::session::list_sessions() {
            for s in sessions.iter().take(20) {
                let age = chrono::Utc::now() - s.updated_at;
                let age_str = if age.num_hours() < 1 {
                    format!("{}m", age.num_minutes())
                } else if age.num_days() < 1 {
                    format!("{}h", age.num_hours())
                } else {
                    format!("{}d", age.num_days())
                };

                let status = if age.num_hours() < 1 {
                    SessionStatus::Active
                } else if age.num_days() < 1 {
                    SessionStatus::Backburner
                } else {
                    SessionStatus::Cold
                };

                self.detected_sessions.push(DetectedSession {
                    id: s.id.clone(),
                    tool: "hyle".into(),
                    status,
                    age: age_str,
                    tokens: s.total_tokens,
                    messages: s.message_count,
                    integration: Integration::Full,
                });
            }
        }

        // Scan for foreign sessions (claude code, aider, etc.)
        self.scan_foreign_sessions();
    }

    fn scan_foreign_sessions(&mut self) {
        // Claude Code sessions (~/.claude/)
        if let Some(home) = dirs::home_dir() {
            let claude_dir = home.join(".claude");
            if claude_dir.exists() {
                if let Ok(entries) = std::fs::read_dir(&claude_dir) {
                    for entry in entries.filter_map(|e| e.ok()).take(5) {
                        if entry.path().is_dir() {
                            self.detected_sessions.push(DetectedSession {
                                id: entry.file_name().to_string_lossy().to_string(),
                                tool: "claude-code".into(),
                                status: SessionStatus::Foreign,
                                age: "?".into(),
                                tokens: 0,
                                messages: 0,
                                integration: Integration::ReadOnly,
                            });
                        }
                    }
                }
            }

            // Aider sessions
            let aider_dir = home.join(".aider");
            if aider_dir.exists() {
                self.detected_sessions.push(DetectedSession {
                    id: "aider-history".into(),
                    tool: "aider".into(),
                    status: SessionStatus::Foreign,
                    age: "?".into(),
                    tokens: 0,
                    messages: 0,
                    integration: Integration::ReadOnly,
                });
            }
        }
    }

    /// Push a view onto the stack (for zoom-in navigation)
    fn push_view(&mut self, view: View) {
        self.view_stack.push(self.tab);
        self.tab = view;
    }

    /// Pop view from stack (zoom-out with Esc)
    fn pop_view(&mut self) -> bool {
        if let Some(prev) = self.view_stack.pop() {
            self.tab = prev;
            true
        } else {
            false
        }
    }

    /// Check if we're in an overlay view
    fn in_overlay(&self) -> bool {
        self.tab.is_overlay()
    }

    /// Process response for tool calls, execute them, return feedback
    fn process_tool_calls(&mut self, response: &str) -> Option<String> {
        let calls = parse_tool_calls(response);
        if calls.is_empty() {
            return None;
        }

        self.executing_tools = true;
        self.log(format!("Executing {} tool call(s)...", calls.len()));

        // Execute all tool calls
        let results = execute_tool_calls(&calls, &mut self.tool_executor, &mut self.tool_tracker);

        // Collect indices for formatting
        let indices: Vec<usize> = results.iter().map(|(idx, _)| *idx).collect();

        // Log results
        for (idx, result) in &results {
            if let Some(call) = self.tool_tracker.get(*idx) {
                let display = ToolCallDisplay::new(call).with_tick(self.tick);
                self.output.push(format!("  {}", display.header()));

                if result.is_err() {
                    self.log(format!("Tool {} failed", call.name));
                }
            }
        }
        self.mark_dirty();

        self.executing_tools = false;

        // Format results for LLM feedback
        Some(format_tool_results(&self.tool_tracker, &indices))
    }

    /// Get tool status for status bar
    fn tool_status(&self) -> String {
        self.tool_tracker.status_summary(self.tick)
    }

    /// Get project type as string for slash commands
    fn project_type_str(&self) -> Option<&'static str> {
        self.project.as_ref().map(|p| match p.project_type {
            ProjectType::Rust => "Rust",
            ProjectType::Node => "Node.js",
            ProjectType::Python => "Python",
            ProjectType::Go => "Go",
            ProjectType::Unknown => "Unknown",
        })
    }

    /// Handle Ctrl-C - returns true if should exit
    fn handle_ctrl_c(&mut self) -> bool {
        match self.exit_state {
            ExitState::Running => {
                self.exit_state = ExitState::WarnOnce;
                self.exit_warn_time = Some(std::time::Instant::now());
                self.log("Press Ctrl-C again to quit (session will be saved)");
                false
            }
            ExitState::WarnOnce => {
                self.exit_state = ExitState::Exiting;
                true
            }
            ExitState::Exiting => true,
        }
    }

    /// Reset exit warning after timeout
    fn check_exit_timeout(&mut self) {
        if let Some(warn_time) = self.exit_warn_time {
            if warn_time.elapsed() > Duration::from_secs(3) {
                self.exit_state = ExitState::Running;
                self.exit_warn_time = None;
            }
        }
    }

    /// Refresh git status
    fn refresh_git_status(&mut self) {
        if let Ok(output) = std::process::Command::new("git")
            .args(["status", "--porcelain", "-b"])
            .output()
        {
            self.git_status = String::from_utf8_lossy(&output.stdout)
                .lines()
                .map(|s| s.to_string())
                .collect();
        }
    }

    /// Add prompt to history (dedup consecutive)
    fn add_to_history(&mut self, prompt: &str) {
        if prompt.trim().is_empty() {
            return;
        }
        // Don't add if same as last
        if self.prompt_history.last().map(|s| s.as_str()) != Some(prompt) {
            self.prompt_history.push(prompt.to_string());
            // Keep last 100 prompts
            if self.prompt_history.len() > 100 {
                self.prompt_history.remove(0);
            }
        }
        self.history_index = None;
    }

    /// Navigate history up
    fn history_up(&mut self) {
        if self.prompt_history.is_empty() {
            return;
        }

        match self.history_index {
            None => {
                // Save current input and go to last history item
                self.saved_input = self.input.clone();
                self.history_index = Some(self.prompt_history.len() - 1);
                self.input = self.prompt_history.last().unwrap().clone();
                self.cursor_pos = self.input.len();
            }
            Some(0) => {
                // Already at oldest
            }
            Some(i) => {
                self.history_index = Some(i - 1);
                self.input = self.prompt_history[i - 1].clone();
                self.cursor_pos = self.input.len();
            }
        }
    }

    /// Navigate history down
    fn history_down(&mut self) {
        match self.history_index {
            None => {}
            Some(i) if i >= self.prompt_history.len() - 1 => {
                // Restore saved input
                self.history_index = None;
                self.input = std::mem::take(&mut self.saved_input);
                self.cursor_pos = self.input.len();
            }
            Some(i) => {
                self.history_index = Some(i + 1);
                self.input = self.prompt_history[i + 1].clone();
                self.cursor_pos = self.input.len();
            }
        }
    }

    /// Trim output buffer if too large (keep last 1000 lines)
    fn trim_output_buffer(&mut self) {
        const MAX_LINES: usize = 1000;
        if self.output.len() > MAX_LINES {
            let trim = self.output.len() - MAX_LINES;
            self.output.drain(0..trim);
            // Adjust scroll offset
            self.scroll_offset = self.scroll_offset.saturating_sub(trim as u16);
        }
    }

    /// Update line count cache
    fn update_line_count(&mut self) {
        self.output_line_count = self.output.iter()
            .map(|s| s.lines().count().max(1))
            .sum();
    }

    /// Mark output as dirty (needs cache rebuild)
    fn mark_dirty(&mut self) {
        self.output_dirty = true;
    }

    /// Rebuild output cache if dirty, return cached text
    fn get_output_text(&mut self) -> &str {
        if self.output_dirty {
            self.output_cache = self.output.join("\n");
            self.update_line_count();
            self.output_dirty = false;
        }
        &self.output_cache
    }

    /// Append to output with dirty marking
    fn append_output(&mut self, line: String) {
        self.output.push(line);
        self.mark_dirty();
        self.trim_output_buffer();
    }

    /// Append to last line (for streaming tokens) - incremental update
    fn append_to_last(&mut self, text: &str) {
        if let Some(last) = self.output.last_mut() {
            last.push_str(text);
            // Incremental cache update: just append to cache instead of full rebuild
            if !self.output_dirty {
                self.output_cache.push_str(text);
            }
            // Don't mark dirty - we updated incrementally
        }
    }

    /// Scroll to bottom
    fn scroll_to_bottom(&mut self, visible_height: u16) {
        let total = self.output_line_count as u16;
        if total > visible_height {
            self.scroll_offset = total - visible_height;
        } else {
            self.scroll_offset = 0;
        }
    }

    fn log(&mut self, msg: impl Into<String>) {
        let now = chrono::Local::now().format("%H:%M:%S");
        self.log.push(format!("[{}] {}", now, msg.into()));
    }

    // === COGNITIVE ARCHITECTURE METHODS ===

    /// Update intent from user prompt
    fn update_intent_from_prompt(&mut self, prompt: &str) {
        use crate::intent::{Intent, IntentKind};

        // Extract keywords for salience tracking
        self.salience_keywords = extract_keywords(prompt);

        // Extract file references for focus tracking
        self.focus_files = prompt.split_whitespace()
            .filter(|w| w.contains('.') && (
                w.ends_with(".rs") || w.ends_with(".py") || w.ends_with(".js") ||
                w.ends_with(".ts") || w.ends_with(".go") || w.ends_with(".md") ||
                w.ends_with(".json") || w.ends_with(".toml") || w.ends_with(".yaml")
            ))
            .map(|s| s.to_string())
            .collect();

        // Simple heuristic: check if this looks like a new task or continuation
        let prompt_lower = prompt.to_lowercase();

        // Detect intent type
        if prompt_lower.starts_with("fix ") || prompt_lower.contains("bug") || prompt_lower.contains("error") {
            let intent = Intent::new(&prompt[..prompt.len().min(100)], IntentKind::Fix);
            self.intent_stack.push(intent);
        } else if prompt_lower.starts_with("also ") || prompt_lower.starts_with("and ") {
            // Continuation of existing task
            if let Some(active) = self.intent_stack.active() {
                let intent = Intent::subtask(&prompt[..prompt.len().min(100)], &active.id);
                self.intent_stack.push(intent);
            }
        } else if self.intent_stack.is_empty() || prompt.len() > 50 {
            // New primary intent
            let intent = Intent::primary(&prompt[..prompt.len().min(100)]);
            self.intent_stack.push(intent);
        }

        // Update the view
        self.intent_view = IntentView::from_stack(&self.intent_stack);
    }

    /// Assess whether to continue the agentic loop
    fn should_continue_loop(&self, tool_results: &str) -> LoopDecision {
        use crate::cognitive::LoopDecision;

        // Check iteration limit
        if self.loop_iteration >= self.max_iterations {
            return LoopDecision::MaxIterations;
        }

        // Check if stuck
        if self.stuck_detector.is_stuck() {
            return LoopDecision::Stuck {
                reason: "Repeated actions or errors detected".into(),
                suggestions: vec![
                    "Try a different approach".into(),
                    "Break down the task into smaller steps".into(),
                ],
            };
        }

        // Check momentum
        if self.momentum.should_pause() {
            return LoopDecision::PauseConcern {
                reason: "Multiple tool failures detected".into(),
            };
        }

        // Check for completion signals in results
        if tool_results.contains("Task complete") ||
           tool_results.contains("No more changes needed") ||
           tool_results.contains("All done") {
            return LoopDecision::Complete {
                summary: "Task appears complete based on tool results".into(),
            };
        }

        LoopDecision::Continue
    }

    /// Record tool execution outcome for momentum tracking
    fn record_tool_outcome(&mut self, tool_name: &str, success: bool, was_useful: bool) {
        use crate::cognitive::ToolOutcome;
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        // Record in momentum
        self.momentum.record(ToolOutcome {
            tool_name: tool_name.to_string(),
            success,
            was_useful,
        });

        // Record in stuck detector
        let mut hasher = DefaultHasher::new();
        tool_name.hash(&mut hasher);
        self.stuck_detector.record_action(hasher.finish());

        if !success {
            self.stuck_detector.record_error(tool_name);
        }

        if was_useful {
            self.stuck_detector.record_change();
        } else {
            self.stuck_detector.record_no_change();
        }
    }

    /// Get context for LLM with intent info
    fn get_llm_context(&self) -> String {
        let mut ctx = String::new();

        // Add intent view at appropriate verbosity based on loop iteration
        let verbosity = if self.loop_iteration == 0 {
            Verbosity::Full  // First iteration: full context
        } else if self.loop_iteration < 3 {
            Verbosity::Normal
        } else {
            Verbosity::Minimal  // Later iterations: minimal
        };

        ctx.push_str(&self.intent_view.for_llm(verbosity));
        ctx
    }

    /// Build salience-aware context from conversation history
    /// Returns context string with most salient items in full detail
    fn build_salient_context(&self, messages: &[serde_json::Value], budget_tokens: usize) -> String {
        let mut salience = SalienceContext::new(budget_tokens);
        salience.set_keywords(self.salience_keywords.clone());
        salience.set_focus_files(self.focus_files.clone());

        // Process messages from oldest to newest, assigning age
        let total = messages.len();
        for (i, msg) in messages.iter().enumerate() {
            let age = (total - i - 1) as u32;
            let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("unknown");
            let content = msg.get("content").and_then(|c| c.as_str()).unwrap_or("");

            let category = match role {
                "system" => ContextCategory::SystemPrompt,
                "user" => ContextCategory::UserMessage,
                "assistant" => {
                    // Check if it contains tool calls or errors
                    if content.contains("```tool") || content.contains("<tool") {
                        ContextCategory::ToolCall
                    } else if content.to_lowercase().contains("error") {
                        ContextCategory::Error
                    } else {
                        ContextCategory::AssistantResponse
                    }
                }
                _ => ContextCategory::Summary,
            };

            // System prompt always at focus tier
            if role == "system" {
                salience.add_with_tier(content.to_string(), category, SalienceTier::Focus);
            } else {
                salience.add(content.to_string(), category, age);
            }
        }

        // Add current intent as high-salience context
        let intent_ctx = self.intent_view.for_llm(Verbosity::Normal);
        if !intent_ctx.is_empty() {
            salience.add_with_tier(
                format!("[Current Focus]\n{}", intent_ctx),
                ContextCategory::Intent,
                SalienceTier::Focus
            );
        }

        // Stats available for debugging
        let _stats = salience.stats();

        salience.build()
    }

    /// Get salience stats for display
    fn salience_stats(&self, messages: &[serde_json::Value]) -> String {
        let mut salience = SalienceContext::new(4000);
        salience.set_keywords(self.salience_keywords.clone());

        for (i, msg) in messages.iter().enumerate() {
            let age = (messages.len() - i - 1) as u32;
            let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("unknown");
            let content = msg.get("content").and_then(|c| c.as_str()).unwrap_or("");

            let category = match role {
                "system" => ContextCategory::SystemPrompt,
                "user" => ContextCategory::UserMessage,
                "assistant" => ContextCategory::AssistantResponse,
                _ => ContextCategory::Summary,
            };

            salience.add(content.to_string(), category, age);
        }

        format!("{}", salience.stats())
    }
}

/// Run the main TUI
pub async fn run_tui(
    api_key: &str,
    model: &str,
    paths: Vec<PathBuf>,
    resume: bool,
    project: Option<Project>,
    claude_context: Option<Vec<crate::session::Message>>,
) -> Result<()> {
    let mut terminal = setup_terminal()?;
    let result = run_tui_loop(&mut terminal, api_key, model, paths, resume, project, claude_context).await;
    restore_terminal(terminal)?;
    result
}

async fn run_tui_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    api_key: &str,
    model: &str,
    _paths: Vec<PathBuf>,
    resume: bool,
    project: Option<Project>,
    claude_context: Option<Vec<crate::session::Message>>,
) -> Result<()> {
    // Get context window for this model
    let context_window = crate::models::get_context_window(model);
    let mut state = TuiState::new(context_window, project, model, api_key);

    // Load or create session
    let mut session = if resume {
        match Session::load_or_create(model) {
            Ok(s) => {
                if s.messages.len() > 1 {
                    state.log(format!("Resumed session {} ({} messages)", s.meta.id, s.messages.len()));
                    // Restore conversation to output
                    for msg in &s.messages {
                        if msg.role == "user" {
                            state.output.push(format!("> {}", msg.content));
                        } else if msg.role == "assistant" {
                            state.output.push(format!("  {}", msg.content.lines().next().unwrap_or("")));
                            if msg.content.lines().count() > 1 {
                                state.output.push("  ...".into());
                            }
                        }
                    }
                    state.mark_dirty();
                }
                s
            }
            Err(e) => {
                state.log(format!("Failed to resume: {}", e));
                Session::new(model)?
            }
        }
    } else {
        Session::new(model)?
    };

    // Inject Claude Code context if available
    if let Some(claude_msgs) = claude_context {
        if !claude_msgs.is_empty() {
            state.log(format!("Imported {} prompts from Claude Code session", claude_msgs.len()));
            state.output.push("─── Imported Claude Context ───".into());
            for msg in claude_msgs {
                // Add to session for API context
                session.add_message(msg.clone())?;
                // Show in output (truncated)
                let display = if msg.content.len() > 60 {
                    format!("> {}...", &msg.content[..60])
                } else {
                    format!("> {}", msg.content)
                };
                state.output.push(display);
            }
            state.output.push("───────────────────────────────".into());
            state.mark_dirty();
        }
    }

    state.log(format!("Model: {} ({}k ctx)", model, context_window / 1000));
    state.model_tracker.set_model(model);

    // Load existing sessions on startup
    state.refresh_sessions();

    let (tx, mut rx) = mpsc::channel::<TuiMsg>(256);

    // Telemetry sampling interval
    let mut last_telemetry = std::time::Instant::now();

    loop {
        state.tick += 1;

        // Sample telemetry at ~4Hz
        if last_telemetry.elapsed() >= Duration::from_millis(250) {
            state.telemetry.sample();
            state.traces.memory.sample();
            last_telemetry = std::time::Instant::now();

            // Auto-throttle on high pressure
            if state.telemetry.pressure() == PressureLevel::Critical && state.throttle == ThrottleMode::Normal {
                state.throttle = ThrottleMode::Throttled;
                state.log("Auto-throttled due to high CPU pressure");
            }
        }

        // Handle pending retry after model switch
        if state.pending_retry && !state.last_prompt.is_empty() {
            state.pending_retry = false;
            state.output.push(format!("[Retrying with {}...]", state.current_model));
            state.mark_dirty();

            // Spawn retry API call
            let tx = tx.clone();
            let api_key = state.api_key.clone();
            let model = state.current_model.clone();
            let project_clone = state.project.clone();
            let history = session.messages_for_api();
            let prompt = state.last_prompt.clone();

            tokio::spawn(async move {
                match client::stream_completion_full(&api_key, &model, &prompt, project_clone.as_ref(), &history).await {
                    Ok(mut stream) => {
                        while let Some(event) = stream.recv().await {
                            match event {
                                StreamEvent::Token(t) => {
                                    let _ = tx.send(TuiMsg::Token(t)).await;
                                }
                                StreamEvent::Done(u) => {
                                    let _ = tx.send(TuiMsg::Done(u)).await;
                                }
                                StreamEvent::Error(e) => {
                                    let _ = tx.send(TuiMsg::Error(e)).await;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(TuiMsg::Error(e.to_string())).await;
                    }
                }
            });
        }

        // Check for API responses
        while let Ok(msg) = rx.try_recv() {
            match msg {
                TuiMsg::Token(t) => {
                    // Record time to first token
                    if state.ttft.is_none() {
                        let ttft = state.request_start.elapsed();
                        state.ttft = Some(ttft);
                        state.traces.latency.record_ttft(ttft);
                    }

                    // Update tokens/sec estimate
                    let elapsed = state.last_token_time.elapsed().as_secs_f32();
                    if elapsed > 0.0 {
                        state.tokens_per_sec = 1.0 / elapsed;
                    }
                    state.last_token_time = std::time::Instant::now();

                    // Append to output and accumulate response
                    state.current_response.push_str(&t);
                    // Use incremental update to avoid full cache rebuild per token
                    state.append_to_last(&t);
                }
                TuiMsg::Done(usage) => {
                    state.is_generating = false;
                    state.prompt_tokens = usage.prompt_tokens;
                    state.completion_tokens = usage.completion_tokens;

                    // Calculate and accumulate cost
                    let request_cost = crate::models::calculate_cost(
                        &state.current_model,
                        usage.prompt_tokens,
                        usage.completion_tokens
                    );
                    state.session_cost += request_cost;

                    // Record traces
                    let duration = state.request_start.elapsed();
                    state.traces.latency.record_total(duration);
                    state.traces.tokens.record(
                        usage.prompt_tokens,
                        usage.completion_tokens,
                        duration.as_secs_f64()
                    );
                    state.traces.context.record(usage.prompt_tokens);

                    // Evaluate response quality
                    if !state.current_response.is_empty() && !state.last_prompt.is_empty() {
                        state.model_tracker.record_response(
                            &state.last_prompt,
                            &state.current_response,
                            usage.completion_tokens as u64
                        );

                        if let Some(stats) = state.model_tracker.current_stats() {
                            if stats.should_switch() {
                                state.log(format!(
                                    "Quality warning: avg={:.2}, {} consecutive low scores",
                                    stats.average_quality, stats.consecutive_failures
                                ));
                            }
                        }
                    }

                    // Save assistant message to session
                    if !state.current_response.is_empty() {
                        if let Err(e) = session.add_assistant_message(
                            &state.current_response,
                            Some(usage.completion_tokens)
                        ) {
                            state.log(format!("Session save error: {}", e));
                        }

                        // Check for tool calls - spawn execution in background to avoid blocking
                        let response_copy = state.current_response.clone();
                        let calls = parse_tool_calls(&response_copy);
                        if !calls.is_empty() {
                            state.executing_tools = true;
                            state.output.push(format!("[Executing {} tool(s)...]", calls.len()));
                            state.mark_dirty();

                            // Spawn tool execution in blocking thread pool
                            let tx = tx.clone();
                            tokio::task::spawn_blocking(move || {
                                // Create temporary executor and tracker for this batch
                                let mut executor = ToolExecutor::new();
                                let mut tracker = ToolCallTracker::new();

                                let results = execute_tool_calls(&calls, &mut executor, &mut tracker);
                                let indices: Vec<usize> = results.iter().map(|(idx, _)| *idx).collect();
                                let feedback = format_tool_results(&tracker, &indices);

                                // Send results back to main loop
                                let rt = tokio::runtime::Handle::current();
                                rt.block_on(async {
                                    let _ = tx.send(TuiMsg::ToolsComplete { feedback }).await;
                                });
                            });
                        } else {
                            // No tool calls - reset loop counter
                            state.loop_iteration = 0;
                        }

                        state.current_response.clear();
                    }

                    // Save session metadata
                    if let Err(e) = session.save_meta() {
                        state.log(format!("Session meta save error: {}", e));
                    }

                    state.output.push(format!(
                        "\n[{} + {} = {} tokens, {:.1}s]",
                        usage.prompt_tokens, usage.completion_tokens, usage.total_tokens,
                        duration.as_secs_f64()
                    ));
                    state.mark_dirty();
                    state.log(format!("Completed: {} tokens in {:.1}s", usage.total_tokens, duration.as_secs_f64()));
                }
                TuiMsg::Error(e) => {
                    state.is_generating = false;
                    state.loop_iteration = 0; // Reset on error

                    // Check for rate limit and auto-switch
                    let (handled, should_retry) = state.handle_rate_limit_error(&e);

                    if handled {
                        state.mark_dirty();
                        if should_retry {
                            // Set flag to retry with new model
                            state.pending_retry = true;
                            state.is_generating = true; // Keep generating state
                            state.log(format!("Rate limit: switched to {}, retrying...", state.current_model));
                        } else {
                            state.log("All models rate limited. Press ESC to pick a model.");
                        }
                    } else {
                        state.output.push(format!("\n[Error: {}]", e));
                        state.mark_dirty();
                        state.log(format!("Error: {}", e));
                    }
                }
                // Agent mode events
                TuiMsg::AgentStatus(status) => {
                    state.output.push(format!("[Agent: {}]", status));
                    state.mark_dirty();
                }
                TuiMsg::AgentToolExecuting { name } => {
                    state.output.push(format!("  → Executing: {}", name));
                    state.mark_dirty();
                }
                TuiMsg::AgentToolDone { name, success, output } => {
                    let icon = if success { "✓" } else { "✗" };
                    state.output.push(format!("  {} {}", icon, name));
                    // Show first few lines of output
                    for line in output.lines().take(5) {
                        state.output.push(format!("    {}", line));
                    }
                    if output.lines().count() > 5 {
                        state.output.push("    ...".into());
                    }
                    state.mark_dirty();
                }
                TuiMsg::AgentIterationDone { iteration, tools } => {
                    state.output.push(format!("─── Iteration {} ({} tools) ───", iteration, tools));
                    state.mark_dirty();
                }
                TuiMsg::AgentComplete { iterations, success } => {
                    state.agent_running = false;
                    state.is_generating = false;
                    let status = if success { "completed" } else { "stopped" };
                    state.output.push(format!("[Agent {} after {} iterations]", status, iterations));
                    state.mark_dirty();
                }
                TuiMsg::ToolsComplete { feedback } => {
                    // Tools finished executing in background
                    state.executing_tools = false;

                    // Show tool execution results
                    state.output.push(String::new());
                    state.output.push("─── Tool Results ───".to_string());
                    for line in feedback.lines().take(20) {
                        state.output.push(format!("  {}", line));
                    }
                    state.mark_dirty();

                    // AGENTIC LOOP: Continue if we have tool results and haven't hit max iterations
                    if state.loop_iteration < state.max_iterations {
                        state.loop_iteration += 1;
                        state.log(format!("Agentic loop iteration {}/{}", state.loop_iteration, state.max_iterations));

                        // Send continuation message
                        let tx = tx.clone();
                        let tool_results = feedback.clone();
                        let iteration = state.loop_iteration;
                        tokio::spawn(async move {
                            let _ = tx.send(TuiMsg::ContinueLoop {
                                results: tool_results,
                                iteration,
                            }).await;
                        });
                    } else {
                        state.loop_iteration = 0;
                    }
                }
                TuiMsg::ContinueLoop { results, iteration } => {
                    // AGENTIC LOOP: Continue with tool results
                    state.output.push(String::new());
                    state.output.push(format!("─── Continuing (iteration {}) ───", iteration));
                    state.mark_dirty();

                    // Add tool results to session as a system message
                    let tool_msg = format!("Tool execution results:\n{}", results);
                    if let Err(e) = session.add_system_message(&tool_msg) {
                        state.log(format!("Session save error: {}", e));
                    }

                    // Use cognitive architecture for loop decision
                    let decision = state.should_continue_loop(&results);
                    match decision {
                        LoopDecision::MaxIterations => {
                            state.output.push("[Max iterations reached - pausing for input]".into());
                            state.is_generating = false;
                            state.loop_iteration = 0;
                            state.mark_dirty();
                            continue;
                        }
                        LoopDecision::Stuck { reason, suggestions } => {
                            state.output.push(format!("[Stuck: {}]", reason));
                            for s in suggestions {
                                state.output.push(format!("  - {}", s));
                            }
                            state.is_generating = false;
                            state.loop_iteration = 0;
                            state.stuck_detector.clear();
                            state.mark_dirty();
                            continue;
                        }
                        LoopDecision::PauseConcern { reason } => {
                            state.output.push(format!("[Pausing: {}]", reason));
                            state.is_generating = false;
                            state.mark_dirty();
                            continue;
                        }
                        LoopDecision::Complete { summary } => {
                            state.output.push(format!("[Complete: {}]", summary));
                            state.is_generating = false;
                            state.loop_iteration = 0;
                            // Mark active intent as completed
                            state.intent_stack.pop();
                            state.intent_view = IntentView::from_stack(&state.intent_stack);
                            state.mark_dirty();
                            continue;
                        }
                        LoopDecision::Continue => {
                            // Continue with next iteration
                        }
                        _ => {
                            // Other decisions default to continue
                        }
                    }

                    // Build dynamic continuation prompt with intent context
                    let intent_ctx = state.get_llm_context();
                    let continuation = if intent_ctx.is_empty() {
                        "Continue based on the tool results above. If the task is complete, summarize what was done. If more steps are needed, proceed with the next step.".to_string()
                    } else {
                        format!("{}\n\nContinue with the next step. If done, summarize.", intent_ctx)
                    };

                    state.output.push(format!("> {}", if continuation.len() > 60 {
                        format!("{}...", &continuation[..60])
                    } else {
                        continuation.clone()
                    }));
                    state.output.push(String::new()); // For response
                    state.is_generating = true;
                    state.ttft = None;
                    state.request_start = std::time::Instant::now();
                    state.last_token_time = std::time::Instant::now();

                    // Spawn next API call
                    let tx = tx.clone();
                    let api_key = state.api_key.clone();
                    let model = state.current_model.clone(); // Use state model, can switch on rate limit
                    let project_clone = state.project.clone();
                    let history = session.messages_for_api();
                    let cont_prompt = continuation;

                    tokio::spawn(async move {
                        match client::stream_completion_full(&api_key, &model, &cont_prompt, project_clone.as_ref(), &history).await {
                            Ok(mut stream) => {
                                while let Some(event) = stream.recv().await {
                                    match event {
                                        StreamEvent::Token(t) => {
                                            let _ = tx.send(TuiMsg::Token(t)).await;
                                        }
                                        StreamEvent::Done(u) => {
                                            let _ = tx.send(TuiMsg::Done(u)).await;
                                        }
                                        StreamEvent::Error(e) => {
                                            let _ = tx.send(TuiMsg::Error(e)).await;
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                let _ = tx.send(TuiMsg::Error(e.to_string())).await;
                            }
                        }
                    });
                }
            }
        }

        // Update cache before render (avoids allocation during draw)
        if state.output_dirty {
            state.output_cache = state.output.join("\n");
            state.update_line_count();
            state.output_dirty = false;
        }

        // Render
        terminal.draw(|f| render_tui(f, &state))?;

        // Handle input
        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }

                // Check exit timeout
                state.check_exit_timeout();

                // Global controls
                match key.code {
                    // Ctrl-C: warn first, then exit
                    KeyCode::Char('c') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                        if state.handle_ctrl_c() {
                            // Save session before exit
                            if let Err(e) = session.save_meta() {
                                state.log(format!("Session save error: {}", e));
                            }
                            break;
                        }
                    }
                    // Esc: zoom out (context-dependent)
                    KeyCode::Esc => {
                        if state.in_overlay() {
                            // Pop back from overlay view
                            state.pop_view();
                        } else if state.rate_limit_pending {
                            // We hit rate limit - show available models
                            state.output.push(String::new());
                            state.output.push("─── Available Models (use /switch <model>) ───".into());
                            for (i, m) in FREE_MODEL_FALLBACKS.iter().enumerate() {
                                let marker = if state.rate_limited_models.contains(&m.to_string()) {
                                    "✗" // Rate limited
                                } else if *m == state.current_model {
                                    "●" // Current
                                } else {
                                    " "
                                };
                                state.output.push(format!("  [{}] {}: {}", marker, i + 1, m));
                            }
                            state.output.push("─── Type /switch <name> or /switch 1-5 ───".into());
                            state.rate_limit_pending = false;
                            state.mark_dirty();
                        } else if state.input.is_empty() {
                            // Clear any selection state, but don't exit
                            state.history_index = None;
                            state.auto_scroll = true;
                        } else {
                            // Clear input
                            state.input.clear();
                            state.cursor_pos = 0;
                            state.history_index = None;
                        }
                    }
                    // Tab: cycle through main views
                    KeyCode::Tab => {
                        let views = View::main_views();
                        let idx = views.iter().position(|v| *v == state.tab).unwrap_or(0);
                        state.tab = views[(idx + 1) % views.len()];
                        state.view_stack.clear(); // Clear stack when switching tabs
                        // Auto-refresh when entering Sessions
                        if state.tab == View::Sessions {
                            state.refresh_sessions();
                        }
                    }
                    // Shift+Tab: cycle backwards
                    KeyCode::BackTab => {
                        let views = View::main_views();
                        let idx = views.iter().position(|v| *v == state.tab).unwrap_or(0);
                        state.tab = views[(idx + views.len() - 1) % views.len()];
                        state.view_stack.clear();
                        // Auto-refresh when entering Sessions
                        if state.tab == View::Sessions {
                            state.refresh_sessions();
                        }
                    }
                    // Quick access to overlay views
                    KeyCode::Char('p') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                        state.push_view(View::Prompts);
                    }
                    KeyCode::Char('g') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                        state.refresh_git_status();
                        state.push_view(View::Git);
                    }
                    KeyCode::Char('a') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                        state.push_view(View::Artifacts);
                    }
                    KeyCode::Char('k') if key.modifiers.is_empty() && state.is_generating => {
                        state.throttle = ThrottleMode::Killed;
                        state.log("Operation killed");
                    }
                    KeyCode::Char('c') if key.modifiers.is_empty() && state.telemetry.spike_snapshot.is_some() => {
                        state.telemetry.clear_spike();
                        state.log("Spike snapshot cleared");
                    }
                    KeyCode::Char('t') if key.modifiers.is_empty() => {
                        state.throttle = ThrottleMode::Throttled;
                        state.log("Switched to throttled mode");
                    }
                    KeyCode::Char('f') if key.modifiers.is_empty() && !state.is_generating => {
                        state.throttle = ThrottleMode::Full;
                        state.log("Switched to full speed mode");
                    }
                    KeyCode::Char('n') if key.modifiers.is_empty() => {
                        state.throttle = ThrottleMode::Normal;
                        state.log("Switched to normal mode");
                    }
                    // Refresh sessions with 'r' in Sessions view
                    KeyCode::Char('r') if state.tab == View::Sessions => {
                        state.refresh_sessions();
                        state.log(format!("Refreshed: {} sessions found", state.detected_sessions.len()));
                    }
                    _ => {}
                }

                // Tab-specific input
                if state.tab == Tab::Chat && !state.is_generating {
                    match key.code {
                        KeyCode::Up => {
                            state.history_up();
                        }
                        KeyCode::Down => {
                            state.history_down();
                        }
                        KeyCode::PageUp => {
                            // Scroll chat up
                            state.scroll_offset = state.scroll_offset.saturating_sub(10);
                            state.auto_scroll = false;
                        }
                        KeyCode::PageDown => {
                            // Scroll chat down
                            state.scroll_offset = state.scroll_offset.saturating_add(10);
                        }
                        KeyCode::End => {
                            // Jump to bottom, enable auto-scroll
                            state.auto_scroll = true;
                        }
                        KeyCode::Enter => {
                            if !state.input.is_empty() {
                                let prompt = state.input.clone();
                                state.add_to_history(&prompt);
                                state.input.clear();
                                state.cursor_pos = 0;
                                state.history_index = None;
                                state.output.push(format!("> {}", prompt));
                                state.mark_dirty();
                                state.auto_scroll = true;

                                // Check for slash commands first
                                if is_slash_command(&prompt) {
                                    let project_type = state.project_type_str();
                                    let ctx = SlashContext {
                                        project_type: project_type.map(|s| s.to_string()),
                                        model: state.current_model.clone(),
                                        session_id: session.meta.id.clone(),
                                        total_tokens: session.meta.total_tokens,
                                        message_count: session.messages.len(),
                                    };
                                    if let Some(result) = execute_slash_command_with_context(&prompt, project_type, Some(&ctx)) {
                                        // Handle special SWITCH_MODEL signals
                                        if result.output == "SWITCH_MODEL_PICKER" {
                                            state.output.push("─── Available Models ───".into());
                                            for (i, m) in FREE_MODEL_FALLBACKS.iter().enumerate() {
                                                let marker = if state.rate_limited_models.contains(&m.to_string()) {
                                                    "✗"
                                                } else if *m == state.current_model {
                                                    "●"
                                                } else {
                                                    " "
                                                };
                                                state.output.push(format!("  [{}] {}: {}", marker, i + 1, m));
                                            }
                                            state.output.push("Use /switch <name> or /switch 1-5".into());
                                            state.mark_dirty();
                                            continue;
                                        } else if result.output.starts_with("SWITCH_MODEL:") {
                                            let target = result.output.trim_start_matches("SWITCH_MODEL:");
                                            // Try to parse as number first
                                            let new_model = if let Ok(n) = target.parse::<usize>() {
                                                FREE_MODEL_FALLBACKS.get(n.saturating_sub(1)).map(|s| s.to_string())
                                            } else {
                                                // Find by partial match
                                                FREE_MODEL_FALLBACKS.iter()
                                                    .find(|m| m.contains(target))
                                                    .map(|s| s.to_string())
                                            };

                                            if let Some(model) = new_model {
                                                state.current_model = model.clone();
                                                state.rate_limited_models.clear(); // Clear rate limits when manually switching
                                                state.rate_limit_pending = false;
                                                state.output.push(format!("[✓] Switched to: {}", model));
                                                state.log(format!("Model switched to: {}", model));
                                            } else {
                                                state.output.push(format!("[✗] Unknown model: {}", target));
                                            }
                                            state.mark_dirty();
                                            continue;
                                        } else if result.output == "TOGGLE_AGENT_MODE" {
                                            state.agent_mode = !state.agent_mode;
                                            let mode = if state.agent_mode { "ON" } else { "OFF" };
                                            state.output.push(format!("[Agent Mode: {}]", mode));
                                            if state.agent_mode {
                                                state.output.push("  LLM will autonomously execute tools until task complete".into());
                                            } else {
                                                state.output.push("  LLM will respond without automatic tool execution".into());
                                            }
                                            state.mark_dirty();
                                            continue;
                                        }

                                        let status = if result.success { "✓" } else { "✗" };
                                        state.output.push(format!("[{}] {}", status, prompt));
                                        for line in result.output.lines().take(50) {
                                            state.output.push(format!("  {}", line));
                                        }
                                        if result.output.lines().count() > 50 {
                                            state.output.push("  ... (truncated)".into());
                                        }
                                        state.mark_dirty();
                                        state.log(format!("Slash: {} -> {}", prompt, if result.success { "ok" } else { "failed" }));
                                        continue;
                                    }
                                    // Unknown slash command falls through to LLM
                                }

                                state.output.push(String::new()); // For response
                                state.is_generating = true;
                                state.ttft = None;
                                state.request_start = std::time::Instant::now();
                                state.last_token_time = std::time::Instant::now();
                                state.log(format!("Sending: {}", &prompt[..prompt.len().min(50)]));
                                state.last_prompt = prompt.clone();

                                // Save user message to session
                                if let Err(e) = session.add_user_message(&prompt) {
                                    state.log(format!("Session save error: {}", e));
                                }

                                // Update intent tracking
                                state.update_intent_from_prompt(&prompt);
                                state.loop_iteration = 0; // New prompt resets loop counter
                                state.stuck_detector.clear(); // Clear stuck detection for new task

                                // Spawn API call with session history
                                let tx = tx.clone();
                                let api_key = state.api_key.clone();
                                let model = state.current_model.clone(); // Use state model, can switch on rate limit
                                let project_clone = state.project.clone();
                                let history = session.messages_for_api();

                                tokio::spawn(async move {
                                    match client::stream_completion_full(&api_key, &model, &prompt, project_clone.as_ref(), &history).await {
                                        Ok(mut stream) => {
                                            while let Some(event) = stream.recv().await {
                                                match event {
                                                    StreamEvent::Token(t) => {
                                                        let _ = tx.send(TuiMsg::Token(t)).await;
                                                    }
                                                    StreamEvent::Done(u) => {
                                                        let _ = tx.send(TuiMsg::Done(u)).await;
                                                    }
                                                    StreamEvent::Error(e) => {
                                                        let _ = tx.send(TuiMsg::Error(e)).await;
                                                    }
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            let _ = tx.send(TuiMsg::Error(e.to_string())).await;
                                        }
                                    }
                                });
                            }
                        }
                        // Readline: Ctrl-A = jump to start
                        KeyCode::Char('a') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                            state.cursor_pos = 0;
                        }
                        // Readline: Ctrl-E = jump to end
                        KeyCode::Char('e') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                            state.cursor_pos = state.input.len();
                        }
                        // Readline: Ctrl-K = kill to end of line
                        KeyCode::Char('k') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                            state.input.truncate(state.cursor_pos);
                        }
                        // Readline: Ctrl-U = kill to start of line
                        KeyCode::Char('u') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                            state.input = state.input[state.cursor_pos..].to_string();
                            state.cursor_pos = 0;
                        }
                        // Left arrow: move cursor left
                        KeyCode::Left => {
                            state.cursor_pos = state.cursor_pos.saturating_sub(1);
                        }
                        // Right arrow: move cursor right
                        KeyCode::Right => {
                            state.cursor_pos = (state.cursor_pos + 1).min(state.input.len());
                        }
                        // Home: jump to start
                        KeyCode::Home => {
                            state.cursor_pos = 0;
                        }
                        // Insert character at cursor position
                        KeyCode::Char(c) => {
                            state.input.insert(state.cursor_pos, c);
                            state.cursor_pos += 1;
                        }
                        // Backspace: delete char before cursor
                        KeyCode::Backspace => {
                            if state.cursor_pos > 0 {
                                state.input.remove(state.cursor_pos - 1);
                                state.cursor_pos -= 1;
                            }
                        }
                        // Delete: delete char at cursor
                        KeyCode::Delete => {
                            if state.cursor_pos < state.input.len() {
                                state.input.remove(state.cursor_pos);
                            }
                        }
                        _ => {}
                    }
                }

                // Prompts view navigation
                if state.tab == View::Prompts {
                    match key.code {
                        KeyCode::Up => {
                            if state.prompt_selected > 0 {
                                state.prompt_selected -= 1;
                            }
                        }
                        KeyCode::Down => {
                            if state.prompt_selected < state.prompt_history.len().saturating_sub(1) {
                                state.prompt_selected += 1;
                            }
                        }
                        KeyCode::Enter => {
                            // Copy selected prompt to input and switch to Chat
                            if let Some(prompt) = state.prompt_history.get(state.prompt_selected) {
                                state.input = prompt.clone();
                                state.cursor_pos = state.input.len();
                                state.pop_view();
                            }
                        }
                        _ => {}
                    }
                }

                // Sessions view navigation
                if state.tab == View::Sessions {
                    match key.code {
                        KeyCode::Up => {
                            if state.session_selected > 0 {
                                state.session_selected -= 1;
                            }
                        }
                        KeyCode::Down => {
                            if state.session_selected < state.detected_sessions.len().saturating_sub(1) {
                                state.session_selected += 1;
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    Ok(())
}

fn render_tui(f: &mut Frame, state: &TuiState) {
    let area = f.size();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Header + tabs
            Constraint::Min(5),     // Main content
            Constraint::Length(3),  // Input
            Constraint::Length(1),  // Status
        ])
        .split(area);

    // Exit warning banner
    let exit_warning = matches!(state.exit_state, ExitState::WarnOnce);

    // Header with tabs and navigation hint
    let nav_hint = if state.in_overlay() {
        " [Esc to go back]".to_string()
    } else if !state.view_stack.is_empty() {
        format!(" [{}→]", state.view_stack.len())
    } else {
        String::new()
    };

    // Use current_model from state (can change at runtime via /switch or rate limit)
    let model_display = if !state.rate_limited_models.is_empty() {
        format!("{} ({}✗)", state.current_model, state.rate_limited_models.len())
    } else {
        state.current_model.clone()
    };

    // Context usage indicator
    let context_pct = state.traces.context.usage.last().unwrap_or(0.0);
    let context_indicator = if state.traces.context.is_full() {
        " | CTX:FULL".to_string()
    } else if context_pct > 0.0 {
        format!(" | CTX:{:.0}%", context_pct)
    } else {
        String::new()
    };

    // Agent mode indicator
    let agent_indicator = if state.agent_running {
        " | 🤖⚡"
    } else if state.agent_mode {
        " | 🤖"
    } else {
        ""
    };

    let header_title = if exit_warning {
        format!("hyle | {} | ⚠ Press Ctrl-C again to quit{}", model_display, nav_hint)
    } else if state.rate_limit_pending {
        format!("hyle | {} | ⚠ Rate limited - press ESC{}", model_display, nav_hint)
    } else if state.traces.context.is_full() {
        format!("hyle | {}{}{} | ⚠ CONTEXT FULL{}", model_display, context_indicator, agent_indicator, nav_hint)
    } else if state.traces.context.is_warning() {
        format!("hyle | {}{}{} | ⚠ >80%{}", model_display, context_indicator, agent_indicator, nav_hint)
    } else {
        format!("hyle | {}{}{}{}", model_display, context_indicator, agent_indicator, nav_hint)
    };

    let header_style = if exit_warning {
        Style::default().fg(Color::Yellow)
    } else if state.rate_limit_pending {
        Style::default().fg(Color::Magenta)
    } else if state.traces.context.is_full() {
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
    } else if state.traces.context.is_warning() {
        Style::default().fg(Color::Yellow)
    } else if !state.rate_limited_models.is_empty() {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default()
    };

    let tabs = Tabs::new(View::main_views().iter().map(|t| t.name()))
        .select(View::main_views().iter().position(|v| *v == state.tab).unwrap_or(0))
        .style(Style::default().fg(Color::White))
        .highlight_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .block(Block::default().borders(Borders::ALL).title(header_title).style(header_style));
    f.render_widget(tabs, chunks[0]);

    // Main content based on view
    match state.tab {
        View::Chat => render_chat(f, state, chunks[1]),
        View::Telemetry => render_telemetry(f, state, chunks[1]),
        View::Log => render_log(f, state, chunks[1]),
        View::Sessions => render_sessions(f, state, chunks[1]),
        View::Prompts => render_prompts(f, state, chunks[1]),
        View::Git => render_git(f, state, chunks[1]),
        View::Artifacts => render_artifacts(f, state, chunks[1]),
        View::Plans => render_plans(f, state, chunks[1]),
    }

    // Input
    let input_style = if state.is_generating {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default()
    };
    let input_title = if state.is_generating {
        // Show token count and rate while generating
        let elapsed = state.request_start.elapsed().as_secs_f32();
        let estimated_tokens = (elapsed * state.tokens_per_sec).round() as u32;
        if state.ttft.is_some() {
            format!("Generating... ~{} tokens ({:.1} tok/s)", estimated_tokens, state.tokens_per_sec)
        } else {
            "Waiting for first token...".into()
        }
    } else {
        "Input (Enter to send, Ctrl-A/E/K/U readline)".into()
    };
    let input = Paragraph::new(state.input.as_str())
        .style(input_style)
        .block(Block::default().borders(Borders::ALL).title(input_title));
    f.render_widget(input, chunks[2]);

    // Position cursor in input field (account for border)
    if !state.is_generating && state.tab == Tab::Chat {
        let cursor_x = chunks[2].x + 1 + state.cursor_pos as u16;
        let cursor_y = chunks[2].y + 1;
        f.set_cursor(cursor_x, cursor_y);
    }

    // Status bar
    let pressure = state.telemetry.pressure();
    let sparkline = state.telemetry.cpu_sparkline(16);

    // Build contextual help based on current view
    let help = if state.in_overlay() {
        "Esc:back ↑↓:select Enter:use"
    } else if exit_warning {
        "Ctrl-C:QUIT NOW"
    } else {
        "^C:quit ^P:prompts ^G:git Tab:tabs"
    };

    // Cost indicator (only for paid models)
    let cost_str = if state.session_cost > 0.001 {
        format!(" | ${:.4}", state.session_cost)
    } else if state.session_cost > 0.0 {
        format!(" | ${:.6}", state.session_cost)
    } else {
        String::new()
    };

    let status = format!(
        " {} | {} {} | {}{} | {}",
        if state.is_generating { spinner_char(state.tick) } else { ' ' },
        sparkline,
        pressure.symbol(),
        state.throttle.name(),
        cost_str,
        help,
    );

    let status_style = if exit_warning {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        match pressure {
            PressureLevel::Critical => Style::default().fg(Color::Red),
            PressureLevel::High => Style::default().fg(Color::Yellow),
            _ => Style::default().fg(Color::DarkGray),
        }
    };
    let status = Paragraph::new(status).style(status_style);
    f.render_widget(status, chunks[3]);
}

fn render_chat(f: &mut Frame, state: &TuiState, area: Rect) {
    let visible_height = area.height.saturating_sub(2); // Account for borders

    // Use cached values - no allocation on render
    let line_count = state.output_line_count as u16;

    // Calculate scroll position
    let scroll = if state.auto_scroll {
        line_count.saturating_sub(visible_height)
    } else {
        state.scroll_offset.min(line_count.saturating_sub(visible_height))
    };

    // Build title with scroll indicator
    let scroll_indicator = if line_count > visible_height {
        let pct = if line_count > 0 {
            ((scroll + visible_height) * 100 / line_count).min(100)
        } else {
            100
        };
        format!(" [{}/{}] {}%", scroll + visible_height, line_count, pct)
    } else {
        String::new()
    };

    let history_indicator = if state.history_index.is_some() {
        format!(" [history {}/{}]",
            state.history_index.unwrap() + 1,
            state.prompt_history.len())
    } else {
        String::new()
    };

    let title = format!("Chat{}{}", history_indicator, scroll_indicator);

    // Use cached output - rebuilt only when dirty
    let para = Paragraph::new(state.output_cache.as_str())
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0))
        .block(Block::default().borders(Borders::ALL).title(title));
    f.render_widget(para, area);
}

fn format_bytes(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        format!("{}", n)
    }
}

fn render_telemetry(f: &mut Frame, state: &TuiState, area: Rect) {
    let width = area.width as usize - 4;
    let spark_width = width.min(30);

    // Build system section with all telemetry stats
    let mut lines = vec![
        "── System ──".into(),
        format!(
            "Pressure: {:?}  Throttle: {} (delay: {:.1}x)",
            state.telemetry.pressure(),
            state.throttle.name(),
            state.throttle.delay_multiplier()
        ),
        format!(
            "CPU: {} [{:.1}% avg, last {}ms ago]",
            state.telemetry.cpu_sparkline(spark_width),
            state.telemetry.average_cpu().unwrap_or(0.0),
            state.telemetry.since_last_sample().as_millis()
        ),
    ];

    // Show recent samples summary with all metrics
    let recent = state.telemetry.recent(5);
    if !recent.is_empty() {
        let recent_cpu: Vec<String> = recent.iter()
            .map(|s| format!("{:.0}", s.cpu_percent))
            .collect();
        lines.push(format!("Recent CPU: [{}]", recent_cpu.join(", ")));

        // Show memory and network from most recent sample
        if let Some(latest) = recent.first() {
            lines.push(format!(
                "Memory: {:.1}%  Net: ↓{}B ↑{}B",
                latest.mem_percent,
                format_bytes(latest.net_rx_bytes),
                format_bytes(latest.net_tx_bytes)
            ));
        }
    }

    lines.push(String::new());
    lines.push("── Traces ──".into());

    // Add trace lines with averages and max
    if state.traces.has_data() {
        for line in state.traces.render(width) {
            lines.push(line);
        }
    } else {
        lines.push("(no trace data yet)".into());
    }

    // Add trace statistics
    if let (Some(avg), Some(max)) = (
        state.traces.tokens.tokens_per_sec.average(),
        state.traces.tokens.tokens_per_sec.max()
    ) {
        lines.push(format!("Token rate: avg {:.1}/s, max {:.1}/s", avg, max));
    }

    // Context warnings
    if state.traces.context.is_full() {
        lines.push("⚠ CONTEXT FULL - responses may be truncated".into());
    } else if state.traces.context.is_warning() {
        lines.push("⚠ Context >80% - consider summarizing".into());
    }

    lines.push(String::new());
    lines.push("── Stats ──".into());
    lines.push(format!(
        "Total tokens: {} prompt + {} completion = {}",
        state.traces.tokens.total_prompt,
        state.traces.tokens.total_completion,
        state.traces.tokens.total()
    ));

    if let Some(ttft) = state.ttft {
        lines.push(format!("Last TTFT: {}ms", ttft.as_millis()));
    }

    // Latency stats
    if let (Some(avg), Some(max)) = (
        state.traces.latency.ttft.average(),
        state.traces.latency.ttft.max()
    ) {
        lines.push(format!("TTFT: avg {:.0}ms, max {:.0}ms", avg, max));
    }

    // Spike detection
    if let Some(snapshot) = &state.telemetry.spike_snapshot {
        lines.push(String::new());
        lines.push(format!(
            "⚠ CPU spike detected! Pre-spike snapshot: {} samples (press 'c' to clear)",
            snapshot.len()
        ));
    }

    let para = Paragraph::new(lines.join("\n"))
        .block(Block::default().borders(Borders::ALL).title("Telemetry"));
    f.render_widget(para, area);
}

fn render_log(f: &mut Frame, state: &TuiState, area: Rect) {
    let text: String = state.log.iter().rev().take(50).cloned().collect::<Vec<_>>().join("\n");
    let para = Paragraph::new(text)
        .block(Block::default().borders(Borders::ALL).title("Log"));
    f.render_widget(para, area);
}

fn render_sessions(f: &mut Frame, state: &TuiState, area: Rect) {
    let mut lines = vec![
        "Sessions (↑↓:select Enter:resume/view r:refresh)".into(),
        "".into(),
    ];

    if state.detected_sessions.is_empty() {
        lines.push("No sessions found. Start one with `hyle --new`".into());
        lines.push("".into());
        lines.push("Sessions from other tools will appear here:".into());
        lines.push("  - claude-code, aider, codex, gemini".into());
    } else {
        // Group by status
        let active: Vec<_> = state.detected_sessions.iter()
            .filter(|s| matches!(s.status, SessionStatus::Active | SessionStatus::Backburner))
            .collect();
        let cold: Vec<_> = state.detected_sessions.iter()
            .filter(|s| matches!(s.status, SessionStatus::Cold))
            .collect();
        let foreign: Vec<_> = state.detected_sessions.iter()
            .filter(|s| matches!(s.status, SessionStatus::Foreign))
            .collect();

        if !active.is_empty() {
            lines.push("── Active/Backburner ──".into());
            for (i, s) in active.iter().enumerate() {
                let marker = if i == state.session_selected { ">" } else { " " };
                let status_icon = match s.status {
                    SessionStatus::Active => "●",
                    SessionStatus::Backburner => "◐",
                    _ => "○",
                };
                let int_icon = match s.integration {
                    Integration::Full => "★",
                    Integration::Partial => "☆",
                    Integration::ReadOnly => "○",
                };
                lines.push(format!("{} {} {} {} | {}msg {}tok | {} {}",
                    marker, status_icon, s.tool, s.id,
                    s.messages, s.tokens, s.age, int_icon));
            }
            lines.push("".into());
        }

        if !cold.is_empty() {
            lines.push("── Cold (can revive) ──".into());
            for s in cold.iter().take(5) {
                lines.push(format!("  ○ {} {} | {}msg {}tok | {}",
                    s.tool, s.id, s.messages, s.tokens, s.age));
            }
            lines.push("".into());
        }

        if !foreign.is_empty() {
            lines.push("── Foreign Tools (read-only) ──".into());
            for s in foreign.iter().take(5) {
                lines.push(format!("  ◇ {} {}", s.tool, s.id));
            }
        }
    }

    let para = Paragraph::new(lines.join("\n"))
        .block(Block::default().borders(Borders::ALL).title("Sessions"));
    f.render_widget(para, area);
}

fn render_prompts(f: &mut Frame, state: &TuiState, area: Rect) {
    let mut lines = vec![
        "Prompt History (Up/Down to navigate, Enter to reuse, Esc to close)".into(),
        "".into(),
    ];

    if state.prompt_history.is_empty() {
        lines.push("No prompts yet.".into());
    } else {
        for (i, prompt) in state.prompt_history.iter().enumerate().rev() {
            let marker = if i == state.prompt_selected { ">" } else { " " };
            let truncated = if prompt.len() > 60 {
                format!("{}...", &prompt[..60])
            } else {
                prompt.clone()
            };
            lines.push(format!("{} [{}] {}", marker, i + 1, truncated));
        }
    }

    let para = Paragraph::new(lines.join("\n"))
        .block(Block::default().borders(Borders::ALL).title("Prompt Inventory [Ctrl-P]"));
    f.render_widget(para, area);
}

fn render_git(f: &mut Frame, state: &TuiState, area: Rect) {
    let mut lines = vec![
        "Git Status (Esc to close)".into(),
        "".into(),
    ];

    if state.git_status.is_empty() {
        lines.push("Not a git repository or git not available.".into());
    } else {
        for (i, line) in state.git_status.iter().enumerate() {
            let marker = if i == state.git_selected { ">" } else { " " };
            lines.push(format!("{} {}", marker, line));
        }
    }

    let para = Paragraph::new(lines.join("\n"))
        .block(Block::default().borders(Borders::ALL).title("Git [Ctrl-G]"));
    f.render_widget(para, area);
}

fn render_artifacts(f: &mut Frame, state: &TuiState, area: Rect) {
    let mut lines = vec![
        "Generated Artifacts (Esc to close)".into(),
        "".into(),
    ];

    if state.artifacts.is_empty() {
        lines.push("No artifacts generated yet.".into());
        lines.push("".into());
        lines.push("Artifacts include:".into());
        lines.push("  - Generated files".into());
        lines.push("  - Diffs and patches".into());
        lines.push("  - Code snippets".into());
    } else {
        for (i, artifact) in state.artifacts.iter().enumerate() {
            let marker = if i == state.artifact_selected { ">" } else { " " };
            lines.push(format!("{} [{}] {} - {}", marker, artifact.kind, artifact.name, artifact.preview));
        }
    }

    let para = Paragraph::new(lines.join("\n"))
        .block(Block::default().borders(Borders::ALL).title("Artifacts [Ctrl-A]"));
    f.render_widget(para, area);
}

fn render_plans(f: &mut Frame, state: &TuiState, area: Rect) {
    let mut lines = vec![
        "Task Plans (Esc to close)".into(),
        "".into(),
    ];

    if state.plans.is_empty() {
        lines.push("No plans created yet.".into());
        lines.push("".into());
        lines.push("Ask the model to create a plan for complex tasks.".into());
    } else {
        for (i, plan) in state.plans.iter().enumerate() {
            let marker = if i == state.plan_selected { ">" } else { " " };
            let status_icon = match plan.status.as_str() {
                "done" => "✓",
                "in_progress" => "◐",
                _ => "○",
            };
            lines.push(format!("{} {} {} ({} steps)", marker, status_icon, plan.name, plan.steps.len()));
        }
    }

    let para = Paragraph::new(lines.join("\n"))
        .block(Block::default().borders(Borders::ALL).title("Plans"));
    f.render_widget(para, area);
}

fn spinner_char(tick: usize) -> char {
    const SPINNER: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
    SPINNER[tick % SPINNER.len()]
}

// ═══════════════════════════════════════════════════════════════
// TERMINAL SETUP
// ═══════════════════════════════════════════════════════════════

fn setup_terminal() -> Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    Ok(Terminal::new(backend)?)
}

fn restore_terminal(mut terminal: Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}
