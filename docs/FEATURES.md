# hyle Feature Ideas

Detailed specifications for planned and experimental features.

---

## Pinned Prompts with Relevance Decay

**Status:** Planned (v0.6)
**Complexity:** Medium-High

### Concept

As conversation scrolls, important prompts disappear. Users should be able to "pin" prompts that remain relevant, keeping them visible at the top of the TUI.

### Interaction

```
┌─────────────────────────────────────────────────────────────┐
│ PINNED ─────────────────────────────────────────────────────│
│ ┌─────────────────────────────────────────────────────────┐ │
│ │ > add authentication to user routes          [↻] [███░] │ │
│ │   pinned 12 turns ago                        retry decay│ │
│ └─────────────────────────────────────────────────────────┘ │
│ ┌─────────────────────────────────────────────────────────┐ │
│ │ > use JWT with 24h expiry                    [↻] [████] │ │
│ │   pinned 3 turns ago                                    │ │
│ └─────────────────────────────────────────────────────────┘ │
├─────────────────────────────────────────────────────────────┤
│ CONVERSATION ───────────────────────────────────────────────│
│                                                             │
│ > add rate limiting to auth endpoints                       │
│ [read] src/routes/auth.rs                                   │
│ [write] src/middleware/rate_limit.rs                        │
│ ...                                                         │
└─────────────────────────────────────────────────────────────┘
```

### Widgets

1. **Pin/Unpin Toggle**
   - Click on any prompt line to pin it
   - Pinned prompts move to floating header area
   - Click again (or X button) to unpin

2. **Retrigger Button [↻]**
   - Re-sends the exact prompt
   - Useful for iterating on the same request
   - Could support "retrigger with edit" mode

3. **Relevance Decay Indicator [████░]**
   - Visual bar showing "freshness" of the prompt
   - Decays based on:
     - Number of turns since pinned
     - Whether files mentioned have changed
     - Semantic distance from current topic
   - Full bar = highly relevant
   - Empty bar = probably stale, suggest unpinning

### Implementation Notes

```rust
struct PinnedPrompt {
    text: String,
    pinned_at_turn: usize,
    mentioned_files: Vec<PathBuf>,
    original_response_summary: String,
}

impl PinnedPrompt {
    fn relevance_score(&self, current_turn: usize, changed_files: &[PathBuf]) -> f32 {
        let age_decay = 1.0 / (1.0 + (current_turn - self.pinned_at_turn) as f32 * 0.1);

        let file_relevance = self.mentioned_files.iter()
            .filter(|f| changed_files.contains(f))
            .count() as f32 / self.mentioned_files.len().max(1) as f32;

        age_decay * 0.7 + file_relevance * 0.3
    }
}
```

### Mouse Events in Terminal

Requires terminal mouse support (most modern terminals):
- ratatui supports `crossterm::event::EnableMouseCapture`
- Track click coordinates
- Map to prompt lines
- Handle pin/unpin/retrigger actions

---

## Auto Pipe Mode

**Status:** Planned (v0.1)
**Complexity:** Low

### Concept

When hyle is invoked in a pipeline, automatically switch to pipe mode with no TUI.

### Detection

```rust
fn should_use_pipe_mode(args: &Args) -> bool {
    // Explicit flag
    if args.pipe { return true; }

    // Not a TTY (piped input or output)
    if !atty::is(atty::Stream::Stdin) { return true; }
    if !atty::is(atty::Stream::Stdout) { return true; }

    // CI environment
    if std::env::var("CI").is_ok() { return true; }

    false
}
```

### Pipe Mode Behavior

- No ANSI escape codes
- No TUI chrome
- Clean text output only
- Read full prompt from stdin
- Write response to stdout
- Errors to stderr

### Examples

```bash
# Read prompt from file
cat prompt.txt | hyle

# Chain with other tools
hyle "explain this code" < src/main.rs | glow

# JSON output for scripting
hyle --json "list the functions" | jq '.functions[]'

# In a script
echo "fix the type errors" | hyle | patch -p1

# Multi-step pipeline
cat error.log | hyle "explain this error" | hyle "suggest a fix" > fix.md

# With heredoc
hyle <<EOF
Review this code for security issues:
$(cat src/auth.rs)
EOF
```

---

## HuggingFace Integration

**Status:** Planned (v0.3)
**Complexity:** Medium

### Concept

Use HuggingFace Inference API for additional free/cheap model options.

### Configuration

