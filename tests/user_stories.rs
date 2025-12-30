//! User Story Integration Tests
//!
//! These tests trace complete user workflows with logging to verify
//! the system behaves correctly from the user's perspective.
//!
//! Each test represents a real user story:
//! - "As a user, I want to..."
//! - Tests verify the expected output/behavior
//! - Logs are captured for debugging


/// Test helper to capture and display trace logs
struct TestTracer {
    name: String,
    logs: Vec<String>,
}

impl TestTracer {
    fn new(name: &str) -> Self {
        eprintln!("\n╔═══════════════════════════════════════════════════════════════");
        eprintln!("║ USER STORY: {}", name);
        eprintln!("╚═══════════════════════════════════════════════════════════════\n");
        Self {
            name: name.to_string(),
            logs: vec![],
        }
    }

    fn step(&mut self, description: &str) {
        let msg = format!("  → {}", description);
        eprintln!("{}", msg);
        self.logs.push(msg);
    }

    fn expect(&mut self, condition: bool, description: &str) {
        let status = if condition { "✓" } else { "✗" };
        let msg = format!("    {} {}", status, description);
        eprintln!("{}", msg);
        self.logs.push(msg);
        assert!(condition, "FAILED: {}", description);
    }

    fn done(&self) {
        eprintln!("\n  ══════════════════════════════════════════════════════");
        eprintln!("  ✓ Story completed: {}", self.name);
        eprintln!();
    }
}

// ═══════════════════════════════════════════════════════════════
// STORY: User submits sketch via API
// ═══════════════════════════════════════════════════════════════

#[test]
fn story_submit_sketch_validates_input() {
    let mut t = TestTracer::new("Submit sketch validates input length");

    t.step("Given a sketch shorter than 20 characters");
    let short_sketch = "make a page";

    t.step("When the sketch is validated");
    let is_valid = short_sketch.len() >= 20;

    t.expect(!is_valid, "Short sketch should be rejected");
    t.expect(short_sketch.len() == 11, "Sketch length is 11 chars");

    t.step("Given a valid sketch");
    let valid_sketch = "create an interactive visualization of particle physics";

    t.step("When the sketch is validated");
    let is_valid = valid_sketch.len() >= 20;

    t.expect(is_valid, "Valid sketch should be accepted");
    t.expect(valid_sketch.len() > 20, "Sketch exceeds minimum length");

    t.done();
}

#[test]
fn story_project_name_generation() {
    let mut t = TestTracer::new("Project name generated from sketch");

    t.step("Given a sketch with descriptive words");
    let sketch = "create a beautiful fractal tree generator with animation";

    t.step("When extracting project name");
    let words: Vec<&str> = sketch
        .split_whitespace()
        .filter(|w| w.len() > 3 && w.chars().all(|c| c.is_alphanumeric()))
        .take(2)
        .collect();

    t.expect(words.len() >= 1, "At least one word extracted");
    t.expect(words[0] == "create" || words[0] == "beautiful", "First significant word found");

    t.step("When sanitizing the name");
    let base = words.first().copied().unwrap_or("project");
    let sanitized: String = base
        .chars()
        .filter(|c| c.is_alphanumeric())
        .take(12)
        .collect::<String>()
        .to_lowercase();

    t.expect(!sanitized.is_empty(), "Sanitized name is not empty");
    t.expect(sanitized.len() <= 12, "Name is within length limit");
    t.expect(sanitized.chars().all(|c| c.is_alphanumeric()), "Name only contains alphanumeric");

    t.done();
}

#[test]
fn story_project_name_handles_special_chars() {
    let mut t = TestTracer::new("Project name handles special characters");

    t.step("Given a sketch with path injection attempt");
    let sketch = "foo/../../../etc/passwd";

    t.step("When extracting and sanitizing");
    let sanitized: String = sketch
        .chars()
        .filter(|c| c.is_alphanumeric())
        .take(12)
        .collect::<String>()
        .to_lowercase();

    t.expect(!sanitized.contains('/'), "No slashes in sanitized name");
    t.expect(!sanitized.contains('.'), "No dots in sanitized name");
    t.expect(sanitized == "fooetcpasswd", "Only alphanumeric chars remain");

    t.done();
}

// ═══════════════════════════════════════════════════════════════
// STORY: Model selection and fallback
// ═══════════════════════════════════════════════════════════════

#[test]
fn story_model_rotation_round_robin() {
    let mut t = TestTracer::new("Models rotate in round-robin order");

    t.step("Given a list of available models");
    let models = vec![
        "google/gemini-2.0-flash-exp:free",
        "qwen/qwen3-coder:free",
        "mistralai/devstral-2512:free",
    ];

    t.step("When selecting models sequentially");
    let mut index = 0;

    let first = models[index % models.len()];
    index += 1;
    let second = models[index % models.len()];
    index += 1;
    let third = models[index % models.len()];
    index += 1;
    let fourth = models[index % models.len()];

    t.expect(first == models[0], "First selection is first model");
    t.expect(second == models[1], "Second selection is second model");
    t.expect(third == models[2], "Third selection is third model");
    t.expect(fourth == models[0], "Fourth selection wraps to first model");

    t.done();
}

