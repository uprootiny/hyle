# Agentic Loop Design: Fluid & Nuanced

## Multi-LLM Cognitive Architecture

Use multiple models as specialized "cognitive processes":

```
┌──────────────────────────────────────────────────────────────────┐
│                         ORCHESTRATOR                              │
│                                                                   │
│  ┌──────────────┐   ┌──────────────┐   ┌──────────────┐          │
│  │   EXECUTOR   │   │  SUMMARIZER  │   │    SANITY    │          │
│  │  (powerful)  │   │    (free)    │   │    (free)    │          │
│  │              │   │              │   │              │          │
│  │ • Reasoning  │   │ • Compress   │   │ • Validate   │          │
│  │ • Tool use   │   │ • Extract    │   │ • Detect     │          │
│  │ • Code gen   │   │   key facts  │   │   anomalies  │          │
│  │ • Decisions  │   │ • Maintain   │   │ • Check      │          │
│  │              │   │   timeline   │   │   progress   │          │
│  └──────┬───────┘   └──────┬───────┘   └──────┬───────┘          │
│         │                  │                  │                   │
│         ▼                  ▼                  ▼                   │
│  ┌─────────────────────────────────────────────────────────┐     │
│  │                    CONTEXT LAYERS                        │     │
│  │  ┌─────────┐  ┌─────────────┐  ┌───────────────────┐    │     │
│  │  │ Working │  │   Summary   │  │   Project Facts   │    │     │
│  │  │ Memory  │  │   Memory    │  │   (from sanity)   │    │     │
│  │  │ (full)  │  │ (compressed)│  │                   │    │     │
│  │  └─────────┘  └─────────────┘  └───────────────────┘    │     │
│  └─────────────────────────────────────────────────────────┘     │
└──────────────────────────────────────────────────────────────────┘
```

### The Three Processes

**EXECUTOR** (main model - could be expensive)
- Does actual reasoning and tool execution
- Sees: working memory + relevant summaries
- Outputs: tool calls, code, decisions

**SUMMARIZER** (free model - runs async)
- Compresses completed exchanges
- Extracts key facts: "user wants X", "file Y was modified", "error Z occurred"
- Maintains a timeline of what happened
- Runs after each exchange completes

**SANITY CHECKER** (free model - runs periodically)
- Reviews recent actions against stated goal
- Detects: loops, drift, stuck states, contradictions
- Outputs: confidence score, warnings, suggestions
- Runs every N iterations or on request

### Why This Works

1. **Token efficiency**: Expensive model sees focused context
2. **Parallel processing**: Summarizer runs while user reads output
3. **Error catching**: Independent model spots what executor misses
4. **Memory management**: Summaries prevent context overflow
5. **Cost control**: Heavy lifting on cheap/free models

### Example Flow

```
User: "Refactor the auth module to use JWT"

EXECUTOR (claude-3):
  "I'll start by reading the current auth implementation..."
  → tools: read auth.rs

[SUMMARIZER runs async]
  Input: user request + tool output
  Output: "Task: refactor auth to JWT. Read auth.rs (450 lines,
           uses session-based auth, depends on user.rs)"

EXECUTOR continues with tool results + summary context...

[After 3 iterations, SANITY runs]
  Input: goal + action history
  Output: {
    on_track: true,
    progress: 0.4,
    concerns: ["hasn't checked existing tests yet"],
    suggestion: "consider reading auth_test.rs before modifying"
  }

ORCHESTRATOR injects sanity output into next executor prompt...
```

### Free Models for Each Role

```rust
struct CognitiveConfig {
    executor: ModelId,      // User's choice - could be expensive
    summarizer: ModelId,    // mistral-7b-free, llama-free, etc.
    sanity: ModelId,        // Same, or different free model
}

// Good free candidates on OpenRouter:
// - mistralai/mistral-7b-instruct:free
// - meta-llama/llama-3-8b-instruct:free
// - google/gemma-7b-it:free
```

### Sanity Check Protocol

