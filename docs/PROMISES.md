# hyle: Promises vs Reality

An honest accounting of what we claim, what exists, and what's needed.

**Last updated:** 2025-01-10
**Status:** Pre-release / Vaporware with good intentions

---

## Current State

hyle is currently a **landing page with aspirations**. The core CLI exists in early form but most features shown on the marketing pages are unimplemented.

### What Actually Exists

| Component | Status | Notes |
|-----------|--------|-------|
| Landing pages | Done | 13 variants at hyle.lol |
| Core CLI binary | Partial | Basic REPL, OpenRouter integration |
| TUI | Partial | ratatui-based, needs polish |
| Session persistence | Not started | - |
| Backup system | Not started | - |
| Audit logging | Not started | - |
| Safety filters | Not started | - |
| Test suite | Minimal | Nowhere near "364 tests" |
| Ollama integration | Not started | - |

---

## Promise Audit by Persona

### Unix/Composable (`unix.html`)

**What we promise:**
- Pipes and composition with unix tools
- stdin/stdout as first-class interface
- "hyle | jq", "cat file | hyle" workflows

**Reality:**
- TUI mode breaks pipes
- No `--pipe` or `--no-tui` flag
- Output includes ANSI codes and chrome

**To deliver:**
```rust
// Needed in src/main.rs
if !atty::is(atty::Stream::Stdout) || args.pipe {
    run_pipe_mode()?;  // Clean text only, no TUI
}
```

**Priority:** HIGH - This is table stakes for a CLI tool

---

### Velocity (`velocity.html`)

**What we promise:**
- 20Hz TUI refresh
- Instant slash commands
- Fast iteration cycles

**Reality:**
- 20Hz refresh is trivial (ratatui default)
- Slash commands not implemented
- LLM latency dominates; we can't fix that

**To deliver:**
```rust
// src/commands.rs - bypass LLM entirely
match input.trim() {
    "/build" => run_shell("cargo build"),
    "/test" => run_shell("cargo test"),
    "/diff" => run_shell("git diff"),
    "/status" => show_project_status(),
    _ => send_to_llm(input),
}
```

**Priority:** HIGH - Slash commands are a real differentiator

---

### Reliable (`reliable.html`)

**What we promise:**
- Automatic backups before writes
- Session recovery
- "364 tests"

**Reality:**
- No backup system exists
- Sessions don't persist
- Test count is fabricated

**To deliver:**
```rust
// src/tools/write.rs
fn write_file(path: &Path, content: &str) -> Result<()> {
    // Create backup first
    let backup = path.with_extension(format!(
        "{}.{}.bak",
        path.extension().unwrap_or_default().to_str().unwrap(),
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs()
    ));
    fs::copy(path, &backup)?;

    // Then write
    fs::write(path, content)?;
    Ok(())
}
```

**Honest test count:** Currently ~12 tests. Target 100+ for v1.0.

**Priority:** MEDIUM - Backups are nice-to-have, session recovery is important

---

### Secure (`secure.html`)

**What we promise:**
- Command safety filters
- Zero telemetry
- Audit logging
- "Verify everything yourself"

**Reality:**
- No safety filters implemented
- Zero telemetry is true (nothing to phone home)
- No audit logging
- Source is available but sparse

**To deliver:**
```rust
// src/safety.rs
const BLOCKED_PATTERNS: &[&str] = &[
    r"rm\s+-rf\s+/",
    r"rm\s+-rf\s+~",
    r"mkfs\.",
    r"dd\s+if=.*of=/dev/",
    r">\s*/dev/sd",
    r"curl.*\|\s*bash",
    r"wget.*\|\s*bash",
];

pub fn check_command(cmd: &str) -> Result<(), BlockedCommand> {
    for pattern in BLOCKED_PATTERNS {
        if Regex::new(pattern)?.is_match(cmd) {
            return Err(BlockedCommand { pattern, cmd });
        }
    }
    Ok(())
}
```

**For real security:**
- Sandbox via bubblewrap/firejail
- Capability-based permissions (allow_network, allow_write, etc.)
- Signed releases with SBOM

**Priority:** HIGH - Safety filters are easy wins. Sandboxing is v2.