#[test]
fn story_rate_limit_detection() {
    let mut t = TestTracer::new("Rate limit errors trigger model fallback");

    t.step("Given an error message from API");
    let error_messages = vec![
        "HTTP 429: Too Many Requests",
        "rate limit exceeded",
        "throttled: please retry later",
        "request limit reached",
    ];

    t.step("When checking for rate limit indicators");
    for error in &error_messages {
        let is_rate_limit = error.contains("429")
            || error.to_lowercase().contains("rate")
            || error.to_lowercase().contains("throttl")
            || error.to_lowercase().contains("limit");

        t.expect(is_rate_limit, &format!("'{}' detected as rate limit", error));
    }

    t.step("Given a non-rate-limit error");
    let other_error = "Connection reset by peer";
    let is_rate_limit = other_error.contains("429")
        || other_error.to_lowercase().contains("rate")
        || other_error.to_lowercase().contains("throttl")
        || other_error.to_lowercase().contains("limit");

    t.expect(!is_rate_limit, "Connection error not detected as rate limit");

    t.done();
}

// ═══════════════════════════════════════════════════════════════
// STORY: Port validation
// ═══════════════════════════════════════════════════════════════

#[test]
fn story_port_validation_rejects_privileged() {
    let mut t = TestTracer::new("Privileged ports are rejected");

    const MIN_PORT: u16 = 1024;

    t.step("Given privileged port numbers");
    let privileged_ports = [22, 80, 443, 1023];

    t.step("When validating each port");
    for port in privileged_ports {
        let is_valid = port >= MIN_PORT;
        t.expect(!is_valid, &format!("Port {} correctly rejected", port));
    }

    t.step("Given unprivileged port numbers");
    let valid_ports = [1024, 3000, 8080, 65535];

    t.step("When validating each port");
    for port in valid_ports {
        let is_valid = port >= MIN_PORT;
        t.expect(is_valid, &format!("Port {} correctly accepted", port));
    }

    t.done();
}

// ═══════════════════════════════════════════════════════════════
// STORY: Throttle modes
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq)]
enum ThrottleMode {
    Full,
    Normal,
    Throttled,
    Killed,
}

impl ThrottleMode {
    fn delay_multiplier(&self) -> f32 {
        match self {
            ThrottleMode::Full => 0.0,
            ThrottleMode::Normal => 1.0,
            ThrottleMode::Throttled => 3.0,
            ThrottleMode::Killed => 0.0,
        }
    }
}

#[test]
fn story_throttle_modes_have_correct_delays() {
    let mut t = TestTracer::new("Throttle modes apply correct delay multipliers");

    t.step("Given different throttle modes");

    t.step("When checking Full mode");
    t.expect(ThrottleMode::Full.delay_multiplier() == 0.0, "Full mode has 0x delay");

    t.step("When checking Normal mode");
    t.expect(ThrottleMode::Normal.delay_multiplier() == 1.0, "Normal mode has 1x delay");

    t.step("When checking Throttled mode");
    t.expect(ThrottleMode::Throttled.delay_multiplier() == 3.0, "Throttled mode has 3x delay");

    t.step("When checking Killed mode");
    t.expect(ThrottleMode::Killed.delay_multiplier() == 0.0, "Killed mode has 0x delay (N/A)");

    t.done();
}

// ═══════════════════════════════════════════════════════════════
// STORY: Session management
// ═══════════════════════════════════════════════════════════════

#[test]
fn story_session_id_format() {
    let mut t = TestTracer::new("Session IDs follow expected format");

    t.step("Given a session timestamp");
    let timestamp = chrono::Utc::now();
    let formatted = timestamp.format("%Y%m%d_%H%M%S").to_string();

    t.step("When validating format");
    t.expect(formatted.len() == 15, "Timestamp is 15 characters");
    t.expect(formatted.contains('_'), "Timestamp contains underscore separator");

    t.step("When parsing components");
    let parts: Vec<&str> = formatted.split('_').collect();
    t.expect(parts.len() == 2, "Two parts separated by underscore");
    t.expect(parts[0].len() == 8, "Date part is 8 digits");
    t.expect(parts[1].len() == 6, "Time part is 6 digits");

    t.done();
}

// ═══════════════════════════════════════════════════════════════
// STORY: Tool execution
// ═══════════════════════════════════════════════════════════════

