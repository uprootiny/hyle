# Release Plan: hyle v0.3.0

**Theme: Reliability & Autonomy**

## Current State (v0.2.0)

| Metric | Value |
|--------|-------|
| Tests | 254 passing |
| Modules | 31 source files |
| Lines | ~25,000 |

### Recent Additions (Session 2)
- [x] Atomic file writes with verification
- [x] Timestamped backup rotation
- [x] Handoff from Claude Code
- [x] Prompt queue for async input
- [x] Type-safe Role/LogKind enums
- [x] UX metrics framework

## v0.3.0 Goals

### 1. Agent Autonomy (Priority: HIGH)

**Problem**: Agent gives up too quickly (3 failures = stuck)

**Solution**:
- Dynamic iteration limits based on progress
- Retry failed operations with alternatives
- Better error classification (transient vs fatal)

```rust
// Current
max_iterations: 20,
consecutive_failures >= 3  // stuck

// Proposed
base_iterations: 20,
extend_on_progress: true,  // +5 iterations if making progress
max_consecutive_failures: 5,
retry_with_alternatives: true,
```

### 2. UX Quality Tracking (Priority: HIGH)

**Problem**: No visibility into user experience quality

**Solution**:
- Integrate ResponsivenessTracker into TUI
- Track first-token latency, streaming smoothness
- Display quality score in status bar
- `/metrics` command for detailed view

### 3. Autonomy-Focused Prompts (Priority: MEDIUM)

**Problem**: System prompt doesn't emphasize autonomy

**Solution**: Update `code_assistant_prompt()` to include:
- "Complete tasks without asking unless truly blocked"
- "Make reasonable decisions autonomously"
- "Retry with alternatives before giving up"

### 4. Error Recovery (Priority: MEDIUM)

**Problem**: First error often causes cascade failure

**Solution**:
- Classify errors: transient (retry) vs fatal (stop)
- Implement retry with backoff
- Try alternative approaches (different tool, different path)

## Implementation Order

1. **Agent autonomy config** - Adjustable limits, progress detection
2. **System prompt update** - Autonomy-focused language
3. **UX metrics integration** - Hook into TUI event loop
4. **Error recovery** - Retry logic with classification
5. **Slash command** - `/metrics` for quality display
6. **README update** - Roadmap, stats, changelog
7. **Version bump** - 0.2.0 → 0.3.0

## Success Criteria

| Metric | Target |
|--------|--------|
| Autonomous completion rate | >70% of tasks |
| Average iterations to complete | <10 |
| First-token latency p95 | <500ms |
| Stuck rate | <10% |

## Not in Scope (v0.4.0+)

- MCP server support
- Local model support (Ollama)
- Visual diff preview
- Team collaboration
