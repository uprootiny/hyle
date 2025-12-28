# Self-Bootstrapping Roadmap

Goal: hyle develops hyle. No more claude-code dependency.

## Current State (Updated 2025-12-28)

```
93 tests passing
Modules: config, models, client, session, telemetry, traces, skills, tools, ui, backburner, agent, git
TUI: fuzzy picker, tabs, scroll, prompt history, session views, Ctrl-C handling, Esc zoom-out
Agent: tool call parsing (JSON/XML/function), execution, result formatting
Git: status parsing, commit validation, atomic commits, diff generation
Observable: spinner, elapsed time, output buffering, multi-tool tracking
```

## Milestones

### M1: Tool Call Infrastructure [CURRENT]
```
[ ] ToolCall struct with id, name, args, status, output
[ ] ToolCallStatus: Pending -> Running -> Done | Failed | Killed
[ ] Background tool execution with timeout
[ ] Real-time output streaming
[ ] Kill support (Ctrl-C on running tool)
```

**Tests:**
- `test_tool_call_lifecycle` - pending->running->done
- `test_tool_call_timeout` - long-running command times out
- `test_tool_call_kill` - running tool can be killed
- `test_tool_call_output_streaming` - output captured incrementally

### M2: Observable Execution
```
[ ] Spinner while tools run
[ ] "Running..." / "Done" / "Failed" status line
[ ] Tool output buffering with scroll
[ ] Background process panel (like claude-code's ctrl+b)
[ ] Elapsed time display
```

**Tests:**
- `test_tool_status_display` - status string formatting
- `test_output_buffer_limits` - large output truncated sensibly
- `test_background_process_tracking` - multiple concurrent tools

### M3: File Operations
```
[ ] Read file with line numbers (exists in tools.rs)
[ ] Write file with backup
[ ] Edit file with diff preview
[ ] Glob pattern matching
[ ] Grep with context
```

**Tests:**
- `test_file_read_nonexistent` - graceful error
- `test_file_write_backup` - original preserved
- `test_file_edit_preview` - diff shown before apply
- `test_glob_patterns` - **/*.rs works
- `test_grep_context` - surrounding lines included

### M4: Code Generation Loop
```
[ ] System prompt for code assistant
[ ] Parse LLM responses for tool calls
[ ] Execute tool calls sequentially
[ ] Feed results back to LLM
[ ] Detect "task complete" signal
```

**Tests:**
- `test_parse_tool_call_json` - extract tool calls from response
- `test_tool_call_chaining` - read->edit->verify flow
- `test_task_completion_detection` - knows when done

### M5: Git Integration
```
[ ] git status parsing
[ ] git diff generation
[ ] Commit with message
[ ] Branch management
[ ] Push with confirmation
```

**Tests:**
- `test_git_status_parse` - modified/added/deleted
- `test_git_diff_format` - unified diff output
- `test_git_commit_atomic` - single intent per commit

### M6: Self-Development Loop
```
[ ] Load own source as context
[ ] Generate improvements
[ ] Apply changes with review
[ ] Run cargo test
[ ] Commit on success
[ ] Backburner continuous improvement
```

**Tests:**
- `test_self_read_source` - can read src/*.rs
- `test_self_cargo_test` - runs and parses test output
- `test_self_improvement_cycle` - generates valid Rust

---

## Phase 1: Foundation (M1-M2)

### ToolCall Implementation

```rust
// src/tools.rs additions

pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub args: serde_json::Value,
    pub status: ToolCallStatus,
    pub output: Arc<Mutex<String>>,
    pub started_at: Option<Instant>,
    pub finished_at: Option<Instant>,
    pub error: Option<String>,
}

pub enum ToolCallStatus {
    Pending,
    Running,
    Done,
    Failed,
    Killed,
}

impl ToolCall {
    pub fn elapsed(&self) -> Option<Duration> {
        self.started_at.map(|s| {
            self.finished_at.unwrap_or_else(Instant::now) - s
        })
    }

    pub fn status_line(&self) -> String {
        match self.status {
            ToolCallStatus::Pending => "Pending...".into(),
            ToolCallStatus::Running => {
                let elapsed = self.elapsed().map(|d| d.as_secs()).unwrap_or(0);
                format!("Running... {}s", elapsed)
            }
            ToolCallStatus::Done => "Done".into(),
            ToolCallStatus::Failed => format!("Failed: {}", self.error.as_deref().unwrap_or("unknown")),
            ToolCallStatus::Killed => "Killed".into(),
        }
    }
}
```

### Background Execution

```rust
// src/executor.rs (new)

pub struct Executor {
    running: HashMap<String, JoinHandle<()>>,
    kill_signals: HashMap<String, Arc<AtomicBool>>,
}

impl Executor {
    pub async fn run_tool(&mut self, call: &mut ToolCall) -> Result<()> {
        let kill = Arc::new(AtomicBool::new(false));
        self.kill_signals.insert(call.id.clone(), kill.clone());

        call.status = ToolCallStatus::Running;
        call.started_at = Some(Instant::now());

        match call.name.as_str() {
            "read" => self.run_read(call).await,
            "write" => self.run_write(call).await,
            "bash" => self.run_bash(call, kill).await,
            "glob" => self.run_glob(call).await,
            "grep" => self.run_grep(call).await,
            _ => Err(anyhow!("Unknown tool: {}", call.name)),
        }
    }

    pub fn kill(&mut self, id: &str) {
        if let Some(signal) = self.kill_signals.get(id) {
            signal.store(true, Ordering::SeqCst);
        }
    }
}
```

---

## Verification Checklist

Before declaring self-bootstrapping:

```
[ ] Can read any project file
[ ] Can edit files with diff preview
[ ] Can run cargo test and parse output
[ ] Can commit changes with proper messages
[ ] Can resume session after restart
[ ] Can run backburner for continuous improvement
[ ] All tests pass (target: 50+)
[ ] Successfully made 10+ commits using hyle itself
```

---

## Test Suite Structure

```
tests/
├── tool_calls.rs      # M1: Tool call infrastructure
├── observability.rs   # M2: Status, output, timing
├── file_ops.rs        # M3: Read/write/edit/glob/grep
├── code_gen.rs        # M4: LLM response parsing
├── git_ops.rs         # M5: Git integration
└── self_dev.rs        # M6: Self-development loop
```

Run focused tests:
```bash
cargo test tool_calls
cargo test observability
cargo test file_ops
```

---

## Success Criteria

**Minimum Viable Self-Bootstrap:**
1. `hyle` can read its own source files
2. `hyle` can edit files and show diffs
3. `hyle` can run `cargo test` and report results
4. `hyle` can commit changes
5. User can have multi-turn conversation about code changes

**Full Self-Bootstrap:**
1. All above, plus:
2. Backburner runs autonomously with LLM guidance
3. Session persistence across restarts
4. Multiple concurrent tool calls with observability
5. hyle has made 100+ commits to itself
