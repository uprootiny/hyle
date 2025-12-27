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

## Next Steps

1. Define full affordance tree
2. Implement essential affordances (file read, patch, apply)
3. Add session logging to ~/.local/state/hyle/
4. Git hygiene and public repo
