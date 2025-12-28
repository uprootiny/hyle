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

/// Tab selection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Chat,
    Telemetry,
    Log,
}

impl Tab {
    fn all() -> &'static [Tab] {
        &[Tab::Chat, Tab::Telemetry, Tab::Log]
    }

    fn name(&self) -> &'static str {
        match self {
            Tab::Chat => "Chat",
            Tab::Telemetry => "Telemetry",
            Tab::Log => "Log",
        }
    }
}

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
}

impl TuiState {
    fn new(context_window: u32) -> Self {
        Self {
            tab: Tab::Chat,
            input: String::new(),
            output: vec!["Welcome to hyle. Type your request and press Enter.".into()],
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

                // Global controls
                match key.code {
                    KeyCode::Esc => break,
                    KeyCode::Tab => {
                        let tabs = Tab::all();
                        let idx = tabs.iter().position(|t| *t == state.tab).unwrap_or(0);
                        state.tab = tabs[(idx + 1) % tabs.len()];
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

    // Header with tabs
    let tabs = Tabs::new(Tab::all().iter().map(|t| t.name()))
        .select(Tab::all().iter().position(|t| *t == state.tab).unwrap_or(0))
        .style(Style::default().fg(Color::White))
        .highlight_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .block(Block::default().borders(Borders::ALL).title(format!("hyle | {}", model)));
    f.render_widget(tabs, chunks[0]);

    // Main content based on tab
    match state.tab {
        Tab::Chat => render_chat(f, state, chunks[1]),
        Tab::Telemetry => render_telemetry(f, state, chunks[1]),
        Tab::Log => render_log(f, state, chunks[1]),
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
    let sparkline = state.telemetry.cpu_sparkline(20);
    let status = format!(
        " {} | CPU: {} {} | Throttle: {} | k:kill t:throttle f:full n:normal Tab:switch Esc:quit",
        if state.is_generating { spinner_char(state.tick) } else { ' ' },
        sparkline,
        pressure.symbol(),
        state.throttle.name(),
    );
    let status_style = match pressure {
        PressureLevel::Critical => Style::default().fg(Color::Red),
        PressureLevel::High => Style::default().fg(Color::Yellow),
        _ => Style::default().fg(Color::DarkGray),
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
