//! TUI and interactive components
//!
//! Features:
//! - API key prompting
//! - Fuzzy model picker
//! - Interactive chat loop
//! - Telemetry display
//! - Kill/throttle/fullspeed controls

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
use crate::session::Session;
use crate::telemetry::{Telemetry, ThrottleMode, PressureLevel};
use crate::traces::Traces;
use crate::tools::{ToolCallTracker, ToolExecutor, ToolCallDisplay};
use crate::agent::{parse_tool_calls, execute_tool_calls, format_tool_results, is_task_complete};

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
}

/// Main TUI state
struct TuiState {
    tab: Tab,
    input: String,
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

impl TuiState {
    fn new(context_window: u32) -> Self {
        Self {
            tab: Tab::Chat,
            input: String::new(),
            output: vec!["Welcome to hyle. Press Ctrl-C twice to quit, Esc to zoom out.".into()],
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
        }
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
        self.log(&format!("Executing {} tool call(s)...", calls.len()));

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
                    self.log(&format!("Tool {} failed", call.name));
                }
            }
        }

        self.executing_tools = false;

        // Format results for LLM feedback
        Some(format_tool_results(&self.tool_tracker, &indices))
    }

    /// Get tool status for status bar
    fn tool_status(&self) -> String {
        self.tool_tracker.status_summary(self.tick)
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
            }
            Some(0) => {
                // Already at oldest
            }
            Some(i) => {
                self.history_index = Some(i - 1);
                self.input = self.prompt_history[i - 1].clone();
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
            }
            Some(i) => {
                self.history_index = Some(i + 1);
                self.input = self.prompt_history[i + 1].clone();
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
}

/// Run the main TUI
pub async fn run_tui(api_key: &str, model: &str, paths: Vec<PathBuf>, resume: bool) -> Result<()> {
    let mut terminal = setup_terminal()?;
    let result = run_tui_loop(&mut terminal, api_key, model, paths, resume).await;
    restore_terminal(terminal)?;
    result
}