---

### Depth (`depth.html`)

**What we promise:**
- "Formally verified"
- Proofs and theorems
- Deep architectural thinking

**Reality:**
- No formal verification
- No proofs
- Architecture exists only in landing page diagrams

**To deliver:**

Remove "formally verified" claims. Replace with honest statements:
- "Property tested with proptest"
- "Fuzzed with cargo-fuzz"
- "Typed with strict clippy lints"

```rust
// Actually achievable rigor:
#[cfg(test)]
mod proptests {
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn backup_never_loses_data(content: String) {
            // Property: original file always recoverable after failed write
        }
    }
}
```

**Priority:** LOW - Academic posturing helps no one. Be honest.

---

### Playful (`playful.html`)

**What we promise:**
- Creative coding workflows
- Generative art projects
- Interactive demos

**Reality:**
- Prompts shown would actually work
- No special creative features
- The particle canvas is just marketing

**To deliver:**

Nothing special needed. The value prop is real: hyle + good prompts = creative output.

Consider: Link to a gallery of actual generated projects?

**Priority:** LOW - Already honest enough

---

### Observable (`observable.html`)

**What we promise:**
- Live metrics dashboard
- Session tracking
- Token usage monitoring

**Reality:**
- Dashboard is pure mockup
- No metrics exported
- Token counting exists (OpenRouter returns it)

**To deliver:**
```rust
// src/metrics.rs
use prometheus::{Counter, Histogram, Registry};

lazy_static! {
    static ref TOKENS_IN: Counter = Counter::new("hyle_tokens_input", "Input tokens")?;
    static ref TOKENS_OUT: Counter = Counter::new("hyle_tokens_output", "Output tokens")?;
    static ref LATENCY: Histogram = Histogram::new("hyle_request_seconds", "Request latency")?;
}

// src/main.rs
async fn metrics_endpoint() -> impl Responder {
    let encoder = TextEncoder::new();
    let metrics = prometheus::gather();
    encoder.encode_to_string(&metrics)
}
```

**Priority:** MEDIUM - Nice for power users, not essential for v1

---

### Community (`community.html`)

**What we promise:**
- Active community
- Good first issues
- Contribution workflow

**Reality:**
- No community exists
- Issues shown are fictional
- CONTRIBUTING.md doesn't exist

**To deliver:**

1. Create real issues:
   - "Add --version flag"
   - "Implement /help command"
   - "Add config file support"
   - "Write README.md"

2. Create CONTRIBUTING.md with actual guidelines

3. Set up CI that runs on PRs

4. Be responsive to actual contributors

**Priority:** MEDIUM - Can't force community, but can prepare for it

---

### Indie (`indie.html`)

**What we promise:**
- Cost calculator
- Free model support
- Budget-conscious design

**Reality:**
- Calculator math is accurate
- Ollama integration doesn't exist
- No cost tracking per session

**To deliver:**
```rust
// src/providers/ollama.rs
pub struct OllamaProvider {
    base_url: String,  // default: http://localhost:11434
}

impl Provider for OllamaProvider {
    async fn complete(&self, messages: &[Message]) -> Result<Response> {
        // POST /api/chat
        // Cost: $0.00
    }
}

// src/session.rs
pub struct SessionCost {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub model: String,
    pub estimated_cost_usd: f64,
}
```

**Priority:** HIGH - Free local models are a real differentiator

---

### Learn (`learn.html`)

**What we promise:**
- Learning paths
- Progress tracking
- Step-by-step tutorials

**Reality:**
- No curriculum exists
- Progress tracker is UI theater
- Tutorials are fictional

**To deliver:**

**Option A:** Build actual tutorials (high effort, low value)

**Option B:** Be honest and link to real resources:
- "Learn Rust" -> The Rust Book
- "Learn Web Dev" -> MDN
- "Learn CLI" -> Command Line Rust book

Recommend Option B. We're a tool, not a learning platform.

**Priority:** LOW - Remove fake progress tracker, add real resource links

---

### Control (`control.html`)

**What we promise:**
- Full transparency
- Audit everything
- Verify all claims

