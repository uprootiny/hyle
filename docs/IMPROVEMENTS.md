# hyle Improvement Notes

*Observations from developing, using, and stress-testing hyle*

## What Works Well

### Model Evaluation (`hyle eval`)
- Batch testing without TUI overhead
- Progress indicators `[N/M]` show overall progress
- Health tracking persists across runs, skips broken models
- Retry logic with exponential backoff handles rate limits
- Frame stack shows cascade context (session → task → iteration)

### Core Architecture
- Atomic file writes prevent corruption
- Backup rotation keeps last 3 versions
- Intent stack tracks goal hierarchy
- Salience-aware context management

## What Needs Improvement

### 1. TUI Responsiveness
**Issue**: The TUI can feel sluggish during long operations
**Ideas**:
- Async rendering pipeline
- Progressive disclosure of large outputs
- Skeleton loading states during API calls

### 2. Context Window Management
**Issue**: No clear indication of *how* context is being used
**Ideas**:
- Show token breakdown by category (system, history, tools, current)
- Visual context budget indicator
- Automatic summarization trigger visibility

### 3. Error Recovery
**Issue**: When things go wrong, recovery path isn't clear
**Ideas**:
- Suggested actions after errors (retry, switch model, simplify request)
- Undo capability for file changes
- Checkpoint/restore for long sessions

### 4. Model Selection Intelligence
**Issue**: Model picker doesn't account for task type
**Ideas**:
- Task-aware model suggestion (code vs explanation vs tool use)
- Historical performance per task category
- Cost/quality/speed tradeoff controls

### 5. Session Continuity
**Issue**: Session handoff loses context nuance
**Ideas**:
- Semantic session summary, not just message history
- Key decisions and constraints remembered
- Cross-session learning (what worked before)

### 6. Tool Execution Visibility
**Issue**: Tool calls happen but user doesn't see intermediate state
**Ideas**:
- Live tool output streaming
- Diff preview before file writes
- Execution plan display before multi-step operations

### 7. Backburner Mode
**Issue**: Background maintenance is invisible
**Ideas**:
- Periodic status reports
- Integration with git hooks
- Notification on significant findings

### 8. Documentation Co-pilot
**Issue**: Docs get stale as code evolves
**Ideas**:
- Auto-detect doc/code drift
- Suggest doc updates after code changes
- Keep README.md synced with actual state

## Missing Features (Priority Order)

### P0 - Critical
1. **MCP Server Support** - Industry standard for tool integration
2. **Local Model Support (Ollama)** - Offline capability, privacy

### P1 - Important
3. **Visual Diff Preview** - Show changes before applying
4. **Plugin System** - Extensible tool registry
5. **Multi-Agent Orchestration** - Parallel task execution

### P2 - Nice to Have
6. **Voice Interface** - Accessibility, hands-free coding
7. **Team Collaboration** - Shared sessions, pair programming
8. **IDE Integration** - VS Code extension, JetBrains plugin

## Reliability Observations

### Rate Limit Patterns
| Model | Rate Limit Behavior |
|-------|---------------------|
| qwen3-coder | Aggressive, 2-3 requests then blocked |
| devstral | Generous, rarely rate-limited |
| deepseek-r1 | Moderate, handles backoff well |
| nemotron | Reliable, consistent |

### Error Categories
1. **Transient (retry)**: 429, 503, timeout
2. **Permanent (skip)**: 404, 400, policy errors
3. **Degraded (monitor)**: slow responses, quality drop

## UX Observations

### What Users See
- Breadcrumb bar shows goal hierarchy
- Progress indicators during batch operations
- Health status per model

### What Users Should See
- Token budget visualization
- Predicted cost before execution
- Alternative approaches when stuck
- Learning from past mistakes

## Self-Improvement Ideas

### Autonomous Health Checks
- Periodic model quality sampling
- Auto-update recommended model list
- Detect capability changes in models

### Code Quality Monitoring
- Track test coverage trends
- Detect dead code accumulation
- Flag architectural drift

### Performance Tracking
- First-token latency trends
- Context efficiency over time
- Success rate by task type

## Performance Profile (2025-01-10)

### Binary & Startup
- Binary size: 9.9MB (release, stripped)
- Doctor command: 248ms (includes network check)
- User time: 21ms, System time: 34ms

### Threading Architecture (Verified Good)
| Component | Thread Model | Communication |
|-----------|--------------|---------------|
| TUI Event Loop | Tokio main task | Sync render |
| API Calls | Tokio spawned tasks | mpsc channel |
| Telemetry | Dedicated OS thread | mpsc channel |
| Tool Execution | Tokio spawned tasks | mpsc channel |
| Background I/O | BgWorker (tokio spawn_blocking) | mpsc channel |

### TUI Responsiveness
- Poll timeout: 50ms (~20Hz refresh)
- Output caching: Dirty flag avoids per-frame allocation
- Incremental updates: Tokens appended without full rebuild

### Potential Bottlenecks (Status)
1. **Session detection** - ✓ FIXED: Moved to BgWorker async task
2. **Large output** - Mitigated: dirty flag + incremental append
3. **Search highlighting** - Low priority, fast enough for now

