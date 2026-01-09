# hyle Development Journal

Technical report tracing the development of hyle, including blind alleys and course corrections.

## Session 1: Initial Scaffolding (2024-12-27)

### Starting Point
User provided a Rust POC file `codex_poc_agent.rs` - a basic code transformation tool with:
- Policy rules for code changes
- Diff generation
- JSONL logging

### Blind Alley #1: Wrong Provider Focus

**What happened:**
Built a comprehensive multi-provider system supporting:
- Anthropic Claude API
- Ollama local models
- Complex provider abstraction layer

**Why it was wrong:**
User wanted **OpenRouter with FREE models only**. The CLAUDE.md file mentioned Anthropic/Ollama but that wasn't the target. We wasted cycles on:
- Anthropic API integration
- Ollama local model support
- Provider trait abstractions for multiple backends

**Correction:**
Stripped down to OpenRouter-only. Single provider, simpler code.

### Blind Alley #2: Wrong Naming

**What happened:**
Initially called the project:
1. `claude-replacement` (too literal)
2. `codex` (generic)
3. `codish` (user's suggestion, then changed)

**Why it was wrong:**
User wanted a distinct identity: `hyle`

**Correction:**
Renamed everything to `hyle`. Updated:
- Cargo.toml
- All help text
- Config paths (~/.config/hyle/)
- HTTP headers (X-Title, Referer)

### Blind Alley #3: Over-Engineering Tools

**What happened:**
Built a full tool registry with:
```rust
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn spec(&self) -> ToolSpec;
    fn execute(&self, params: Value) -> Result<ToolResult>;
}

pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}
```

Plus individual tool implementations:
- GlobTool
- GrepTool
- ReadTool
- EditTool
- BashTool

**Why it was wrong:**
Premature abstraction. None of this was actually used in the main flow. The tools sat there as dead code generating warnings.

**Correction:**
Simplified to direct functions in `tools.rs`:
- `read_file()` - read with line numbers
- `generate_diff()` - unified diff between strings
- `preview_changes()` - show diff summary

These are called directly, no registry overhead.

### Blind Alley #4: Web UI Distraction

**What happened:**
Built a full Axum web server with:
- HTML UI serving
- Status API endpoints
- CORS handling

**Why it was wrong:**
The core use case is `hyle --free` from terminal. Web UI is a nice-to-have, not essential. Spent time on:
- HTML template
- Router setup
- State sharing

**Correction:**
Removed web server entirely for now. Focus on TUI. Web can come later.

### Blind Alley #5: API Compatibility Issues

**What happened:**
Multiple compile errors from mismatched crate versions:

1. **sysinfo 0.30 API changes:**
   - `global_cpu_usage()` → doesn't exist
   - `refresh_cpu_all()` → `refresh_all()`

2. **ratatui 0.26 API changes:**
   - `Frame::area()` → `Frame::size()`

3. **similar crate:**
   - `iter_inline_changes()` → `unified_diff()` builder

**Why it happened:**
Assumed API from docs/memory without checking actual crate versions.

**Correction:**
Read compiler errors carefully, used suggested alternatives:
```rust
// Before (wrong)
self.system.global_cpu_usage()
// After (correct)
self.system.cpus().iter().map(|c| c.cpu_usage()).sum::<f32>() / cpus.len()
```

### What Actually Works

After corrections, we have a working `hyle` that:

1. **Config Management** (~/.config/hyle/config.json)
   - Secure storage (0600 perms)
   - API key persistence
   - XDG compliance

2. **Models Caching** (~/.cache/hyle/models.json)
   - 24-hour TTL
   - 353 models fetched
   - 35 free models filtered

3. **SSE Streaming**
   - Real-time token display
   - Usage tracking
   - Error handling

4. **Fuzzy Model Picker**
   - Incremental search
   - Free model filtering
   - Context length display

5. **Telemetry**
   - CPU sampling at 4Hz
   - Pressure detection (Low/Medium/High/Critical)
   - Sparkline visualization

6. **Throttle Controls**
   - k = kill
   - t = throttle
   - f = full speed
   - n = normal

### Lessons Learned

1. **Read the actual requirements** - CLAUDE.md was a distraction, not the spec
2. **Start with the CLI contract** - `hyle --free` is the north star
3. **Check crate versions** - don't assume API stability
4. **Avoid premature abstraction** - direct functions beat registries for small tools
5. **Name things early** - identity matters, don't keep "replacement" in names

## Architecture That Emerged

```
src/
├── main.rs       # CLI parsing, command dispatch
├── config.rs     # XDG paths, secure key storage
├── models.rs     # Model list caching, free filter
├── client.rs     # OpenRouter SSE streaming
├── telemetry.rs  # CPU/mem sampling, pressure detection
├── ui.rs         # Fuzzy picker, TUI, controls
└── tools.rs      # File operations, diff generation
```

Each module is focused:
- No cross-dependencies between config/models/client
- UI depends on telemetry for display
- Tools are pure functions, no state

## Test Results

```
running 11 tests
test config::tests::test_config_default ... ok
test config::tests::test_config_serialize ... ok
test client::tests::test_parse_stream_chunk ... ok
test client::tests::test_parse_models_response ... ok
test models::tests::test_model_is_free ... ok
test models::tests::test_display_name ... ok
test telemetry::tests::test_pressure_level ... ok
test telemetry::tests::test_throttle_mode ... ok
test telemetry::tests::test_telemetry_sample ... ok
test tools::tests::test_generate_diff ... ok
test tools::tests::test_preview_changes ... ok

test result: ok. 11 passed; 0 failed
```

## Session 2: Self-Bootstrapping Push (2025-01-04 to 2025-01-08)

### The rm -rf Disaster

**What happened:**
A free model (gemma-2-9b-it) with tool access interpreted "take over development"
as "delete everything and start fresh", executing `rm -rf /home/user/project`.

**Lesson learned:**
Free models with tool access need guardrails.

**Fix applied:**
`BLOCKED_PATTERNS` in tools.rs now blocks:
- `rm -rf`, `rm -r`, `rm --recursive`
- Fork bombs, disk overwrites
- Chained destructive commands
- Remote code execution patterns

### Protocol-Driven Development

Adopted a structured commit protocol with 4 sets:
1. **Foundation**: feat, refactor, feat, test
2. **Refinement**: perf, refactor, style, test
3. **Enhancement**: feat, refactor, chore, test
4. **Polish**: style, docs, chore, review

### Key Additions

1. **Prompt Queue** - Users can type during generation, prompts queue up
2. **Type-Safe Enums** - `Role` and `LogKind` replace stringly-typed fields
3. **Async TUI** - Scrolling works during generation
4. **Library Crate** - `src/lib.rs` exposes modules for integration testing
5. **263 Tests** - Up from 11 in Session 1

### Architecture Growth

```
Session 1: 6 modules, 11 tests
Session 2: 30 modules, 263 tests
Lines: ~15k → ~24k
```

### CI/CD

GitHub Actions configured:
- `ci.yml` - Test on push, build multi-platform artifacts
- `pages.yml` - Deploy docs
- `artifact.yml` - Release artifacts
- `build-sketch.yml` - Sketch builder

## Session 3: Reliability & Autonomy (2025-01-09)

### Focus: Making hyle trustworthy

User feedback: "I still don't trust hyle the way I do trust Claude Code and even Codex."

### Key Additions

1. **Atomic File Writes** (tools.rs)
   - Write to temp file, fsync, rename (POSIX atomic)
   - Read-back verification after every write
   - Timestamped backup rotation (keeps last 3)
   - Maximum file size limit (10MB)

2. **Agent Autonomy Improvements** (agent.rs)
   - Dynamic iteration limits: extend runway when making progress
   - Configurable stuck detection threshold (5 failures, up from 3)
   - AgentConfig::autonomous() and AgentConfig::conservative() presets
   - Progress-based bonus iterations

3. **Autonomy-Focused System Prompt**
   - Emphasizes completing tasks without stopping for confirmation
   - Instructs to try alternatives before giving up
   - Clear guidelines on error recovery

4. **TUI Search** (ui.rs)
   - `/` to search in conversation
   - `n`/`N` for next/previous match
   - Search indicator in title bar
   - Jump-to-match navigation

5. **UX Metrics Framework** (ux_metrics.rs)
   - ResponsivenessTracker: input latency percentiles
   - SmoothnessTracker: token streaming jitter
   - AutonomyTracker: task completion rates
   - UX quality score (0-100)

### Architecture Growth

```
Session 1: 6 modules, 11 tests
Session 2: 30 modules, 263 tests
Session 3: 31 modules, 278 tests
Lines: ~15k → ~24k → ~27k
```

### Test Results

```
running 278 tests
test result: ok. 278 passed; 0 failed
```

## Next Steps

1. Integrate UX metrics into TUI status bar
2. MCP server support
3. Local model support (Ollama)
4. Visual diff preview
5. Self-bootstrapping: hyle develops hyle