```toml
# ~/.config/hyle/config.toml

[providers.huggingface]
api_key = "hf_..."
default_model = "mistralai/Mistral-7B-Instruct-v0.2"

# Free tier models
[[providers.huggingface.models]]
id = "mistralai/Mistral-7B-Instruct-v0.2"
context_window = 8192
free_tier = true

[[providers.huggingface.models]]
id = "google/gemma-7b-it"
context_window = 8192
free_tier = true

[[providers.huggingface.models]]
id = "meta-llama/Llama-2-7b-chat-hf"
context_window = 4096
free_tier = true
```

### Usage

```bash
hyle --provider huggingface --model mistral-7b "explain this"

# Or set as default
export HYLE_PROVIDER=huggingface
export HYLE_MODEL=mistral-7b
hyle "explain this"
```

### Rate Limiting

- HuggingFace free tier has rate limits
- Implement exponential backoff
- Show clear error when rate limited
- Suggest local models as alternative

---

## Guided Walkthrough Sessions

**Status:** Planned (v0.4)
**Complexity:** Medium

### Concept

Interactive learning sessions that guide users through common workflows.

### Walkthrough Format

```yaml
# walkthroughs/rust-api.yaml
name: "Build a REST API"
description: "Create a complete Axum API from scratch"
estimated_turns: 15

steps:
  - id: setup
    prompt_template: "create a new Axum project with {database} support"
    variables:
      database:
        type: choice
        options: [postgres, sqlite, mysql]
        default: postgres
    success_check:
      files_created: [Cargo.toml, src/main.rs]
    next: routes

  - id: routes
    prompt_template: "add CRUD routes for a {resource} resource"
    variables:
      resource:
        type: input
        placeholder: "user"
    success_check:
      files_contain:
        src/routes.rs: ["get_", "create_", "update_", "delete_"]
    next: database

  - id: database
    prompt_template: "add database migrations for the {resource} table"
    success_check:
      files_created: ["migrations/*.sql"]
    next: tests

  - id: tests
    prompt_template: "write integration tests for the API"
    success_check:
      command: "cargo test"
      exit_code: 0
```

### UI

```
┌─────────────────────────────────────────────────────────────┐
│ WALKTHROUGH: Build a REST API                    Step 2/5   │
│ ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━░░░░░░░░░░░░░░░░░░░░  │
├─────────────────────────────────────────────────────────────┤
│ Current: Add CRUD routes                                    │
│                                                             │
│ What resource do you want to create?                        │
│ > [user_____________]                                       │
│                                                             │
│ [Continue]  [Skip]  [Exit Walkthrough]                      │
├─────────────────────────────────────────────────────────────┤
│ > add CRUD routes for a user resource                       │
│                                                             │
│ [write] src/routes/user.rs                                  │
│ pub async fn get_user(...                                   │
└─────────────────────────────────────────────────────────────┘
```

---

## Session Sharing

**Status:** Future
**Complexity:** High

### Concept

Export sessions as shareable artifacts for:
- Bug reports
- Tutorials
- Team knowledge sharing

### Export Formats

```bash
# Markdown (readable)
hyle session export --format markdown > session.md

# Replayable (can be re-run)
hyle session export --format replay > session.hyle

# HTML (interactive viewer)
hyle session export --format html > session.html
```

### Replay Format

```json
{
  "version": "1.0",
  "created": "2025-01-10T...",
  "project": "/path/to/project",
  "model": "claude-3-sonnet",
  "turns": [
    {
      "prompt": "add authentication",
      "response": "...",
      "tools": [
        {"type": "read", "path": "src/main.rs"},
        {"type": "write", "path": "src/auth.rs", "content": "..."}
      ],
      "timestamp": "..."
    }
  ]
}
```

---

## Cost Budget Alerts

**Status:** Planned (v0.3)
**Complexity:** Low

### Concept

Set spending limits per session or per day.

### Configuration

```toml
[budget]
session_limit = 0.50    # USD per session
daily_limit = 5.00      # USD per day
warn_at = 0.80          # Warn at 80% of limit
```

### Behavior

```
┌─────────────────────────────────────────────────────────────┐
│ ⚠ Budget Warning                                            │
│                                                             │
│ This session has used $0.42 of your $0.50 limit.            │
│                                                             │
│ [Continue] [Switch to free model] [End session]             │
└─────────────────────────────────────────────────────────────┘
```

```
┌─────────────────────────────────────────────────────────────┐
│ ✕ Budget Exceeded                                           │
│                                                             │
│ Session limit reached ($0.50).                              │
│                                                             │
│ Options:                                                    │
│ - hyle --budget 1.00  (increase limit)                      │
│ - hyle --model ollama (use free local model)                │
│ - Wait until tomorrow (daily limit resets)                  │
└─────────────────────────────────────────────────────────────┘
```

---

*More features documented as they're designed.*
