# CLI/Web UX Analysis

**Reviewer:** UX Designer
**Date:** 2025-12-28
**Scope:** `src/ui.rs`, `src/skills.rs`, `src/server.rs`

---

## Executive Summary

6 usability issues affecting discoverability, feedback, and progressive disclosure. Power users can't debug system behavior; new users struggle with feature discovery.

---

## Findings

### 1. Hidden Slash Commands — Discovery Friction

**Location:** `src/skills.rs:627`, `src/ui.rs:1741`

```rust
_ => None, // Unknown command, let LLM handle
```

**Problem:** Unknown slash commands silently fall through to LLM. User typing `/unfamiliar` gets no indication hyle tried to interpret it. Typos sent to LLM create confusion.

**Recommendation:**
- Show tooltip: `[Did you mean: /ls, /find, /grep? Or send to LLM?]`
- 500ms delay before fallback, allowing user to see feedback
- Fuzzy match suggestions for partial commands

---

### 2. Help System Buried & Overwhelming

**Location:** `src/skills.rs:691-757`

```rust
fn slash_help_full() -> SlashResult {
    // 50+ lines, 8 categories, all at once
```

Web UI has **zero** help mechanism — no `/help`, no tooltips, no API documentation.

**Recommendation:**
- **CLI:** Contextual help in input footer:
  ```
  Tip: /build to compile | /test to run tests | ? for help
  ```
- **Web:** Collapsible help sidebar with API endpoints and cURL examples

---

### 3. Confusing Rate Limit UX

**Location:** `src/ui.rs:1544-1560`

**Problem:** On rate limit:
- Auto-retry happens silently
- Available models list dumped without context
- No explanation of what happened or what user should do

**Recommendation:**
```
[⚠ Rate Limited] llama-3.2-3b is out of tokens

Switching to fallback: qwen-2-7b-instruct ✓
Retrying in 2s... [Esc to cancel]
```

---

### 4. Tool Output Truncated Silently

**Location:** `src/ui.rs:1296-1304`

```rust
for line in tool_feedback.lines().take(20) {
    state.output.push(format!("  {}", line));  // No indicator of truncation
```

**Problem:** 500-line output shows only 20 lines. No indicator that 480 lines were discarded. User doesn't know if critical information was lost.

**Recommendation:**
```
[Tool: /read_file src/main.rs]
  ✓ Read 2,450 bytes (42 lines)
  Showing: First 20 lines (… 22 lines in /Log tab)
```

---

### 5. Cognitive State Invisible

**Location:** `src/ui.rs:914-951`

```rust
fn should_continue_loop(&self, tool_results: &str) -> LoopDecision {
    // Returns: MaxIterations, Stuck, PauseConcern, Complete, Continue
    // User sees nothing
```

**Problem:** Intent tracking, momentum, stuck detection all running invisibly. User has no idea why AI stopped or changed behavior.

**Recommendation:** Add state display in Telemetry tab:
```
─── Agentic Loop ───
Iteration: 3/10
Momentum: 72% (2 failures, 1 success)
Stuck Score: 12/50 (not stuck)
```

On pause:
```
[Pausing: Multiple tool failures detected]
(r) Retry | (d) Different approach | (c) Continue anyway
```

---

### 6. Web UI ↔ CLI Asymmetry

**Locations:**
- `src/ui.rs:215-230` — 8 different tabs
- `src/server.rs:296-532` — Full HTML, zero help

**Problem:**
- CLI has 8 tabs, navigation only via Tab key (not documented in UI)
- Web has chat interface but no status indicator, no model badge, no API docs

**Recommendation:**

**CLI startup:**
```
Hyle v0.2.0 | Ready

Tips:
• Tab to cycle views (Chat → Telemetry → Log → Sessions)
• /help for slash commands
• ? anytime for quick help
```

**Web header:**
```html
<header>
    <h1>hyle</h1>
    <nav>
        <a href="#status">Status</a>
        <a href="#sessions">Sessions</a>
        <a href="#api-docs">API</a>
    </nav>
    <span id="model-badge">llama-3.2-3b | Ready</span>
</header>
```

---

## Summary Table

| Issue | Category | Severity | Quick Fix |
|-------|----------|----------|-----------|
| Slash typos → LLM silently | Discoverability | High | "Unknown command" feedback + delay |
| /help buried, web has none | Discoverability | High | Contextual tips in footer |
| Rate limit UX confusing | Feedback | High | Explicit state messages |
| Tool output truncated silently | Feedback | Medium | "(+N lines in /Log)" indicator |
| Cognitive state invisible | Transparency | Medium | Show loop state in Telemetry |
| Web UI has no navigation | Discoverability | Medium | Add header with status/docs |

---

## Design Principles Applied

These recommendations follow:

- **Unix Philosophy:** Output is input to next tool
- **Suckless:** Clarity, simplicity, user agency
- **Progressive Disclosure:** Show what matters now, hide complexity until needed
- **Transparency over Magic:** Users should understand system behavior
