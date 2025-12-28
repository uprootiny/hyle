# hyle Feature Tree Map

Status Legend: `[ ]` untested | `[x]` passing | `[!]` failing | `[~]` partial

Last updated: 2025-12-28

## Test Results Summary

```
Total: 25 features
Passing: 0
Failing: 0
Untested: 25
Coverage: 0%
```

---

## 1. CLI Commands

### 1.1 Help & Info
```
[ ] hyle --help              # Show usage info
[ ] hyle -h                  # Short form
[ ] hyle doctor              # Health check
[ ] hyle models              # List cached models
[ ] hyle models --refresh    # Force refresh
```

### 1.2 Interactive Mode
```
[ ] hyle                     # Resume last session (default)
[ ] hyle --free              # Free models only
[ ] hyle -f                  # Short form
[ ] hyle --new               # Fresh session
[ ] hyle -n                  # Short form
[ ] hyle --model <id>        # Specific model
[ ] hyle -m <id>             # Short form
```

### 1.3 Task Mode
```
[ ] hyle --task "..."        # One-shot task
[ ] hyle -t "..."            # Short form
```

### 1.4 Session Management
```
[ ] hyle sessions            # List sessions
[ ] hyle sessions --list     # Explicit list
[ ] hyle sessions --clean    # Cleanup old
```

### 1.5 Configuration
```
[ ] hyle config set key X    # Set API key
[ ] hyle config set model X  # Set default model
```

### 1.6 Background Mode
```
[ ] hyle --backburner        # Start daemon
[ ] hyle -b                  # Short form
```

---

## 2. TUI Features

### 2.1 Model Picker
```
[ ] Fuzzy search filtering
[ ] Up/Down navigation
[ ] Enter to select
[ ] Esc to cancel
[ ] Context length display
[ ] Free indicator [FREE]
```

### 2.2 Tabs
```
[ ] Chat tab (default)
[ ] Telemetry tab
[ ] Log tab
[ ] Tab key cycling
```

### 2.3 Chat Interface
```
[ ] Text input
[ ] Enter to send
[ ] Response streaming
[ ] Token count display
[ ] Timing display
```

### 2.4 Controls
```
[ ] k - Kill operation
[ ] t - Throttle mode
[ ] f - Full speed mode
[ ] n - Normal mode
[ ] Esc - Quit
```

### 2.5 Status Bar
```
[ ] Spinner during generation
[ ] CPU sparkline
[ ] Pressure indicator
[ ] Throttle mode display
[ ] Key hints
```

---

## 3. Session Persistence

### 3.1 Session Creation
```
[ ] Auto-create on new conversation
[ ] Generate unique session ID
[ ] Store in ~/.local/state/hyle/sessions/
```

### 3.2 Session Resume
```
[ ] Resume last session by default
[ ] Resume same model within 1 hour
[ ] Show message count on resume
```

### 3.3 Message Storage
```
[ ] Save user messages
[ ] Save assistant messages
[ ] Track token counts
[ ] JSONL format
```

### 3.4 Session Commands
```
[ ] List sessions with stats
[ ] Show session age
[ ] Cleanup old sessions
[ ] Keep last 10 by default
```

---

## 4. API Integration

### 4.1 Key Management
```
[ ] Load from config file
[ ] Load from OPENROUTER_API_KEY env
[ ] Prompt for key if missing
[ ] Secure storage (0600)
```

### 4.2 Model Cache
```
[ ] Fetch from /api/v1/models
[ ] Cache to ~/.cache/hyle/models.json
[ ] 24h TTL
[ ] Force refresh option
```

### 4.3 Streaming
```
[ ] SSE event stream
[ ] Token-by-token display
[ ] Usage tracking
[ ] Error handling
```

---

## 5. Telemetry

### 5.1 System Monitoring
```
[ ] CPU percentage
[ ] Memory usage
[ ] 4Hz sampling rate
```

### 5.2 Pressure Detection
```
[ ] Normal level (<50%)
[ ] Medium level (50-75%)
[ ] High level (75-90%)
[ ] Critical level (>90%)
```

### 5.3 Auto-throttle
```
[ ] Detect high pressure
[ ] Switch to throttled mode
[ ] Log throttle events
```

### 5.4 Visualization
```
[ ] CPU sparkline
[ ] Pressure symbol
[ ] Telemetry tab display
```

---

## 6. Traces

### 6.1 Token Traces
```
[ ] Prompt token count
[ ] Completion token count
[ ] Total running count
[ ] Tokens per second
```

### 6.2 Context Traces
```
[ ] Context usage tracking
[ ] Context window percentage
[ ] Warning threshold
```

### 6.3 Latency Traces
```
[ ] Time to first token (TTFT)
[ ] Total request time
[ ] Rolling average
```

### 6.4 Memory Traces
```
[ ] Process RSS
[ ] Sparkline display
```

---

## 7. Backburner Mode

### 7.1 Lifecycle
```
[ ] Start daemon
[ ] Graceful shutdown (Ctrl-C)
[ ] Cycle-based task rotation
```

### 7.2 Maintenance Tasks
```
[ ] CLI feature tests
[ ] Session cleanup
[ ] Git status check
[ ] Git hygiene analysis
[ ] Cargo check
[ ] LLM suggestions
```

### 7.3 Git Hygiene
```
[ ] Atomic commit detection
[ ] Commit message analysis
[ ] Imperative mood check
[ ] Length validation
[ ] git fsck periodic
```

### 7.4 Dashboard
```
[ ] Feature status summary
[ ] Progress percentage
[ ] Observations log
```

---

## Testing Procedure

For each feature marked `[ ]`:

1. Execute the test case
2. Verify expected behavior
3. Mark as:
   - `[x]` if passing
   - `[!]` if failing (note reason)
   - `[~]` if partial (note what works)
4. Commit progress

Run full test suite:
```bash
cargo test                    # Unit tests
hyle doctor                   # System check
hyle --backburner            # Feature tests
```