### Recommendations (Status)
1. ✓ Move session detection to background task - DONE (v0.3.1)
2. Use rope/piece table for large outputs - Future (if needed)
3. Lazy search match computation - Future (if needed)

## Additional Observations (2025-01-10)

### Health Tracking Works
The `model_health.json` correctly persists:
- Success/failure counts
- Rolling average latency
- Cooldown timers for rate-limited models
- Status categorization (Healthy/Degraded/RateLimited/Unavailable)

### What's Still Missing

**1. Task-Type Affinity**
Models perform differently on different tasks:
- `devstral` excels at tool_use (86%) but mediocre at rust_fn (69%)
- `deepseek-r1t-chimera` consistent across all (79-86%)

Idea: Track per-task-type scores, recommend models by task.

**2. Latency Prediction**
Know before calling whether a model will be fast or slow.
Currently only visible after the fact in health data.

Idea: Show expected latency in model picker based on historical data.

**3. Cost Tracking for Free Models**
"Free" models still have hidden costs:
- Rate limit interruptions
- Slow responses blocking workflows
- Quality variance requiring retries

Idea: Track "effective cost" including time/retry overhead.

**4. Cascade Visibility in TUI**
The frame stack is built but only visible in eval output.
The TUI breadcrumb shows intents but not the full cascade.

Idea: Add a "Frames" tab showing active cascade with metrics.

**5. Learning from Failures**
When a model fails a task, we record it but don't learn patterns.

Ideas:
- Detect "this model always fails tool_use"
- Auto-exclude models from task types they're bad at
- Suggest alternative approaches when stuck

**6. Session Handoff Quality**
The `--handoff` flag imports Claude Code context, but:
- No indication of what was imported
- No diff showing what's new vs old
- No way to selectively import

Idea: Interactive handoff with preview and selection.

---

## v0.3.3 Release Notes (2026-01-10)

### Tool Call Parsing Fixed
- **OpenAI/OpenRouter format support**: Now parses `<tool_call><function=name>JSON</function></tool_call>` format
- Free models like `xiaomi/mimo` now work properly with tool execution
- Maps common function names: `shell→bash`, `read_file→read`, `write_file→write`, `search→grep`
- Added 7 new tests for OpenAI-style parsing

### Input Field Wrap
- Input field now wraps long text instead of truncating
- Added `.wrap(Wrap { trim: false })` to Paragraph widget

### Context Tracking Fixed
- CTX% now shows total tokens (prompt + completion), not just prompt tokens
- Accurate context window usage display in header

### Installation Path Fixed
- Symlink at `~/.local/bin/hyle` now points to correct build location
- Identified conflict with older `/usr/local/bin/hyle` installation

### Test Count
- **364 tests** (up from 321)
- 36 user story tests covering real pain points

---

## v0.3.2 Release Notes (2026-01-10)

### Non-Blocking UI Complete
- Background worker integration fully wired into TUI loop
- Sessions tab shows loading indicator during refresh
- All file I/O moved to `spawn_blocking` tasks
- No blocking operations in render path

### Test Coverage Expanded
- background.rs: 11 tests (roundtrip, file load, error handling)
- contracts.rs: 18 tests (checkpoints, preconditions, all types)
- Total: **321 tests** (up from 299)

### Architecture Verified
```
TUI Event Loop (50ms, ~20Hz)
├── telemetry_rx.try_recv()  ← OS thread
├── process_bg_responses()   ← BgWorker (async I/O)  [NEW]
├── rx.try_recv()            ← API stream
├── event::poll()            ← Keyboard
└── render_tui()             ← Pure CPU
```

### Code Quality
- 0 explicit panics
- 2 expect() calls (in bootstrap, with context)
- 117 unwrap() calls (being reduced)
- 172 clone() calls (acceptable for correctness)

### Future Improvement Ideas

**1. Module Size**
- `ui.rs` is 3284 lines - consider splitting:
  - `ui/render.rs` - rendering functions
  - `ui/input.rs` - input handling
  - `ui/state.rs` - TuiState and views

**2. Error Context**
- Many `unwrap()` calls could use `.context()` for better errors
- Pattern: `file.read()?.context("reading config")?`

**3. Clone Reduction**
- Some hot paths clone strings unnecessarily
- Consider `Cow<'_, str>` for prompt building
- Arc-wrap shared data like project context

**4. Async Consistency**
- Mix of `tokio::spawn` and `spawn_blocking`
- Standardize on clear patterns for each use case

**5. Observable Contracts**
- contracts.rs has types but no runtime checking
- Wire contract checks into ToolExecutor
- Emit events for contract state changes

**6. Model Intelligence**
- Track per-task-type success rates
- Auto-suggest models based on task
- Show expected latency in picker

**7. Session Archaeology**
- Semantic session summaries (not just message history)
- Cross-session learning (what worked before)
- Diff view for handoff

**8. Telemetry Dashboard**
- Export to Prometheus/Grafana format
- Historical session statistics
- Cost tracking over time

---

*These notes inform the roadmap. Priority is based on frequency of issues and impact on reliability.*