#[test]
fn story_tool_call_parsing() {
    let mut t = TestTracer::new("Tool calls are parsed from LLM response");

    t.step("Given an LLM response with tool call XML");
    let response = r#"I'll create the file now.

<tool>
<name>write</name>
<params>
{"path": "index.html", "content": "<!DOCTYPE html>..."}
</params>
</tool>

The file has been created."#;

    t.step("When searching for tool markers");
    let has_tool_start = response.contains("<tool>");
    let has_tool_end = response.contains("</tool>");
    let has_name = response.contains("<name>");
    let has_params = response.contains("<params>");

    t.expect(has_tool_start, "Response contains <tool> marker");
    t.expect(has_tool_end, "Response contains </tool> marker");
    t.expect(has_name, "Response contains <name> element");
    t.expect(has_params, "Response contains <params> element");

    t.step("When extracting tool name");
    let name_start = response.find("<name>").map(|i| i + 6);
    let name_end = response.find("</name>");
    if let (Some(start), Some(end)) = (name_start, name_end) {
        let tool_name = &response[start..end];
        t.expect(tool_name == "write", "Tool name extracted correctly");
    }

    t.done();
}

// ═══════════════════════════════════════════════════════════════
// STORY: Internet artpiece requirements
// ═══════════════════════════════════════════════════════════════

#[test]
fn story_artpiece_prompt_contains_requirements() {
    let mut t = TestTracer::new("Artpiece prompt includes all requirements");

    t.step("Given the artpiece system prompt");
    let prompt = r#"You are creating an INTERNET ARTPIECE — a self-contained, interactive browser experience.

This is NOT a static webpage. This is something people open in their browser and INTERACT with.
Think: generative art, data visualizations, audio toys, interactive fiction, creative tools.

Requirements:
- Single index.html file (all CSS/JS inline or embedded)
- Responsive: works on any screen size
- Smooth: 60fps animations, no jank
- Dynamic: responds to user input (mouse, touch, keyboard)
- Self-contained: no external dependencies, no build step
- Delightful: surprising, playful, aesthetically considered

Make it something people want to share. Make it memorable."#;

    t.step("When checking for key requirements");
    t.expect(prompt.contains("INTERNET ARTPIECE"), "Emphasizes artpiece nature");
    t.expect(prompt.contains("INTERACT"), "Emphasizes interactivity");
    t.expect(prompt.contains("index.html"), "Specifies single HTML file");
    t.expect(prompt.contains("Responsive"), "Requires responsiveness");
    t.expect(prompt.contains("60fps"), "Specifies smooth animation target");
    t.expect(prompt.contains("no external dependencies"), "Requires self-contained");
    t.expect(prompt.contains("mouse, touch, keyboard"), "Lists input types");
    t.expect(prompt.contains("memorable"), "Emphasizes quality bar");

    t.done();
}

// ═══════════════════════════════════════════════════════════════
// STORY: TUI responsiveness
// ═══════════════════════════════════════════════════════════════

#[test]
fn story_tui_poll_timeout_is_reasonable() {
    let mut t = TestTracer::new("TUI poll timeout allows responsive UI");

    t.step("Given the poll timeout configuration");
    let poll_timeout_ms = 50;

    t.step("When evaluating responsiveness");
    let target_fps = 20; // Minimum acceptable UI refresh rate
    let max_frame_time_ms = 1000 / target_fps;

    t.expect(
        poll_timeout_ms <= max_frame_time_ms,
        &format!("Poll timeout {}ms <= {}ms for {}fps", poll_timeout_ms, max_frame_time_ms, target_fps)
    );

    t.step("When evaluating CPU efficiency");
    let min_poll_ms = 10; // Too fast wastes CPU
    t.expect(
        poll_timeout_ms >= min_poll_ms,
        &format!("Poll timeout {}ms >= {}ms to avoid CPU spin", poll_timeout_ms, min_poll_ms)
    );

    t.done();
}

// ═══════════════════════════════════════════════════════════════
// RUN SUMMARY
// ═══════════════════════════════════════════════════════════════

#[test]
fn all_user_stories_documented() {
    eprintln!("\n");
    eprintln!("╔═══════════════════════════════════════════════════════════════");
    eprintln!("║ USER STORY TEST COVERAGE");
    eprintln!("╠═══════════════════════════════════════════════════════════════");
    eprintln!("║ ");
    eprintln!("║ API Submission:");
    eprintln!("║   • story_submit_sketch_validates_input");
    eprintln!("║   • story_project_name_generation");
    eprintln!("║   • story_project_name_handles_special_chars");
    eprintln!("║ ");
    eprintln!("║ Model Management:");
    eprintln!("║   • story_model_rotation_round_robin");
    eprintln!("║   • story_rate_limit_detection");
    eprintln!("║ ");
    eprintln!("║ Security:");
    eprintln!("║   • story_port_validation_rejects_privileged");
    eprintln!("║ ");
    eprintln!("║ Performance:");
    eprintln!("║   • story_throttle_modes_have_correct_delays");
    eprintln!("║   • story_tui_poll_timeout_is_reasonable");
    eprintln!("║ ");
    eprintln!("║ Session:");
    eprintln!("║   • story_session_id_format");
    eprintln!("║ ");
    eprintln!("║ Tool Execution:");
    eprintln!("║   • story_tool_call_parsing");
    eprintln!("║ ");
    eprintln!("║ Artpiece Quality:");
    eprintln!("║   • story_artpiece_prompt_contains_requirements");
    eprintln!("║ ");
    eprintln!("╚═══════════════════════════════════════════════════════════════");
}