```rust
struct SanityCheck {
    // Run every N iterations
    interval: u8,

    // Or when triggered by:
    triggers: Vec<SanityTrigger>,
}

enum SanityTrigger {
    IterationCount(u8),      // Every N iterations
    ToolFailure,             // After any tool fails
    LargeOutput,             // After unexpectedly large output
    UserIdle(Duration),      // User hasn't responded in X time
    ContextThreshold(usize), // Context approaching limit
    Explicit,                // User types /sanity
}

struct SanityResult {
    on_track: bool,
    confidence: f32,         // 0.0 to 1.0
    progress_estimate: f32,  // How far through task
    concerns: Vec<String>,
    suggestions: Vec<String>,
    should_pause: bool,
    should_abort: bool,
}
```

---

## Core Insight

The loop should feel like a **conversation flow**, not a mechanical repeat.
It should have intuition about:
- When to proceed confidently
- When to slow down and check
- When to stop and ask
- When something's wrong

## State Machine View

```
                    ┌─────────────┐
                    │   ASSESS    │◄──────────────────┐
                    └──────┬──────┘                   │
                           │                          │
           ┌───────────────┼───────────────┐          │
           ▼               ▼               ▼          │
    ┌──────────┐    ┌──────────┐    ┌──────────┐     │
    │ EXECUTE  │    │  PAUSE   │    │ COMPLETE │     │
    │ (auto)   │    │ (confirm)│    │  (done)  │     │
    └────┬─────┘    └────┬─────┘    └──────────┘     │
         │               │                            │
         │               │ user confirms              │
         │               ▼                            │
         └───────────────┴────────────────────────────┘
```

## Tool Risk Categories

```rust
enum ToolRisk {
    Safe,       // read, glob, grep - auto-execute always
    Cautious,   // write, edit - auto-execute if context suggests intent
    Confirm,    // delete, shell with rm/mv - pause for confirmation
    Dangerous,  // force flags, sudo, etc - always confirm
}
```

## Context Tiering (Salience)

Instead of dumping all tool results:

```
Tier 1: Current Focus     (full detail, last 1-2 tool results)
Tier 2: Recent Context    (summaries, last 5 operations)
Tier 3: Session Memory    (compressed, key facts only)
Tier 4: Project Knowledge (structure, conventions)
```

The LLM sees a **focused view** with the most salient information prominent.

## Momentum & Confidence

Track a rolling window of success/failure:

```rust
struct Momentum {
    window: VecDeque<ToolOutcome>,  // last N operations
    score: f32,                      // 0.0 to 1.0
}

impl Momentum {
    fn should_slow_down(&self) -> bool {
        self.score < 0.5  // More failures than successes
    }

    fn should_pause(&self) -> bool {
        self.score < 0.3  // Things are going wrong
    }
}
```

## Natural Continuation

Instead of a fixed prompt, the continuation should be **derived from context**:

```rust
fn continuation_prompt(state: &AgenticState) -> Option<String> {
    // If LLM explicitly said what it would do next, use that
    if let Some(next_step) = state.extract_stated_next_step() {
        return Some(format!("Proceed with: {}", next_step));
    }

    // If tools succeeded, minimal nudge
    if state.last_tools_succeeded() {
        return Some("Continue.".into());  // Minimal, natural
    }

    // If tools failed, acknowledge and adapt
    if state.last_tools_failed() {
        return Some("The previous operation failed. Reassess and try a different approach.".into());
    }

    // If seems stuck, prompt reflection
    if state.seems_stuck() {
        return Some("Take a step back. What's the current state and what should happen next?".into());
    }

    None  // Let the tool results speak for themselves
}
```

## Breakpoint Conditions

```rust
enum Breakpoint {
    // User-triggered
    UserInterrupt,

    // Risk-based
    DestructiveOperation(String),

    // Progress-based
    MaxIterations(u8),
    NoProgress { iterations_without_change: u8 },

    // Confidence-based
    LowMomentum,
    RepeatedFailures,

    // Semantic
    TaskComplete,
    NeedsUserInput,
    Ambiguous,
}
```

## The Assessment Phase

Before each iteration, assess the situation:

