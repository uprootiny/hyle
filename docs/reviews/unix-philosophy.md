# Unix Philosophy Review

**Reviewer:** Unix Greybeard
**Date:** 2025-12-28
**Scope:** Full codebase architecture analysis

---

## Executive Summary

Hyle is ambitious but over-engineered. At 25 modules and 18K lines, it violates core Unix principles of composability and single responsibility. The codebase conflates "featureful" with "well-designed."

---

## Principles Evaluated

> *"Make each program do one thing well."*
> *"Write programs to work together."*
> *"Write programs to handle text streams."*
> — Doug McIlroy

---

## Findings

### 1. Monolithic Dependency Coupling

**Location:** `src/main.rs:11-34`

25 internal modules with deep cross-dependencies:
- `ui.rs` imports 12+ modules
- `skills.rs` imports bootstrap, backburner, prompts
- `agent.rs` does both parsing AND execution

**Problem:** A single change to `telemetry` ripples through 8 modules. Can't extract skills as standalone tools. Can't swap UI backends.

**Recommendation:** Split into clear layers:
```
cli/          # Command parsing, arg handling
core/         # Session, context, models (data only)
tools/        # One file per tool, standalone
tui/          # Orchestrator, calls tools
```

---

### 2. Slash Commands as God Object

**Location:** `src/skills.rs:537-629`

100-line match statement handling 30+ subcommands, each with special-case logic. 1,500 lines of monolithic dispatch.

**Problem:** Not composable. Can't reuse from web/API. Minimal tests (6 in entire file).

**Recommendation:**
```
skills/
├── mod.rs      # 30-line dispatcher
├── build.rs    # ~100 lines
├── test.rs     # ~100 lines
├── git.rs      # ~100 lines
└── analyze.rs  # ~100 lines
```

Each one testable, reusable, composable.

---

### 3. Three Tool Execution Paths

**Locations:**
- `agent.rs:31-44` — parses tool calls from LLM
- `tools.rs:19-43` — defines ToolCall, ToolExecutor
- `skills.rs:126-158` — tool_shell() for slash commands

**Problem:** Feature creep, not architecture. Tool definitions exist in three places. Easy to introduce bugs where one path breaks.

**Recommendation:** Single ToolRegistry trait:
```rust
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn exec(&self, args: &Value) -> Result<ToolOutput>;
}

pub struct ToolRegistry { tools: HashMap<String, Box<dyn Tool>> }
```

All paths use same interface.

---

### 4. Dead Cognitive Architecture

**Location:** `src/cognitive.rs` (36K lines with `#![allow(dead_code)]`)

Momentum tracking, StuckDetector, SalienceContext — none wired to main loop. Speculative code pretending to be architecture.

**Recommendation:** Delete entirely. Write when needed, based on real requirements. Right now it's technical debt.

---

### 5. Skills.rs — 1,500 Lines of Dispatch

**Location:** `src/skills.rs` (51K file)

Contains: tool definitions, git operations, skill definitions, subagent definitions, tool registry, 30+ slash command handlers. All inline.

**Recommendation:** Split each slash command to its own file. Dispatch becomes 30 lines.

---

### 6. No Core/UI Boundary

**Location:** `src/ui.rs:29-43`

UI layer directly imports and calls 10+ modules: client, models, session, telemetry, tools, agent, eval, intent, cognitive.

**Problem:** Can't run as API server. Can't run batch mode without TUI. Can't test core without TUI infrastructure.

**Recommendation:**
```rust
pub struct Hyle {
    session: Session,
    client: Client,
    executor: ToolExecutor,
}

impl Hyle {
    pub async fn send_message(&mut self, msg: &str) -> Result<Response>;
    pub async fn execute_tool(&mut self, tool: &str, args: &Value) -> Result<String>;
}
```

UI, server, batch mode all call `Hyle::*`.

---

## Summary Table

| # | Problem | Location | Fix |
|---|---------|----------|-----|
| 1 | Monolithic coupling | main.rs:11-34 | Split: cli → core → tools layers |
| 2 | God object skills.rs | skills.rs:537-629 | One file per command (~100 lines each) |
| 3 | Three tool paths | agent/tools/skills | Single ToolRegistry trait |
| 4 | 36K dead code | cognitive.rs | Delete, rewrite when needed |
| 5 | No core/UI boundary | ui.rs:29-43 | Extract Hyle struct with methods |

---

## What You're Doing Right

- Excellent testing discipline (40+ tests in tools.rs)
- Clear intent tracking (intent.rs)
- Clean session persistence layer
- Good config system (XDG paths, secure permissions)
- Reasonable dependency count (38 crates)

---

## The Greybeard's Final Word

> *"The key insight is that it's better to have 100 functions operate on one data structure than 10 functions on 10 data structures."*
> — Rob Pike

**Throw away cognitive.rs. Break skills.rs into 20 files. Extract Hyle core. Make tools composable.**

When you do that, someone can:
- Use hyle as a library
- Add web UI without touching CLI code
- Test tool execution without TUI
- Swap OpenRouter for local LLM with 1 file change
- Add new slash command in 1 file, not 10-line additions to match statement