async fn run_tui_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    api_key: &str,
    model: &str,
    _paths: Vec<PathBuf>,
    resume: bool,
) -> Result<()> {
    // Get context window for this model
    let context_window = crate::models::get_context_window(model);
    let mut state = TuiState::new(context_window);

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

    state.log(format!("Model: {} ({}k ctx)", model, context_window / 1000));

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
                    if let Some(last) = state.output.last_mut() {
                        last.push_str(&t);
                    }
                }
                TuiMsg::Done(usage) => {
                    state.is_generating = false;
                    state.prompt_tokens = usage.prompt_tokens;
                    state.completion_tokens = usage.completion_tokens;

                    // Record traces
                    let duration = state.request_start.elapsed();
                    state.traces.latency.record_total(duration);
                    state.traces.tokens.record(
                        usage.prompt_tokens,
                        usage.completion_tokens,
                        duration.as_secs_f64()
                    );
                    state.traces.context.record(usage.prompt_tokens);

                    // Save assistant message to session
                    if !state.current_response.is_empty() {
                        if let Err(e) = session.add_assistant_message(
                            &state.current_response,
                            Some(usage.completion_tokens)
                        ) {
                            state.log(format!("Session save error: {}", e));
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
                    state.log(format!("Completed: {} tokens in {:.1}s", usage.total_tokens, duration.as_secs_f64()));
                }
                TuiMsg::Error(e) => {
                    state.is_generating = false;
                    state.output.push(format!("\n[Error: {}]", e));
                    state.log(format!("Error: {}", e));
                }
            }
        }

        // Render
        terminal.draw(|f| render_tui(f, &state, model))?;

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
                        } else if state.input.is_empty() {
                            // Clear any selection state, but don't exit
                            state.history_index = None;
                            state.auto_scroll = true;
                        } else {
                            // Clear input
                            state.input.clear();
                            state.history_index = None;
                        }
                    }
                    // Tab: cycle through main views
                    KeyCode::Tab => {
                        let views = View::main_views();
                        let idx = views.iter().position(|v| *v == state.tab).unwrap_or(0);
                        state.tab = views[(idx + 1) % views.len()];
                        state.view_stack.clear(); // Clear stack when switching tabs
                    }
                    // Shift+Tab: cycle backwards
                    KeyCode::BackTab => {
                        let views = View::main_views();
                        let idx = views.iter().position(|v| *v == state.tab).unwrap_or(0);
                        state.tab = views[(idx + views.len() - 1) % views.len()];
                        state.view_stack.clear();
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
                                state.history_index = None;
                                state.output.push(format!("> {}", prompt));
                                state.output.push(String::new()); // For response
                                state.is_generating = true;
                                state.ttft = None;
                                state.auto_scroll = true; // Auto-scroll on new message
                                state.request_start = std::time::Instant::now();
                                state.last_token_time = std::time::Instant::now();
                                state.log(format!("Sending: {}", &prompt[..prompt.len().min(50)]));

                                // Save user message to session
                                if let Err(e) = session.add_user_message(&prompt) {
                                    state.log(format!("Session save error: {}", e));
                                }

                                // Spawn API call
                                let tx = tx.clone();
                                let api_key = api_key.to_string();
                                let model = model.to_string();

                                tokio::spawn(async move {
                                    match client::stream_completion(&api_key, &model, &prompt).await {
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
                        KeyCode::Char(c) => {
                            state.input.push(c);
                        }
                        KeyCode::Backspace => {
                            state.input.pop();
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    Ok(())
}

fn render_tui(f: &mut Frame, state: &TuiState, model: &str) {
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
        format!(" [Esc to go back]")
    } else if !state.view_stack.is_empty() {
        format!(" [{}→]", state.view_stack.len())
    } else {
        String::new()
    };

    let header_title = if exit_warning {
        format!("hyle | {} | ⚠ Press Ctrl-C again to quit{}", model, nav_hint)
    } else {
        format!("hyle | {}{}", model, nav_hint)
    };

    let header_style = if exit_warning {
        Style::default().fg(Color::Yellow)
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
        format!("Generating... ({:.1} tok/s)", state.tokens_per_sec)
    } else {
        "Input (Enter to send)".into()
    };
    let input = Paragraph::new(state.input.as_str())
        .style(input_style)
        .block(Block::default().borders(Borders::ALL).title(input_title));
    f.render_widget(input, chunks[2]);

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

    let status = format!(
        " {} | {} {} | {} | {}",
        if state.is_generating { spinner_char(state.tick) } else { ' ' },
        sparkline,
        pressure.symbol(),
        state.throttle.name(),
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
    let text: String = state.output.join("\n");
    let line_count = text.lines().count() as u16;

    // Calculate scroll position
    let scroll = if state.auto_scroll {
        // Auto-scroll to bottom
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

    let para = Paragraph::new(text)
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0))
        .block(Block::default().borders(Borders::ALL).title(title));
    f.render_widget(para, area);
}

fn render_telemetry(f: &mut Frame, state: &TuiState, area: Rect) {
    let width = area.width as usize - 4;

    let mut lines = vec![
        "── System ──".into(),
        format!("Pressure: {:?}  Throttle: {}", state.telemetry.pressure(), state.throttle.name()),
        format!("CPU: {} [{:.1}%]", state.telemetry.cpu_sparkline(width.min(30)), state.telemetry.average_cpu().unwrap_or(0.0)),
        String::new(),
        "── Traces ──".into(),
    ];

    // Add trace lines
    for line in state.traces.render(width) {
        lines.push(line);
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

    if let Some(snapshot) = &state.telemetry.spike_snapshot {
        lines.push(String::new());
        lines.push(format!("Pre-spike snapshot ({} samples)", snapshot.len()));
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