```rust
fn assess(state: &AgenticState, last_response: &str) -> LoopDecision {
    // 1. Check for explicit completion signals
    if contains_completion_signal(last_response) {
        return LoopDecision::Complete;
    }

    // 2. Check for user input requests
    if asks_for_clarification(last_response) {
        return LoopDecision::PauseForInput;
    }

    // 3. Parse pending tool calls
    let tools = parse_tool_calls(last_response);
    if tools.is_empty() {
        return LoopDecision::Complete;
    }

    // 4. Assess risk of pending tools
    let max_risk = tools.iter().map(|t| t.risk_level()).max();
    if max_risk >= ToolRisk::Confirm {
        return LoopDecision::PauseForConfirm(tools);
    }

    // 5. Check momentum
    if state.momentum.should_pause() {
        return LoopDecision::PauseCheck;
    }

    // 6. Check iteration limits
    if state.iteration >= state.max_iterations {
        return LoopDecision::MaxIterations;
    }

    // 7. Check for stuck patterns
    if state.stuck_detector.is_stuck() {
        return LoopDecision::Stuck;
    }

    // All clear - proceed
    LoopDecision::Continue(tools)
}
```

## Stuck Detection

Detect when the agent is spinning:

```rust
struct StuckDetector {
    recent_actions: VecDeque<ActionHash>,  // Last N action signatures
    repeated_errors: HashMap<String, u8>,  // Error message -> count
}

impl StuckDetector {
    fn record(&mut self, action: &ToolCall, result: &ToolResult) {
        let hash = self.hash_action(action);
        self.recent_actions.push_back(hash);

        if !result.success {
            *self.repeated_errors.entry(result.error_type()).or_default() += 1;
        }
    }

    fn is_stuck(&self) -> bool {
        // Same action repeated 3+ times
        self.has_repeated_pattern(3) ||
        // Same error 3+ times
        self.repeated_errors.values().any(|&c| c >= 3)
    }
}
```

## Context Compression

As conversation grows, compress older context:

```rust
fn compress_context(history: &[Message], budget: usize) -> Vec<Message> {
    let mut result = Vec::new();
    let mut used = 0;

    // Always keep: system prompt, last 2 exchanges full
    // Summarize: older exchanges
    // Drop: redundant tool outputs

    for msg in history.iter().rev() {
        let tokens = estimate_tokens(&msg.content);

        if used + tokens > budget {
            // Compress this message
            let summary = summarize_message(msg);
            result.push(summary);
            used += estimate_tokens(&summary.content);
        } else {
            result.push(msg.clone());
            used += tokens;
        }
    }

    result.reverse();
    result
}
```

## Parallel Tool Execution

Some tools can run in parallel:

```rust
fn partition_tools(tools: Vec<ToolCall>) -> (Vec<ToolCall>, Vec<ToolCall>) {
    let (parallel, sequential): (Vec<_>, Vec<_>) = tools
        .into_iter()
        .partition(|t| t.is_read_only() && !t.depends_on_previous());

    (parallel, sequential)
}

// Execute parallel tools concurrently
let parallel_results = futures::future::join_all(
    parallel.iter().map(|t| execute_tool(t))
).await;
```

## User Control Knobs

Let users tune the loop behavior:

```rust
struct LoopConfig {
    auto_execute_reads: bool,      // Default: true
    auto_execute_writes: bool,     // Default: false in cautious mode
    confirm_shell_commands: bool,  // Default: true
    max_iterations: u8,            // Default: 10
    pause_every_n: Option<u8>,     // Pause every N iterations for check-in
    context_budget: usize,         // Max tokens for context
    momentum_threshold: f32,       // When to slow down
}
```

## Implementation Priority

1. **Tool Risk Categories** - Know what's safe vs dangerous
2. **Natural Continuation** - Don't use robotic prompts
3. **Stuck Detection** - Prevent infinite loops
4. **Momentum Tracking** - Adapt speed to success rate
5. **Context Tiering** - Keep context focused
6. **Parallel Execution** - Speed up read operations

## The Fluid Feel

The goal is that the loop feels like a skilled assistant:
- Proceeds confidently when things are clear
- Slows down when uncertain
- Stops to check when risk is high
- Asks for help when stuck
- Knows when it's done

Not a robot going "TOOL EXECUTED. CONTINUING. TOOL EXECUTED. CONTINUING."

But a collaborator: "I'll check the file structure... found it. Now let me read the config...
okay I see the issue. I'll fix line 42... done. Let me verify it compiles... it does.
The task is complete."