**Reality:**
- Source is available
- No audit logging
- Safety filters unimplemented

**To deliver:**

The page is mostly honest about what *could* exist. Need to:

1. Actually implement audit.jsonl
2. Make source code match what we show in "Source Explorer"
3. Add reproducible build instructions
4. Publish SBOM with releases

```rust
// src/audit.rs
pub fn log_operation(op: &Operation) -> Result<()> {
    let entry = AuditEntry {
        timestamp: Utc::now(),
        operation: op.clone(),
        result: op.result.clone(),
    };

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(audit_path())?;

    writeln!(file, "{}", serde_json::to_string(&entry)?)?;
    Ok(())
}
```

**Priority:** HIGH - Audit logging is easy and valuable

---

### Flow (`flow.html`)

**What we promise:**
- Keyboard-driven workflow
- Prompt queuing
- Session resume
- 20Hz refresh

**Reality:**
- Keyboard shortcuts need TUI
- No prompt queue
- No session resume
- Refresh rate is easy

**To deliver:**

```rust
// src/queue.rs
pub struct PromptQueue {
    items: VecDeque<String>,
    current: Option<InFlightRequest>,
}

impl PromptQueue {
    pub fn enqueue(&mut self, prompt: String) {
        self.items.push_back(prompt);
    }

    pub async fn process_next(&mut self) -> Option<Response> {
        if self.current.is_some() {
            return None;  // Still processing
        }

        let prompt = self.items.pop_front()?;
        self.current = Some(send_to_llm(prompt));
        // ...
    }
}

// src/session.rs
pub fn save_session(session: &Session) -> Result<()> {
    let path = state_dir().join("sessions").join(&session.id);
    fs::create_dir_all(&path)?;
    fs::write(path.join("session.json"), serde_json::to_string_pretty(session)?)?;
    Ok(())
}

pub fn load_latest_session() -> Result<Option<Session>> {
    // Find most recent session for current directory
}
```

**Priority:** HIGH - Session resume and queuing are core UX features

---

## Roadmap

### v0.1.0 - Honest Foundation
- [ ] Basic CLI with OpenRouter
- [ ] Slash commands (/build, /test, /diff, /status)
- [ ] --pipe mode for unix composition
- [ ] Actual README.md
- [ ] 20+ real tests

### v0.2.0 - Core Features
- [ ] Session persistence
- [ ] Session resume on startup
- [ ] Backup before write
- [ ] Basic safety filters
- [ ] Ollama/local model support

### v0.3.0 - Power User
- [ ] Prompt queuing
- [ ] Audit logging
- [ ] Cost tracking per session
- [ ] Config file support
- [ ] 50+ tests

### v0.4.0 - Polish
- [ ] Keyboard shortcuts
- [ ] TUI polish
- [ ] /help with all commands
- [ ] Man page
- [ ] 100+ tests

### v1.0.0 - Release
- [ ] All promises from landing pages delivered
- [ ] Reproducible builds
- [ ] Signed releases
- [ ] SBOM
- [ ] Real documentation

---

## Landing Page Fixes Needed

| Page | Issue | Fix |
|------|-------|-----|
| reliable.html | "364 tests" | Change to "Comprehensive test suite" or actual count |
| depth.html | "Formally verified" | Remove or change to "Property tested" |
| community.html | Fake issues | Create real issues or add "Coming soon" |
| learn.html | Fake progress | Link to real resources instead |
| observable.html | Mock dashboard | Add "Planned feature" label |

---

## Philosophy

We should market what we're building, not what we wish existed.

The landing pages can show the vision, but should be honest about current state. A small "Status: Planned" badge on unimplemented features would maintain trust while still selling the vision.

**Good:** "hyle will support prompt queuing" (future tense, honest)
**Bad:** "hyle supports prompt queuing" (present tense, false)
**Current:** Showing queuing UI as if it exists (misleading)

---

## How to Contribute

See [CONTRIBUTING.md](./CONTRIBUTING.md) (TODO: create this)

Priority contributions:
1. Implement slash commands
2. Add --pipe mode
3. Write tests for existing code
4. Implement session save/load
5. Add Ollama provider

---

*This document will be updated as features are implemented.*
