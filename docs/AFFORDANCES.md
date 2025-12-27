# hyle Affordances

Affordances are the actions a user can take within hyle. They are organized by category
and implementation status.

## Affordance Categories

```
AFFORDANCES
├── Navigation
│   ├── [x] Tab switching (Tab key)
│   ├── [x] Scroll output (Up/Down)
│   ├── [ ] Jump to file
│   ├── [ ] Search history
│   └── [ ] Bookmark locations
│
├── Generation Control
│   ├── [x] Send prompt (Enter)
│   ├── [x] Kill operation (k)
│   ├── [x] Throttle mode (t)
│   ├── [x] Full speed mode (f)
│   ├── [x] Normal mode (n)
│   ├── [ ] Pause/resume
│   └── [ ] Regenerate last
│
├── Model Selection
│   ├── [x] Fuzzy picker
│   ├── [x] Free-only filter (--free)
│   ├── [ ] Favorites
│   ├── [ ] Recent models
│   └── [ ] Model comparison
│
├── File Operations
│   ├── [x] Read file (tools::read_file)
│   ├── [x] Generate diff (tools::generate_diff)
│   ├── [ ] Apply patch
│   ├── [ ] Preview changes
│   ├── [ ] Stage changes
│   ├── [ ] Reject changes
│   └── [ ] Undo last change
│
├── Session Management
│   ├── [x] Config persistence
│   ├── [x] Model caching
│   ├── [ ] Session save
│   ├── [ ] Session restore
│   ├── [ ] Export conversation
│   └── [ ] Branch conversation
│
├── Telemetry
│   ├── [x] CPU monitoring
│   ├── [x] Memory monitoring
│   ├── [x] Pressure detection
│   ├── [x] Auto-throttle
│   ├── [x] Sparkline display
│   ├── [ ] Network monitoring
│   └── [ ] Token rate graph
│
├── UI Feedback
│   ├── [x] Spinner during generation
│   ├── [x] Tokens/sec display
│   ├── [x] Token count
│   ├── [ ] Time-to-first-token
│   ├── [ ] ETA estimation
│   ├── [ ] Progress bar
│   └── [ ] Sound notifications
│
├── CLI Commands
│   ├── [x] --free (free model picker)
│   ├── [x] --model <id>
│   ├── [x] --task <text>
│   ├── [x] doctor
│   ├── [x] models --refresh
│   ├── [x] config set key
│   ├── [ ] config show
│   ├── [ ] history
│   └── [ ] clear-cache
│
└── Safety
    ├── [x] Secure key storage (0600)
    ├── [x] Rate limiting
    ├── [ ] Dangerous command warnings
    ├── [ ] File permission checks
    └── [ ] Sandboxed execution
```

## Essential Affordances (Must Have)

These 6 affordances form the core loop:

### 1. Send Prompt (Enter)
**Status:** Implemented
**Location:** `src/ui.rs:375-430`

User types a prompt, presses Enter, request is sent to OpenRouter.
```rust
KeyCode::Enter => {
    if !state.input.is_empty() {
        let prompt = state.input.clone();
        state.input.clear();
        // ... spawn API call
    }
}
```

### 2. Kill Operation (k)
**Status:** Implemented
**Location:** `src/ui.rs:336-339`

Immediately stops current generation.
```rust
KeyCode::Char('k') if state.is_generating => {
    state.throttle = ThrottleMode::Killed;
    state.log("Operation killed");
}
```

### 3. Throttle Mode (t)
**Status:** Implemented
**Location:** `src/ui.rs:340-343`

Reduces request rate under pressure.
```rust
KeyCode::Char('t') => {
    state.throttle = ThrottleMode::Throttled;
    state.log("Switched to throttled mode");
}
```

### 4. Model Selection
**Status:** Implemented
**Location:** `src/ui.rs:107-175`

Fuzzy search through available models.
```rust
let filtered: Vec<_> = if filter.is_empty() {
    models.iter().collect()
} else {
    models.iter()
        .filter_map(|m| matcher.fuzzy_match(&m.id, &filter).map(|score| (m, score)))
        .sorted_by(|a, b| b.1.cmp(&a.1))
        .map(|(m, _)| m)
        .collect()
};
```

### 5. Pressure Detection
**Status:** Implemented
**Location:** `src/telemetry.rs:77-118`

Monitors CPU and auto-throttles on spikes.
```rust
pub fn detect_spike(&self, sample: &Sample) -> bool {
    if let Some(avg_cpu) = self.average_cpu() {
        if sample.cpu_percent > avg_cpu + 30.0 {
            return true;
        }
    }
    sample.cpu_percent > 90.0
}
```

### 6. Config Persistence
**Status:** Implemented
**Location:** `src/config.rs:85-101`

Securely stores API key and preferences.
```rust
pub fn save(&self) -> Result<()> {
    let content = serde_json::to_string_pretty(self)?;
    fs::write(&path, &content)?;
    let mut perms = fs::metadata(&path)?.permissions();
    perms.set_mode(0o600);  // Owner read/write only
    fs::set_permissions(&path, perms)?;
    Ok(())
}
```

## Affordance Implementation Guidelines

### Keyboard Bindings

| Key | Context | Action |
|-----|---------|--------|
| Enter | Chat tab, not generating | Send prompt |
| Esc | Any | Quit application |
| Tab | Any | Cycle tabs |
| k | Generating | Kill operation |
| t | Any | Throttle mode |
| f | Not generating | Full speed mode |
| n | Any | Normal mode |
| Up/Down | Chat tab | Scroll output |

### State Machine

```
IDLE ──Enter──▶ GENERATING ──Done──▶ IDLE
                    │
                    ├──k──▶ KILLED ──▶ IDLE
                    │
                    └──t──▶ THROTTLED ──▶ GENERATING
```

### Telemetry Thresholds

| Level | CPU % | Action |
|-------|-------|--------|
| Low | <50% | Normal operation |
| Medium | 50-75% | No action |
| High | 75-90% | Warning display |
| Critical | >90% | Auto-throttle |

## Testing Affordances

Each affordance should have:
1. Unit test for the core logic
2. Integration test for the full flow
3. Manual test script

Example test for throttle:
```rust
#[test]
fn test_throttle_mode() {
    assert_eq!(ThrottleMode::Normal.delay_multiplier(), 1.0);
    assert_eq!(ThrottleMode::Throttled.delay_multiplier(), 3.0);
}
```

## Future Affordances

### High Priority
- [ ] Apply patch - actually modify files
- [ ] Session logging - track all interactions
- [ ] ETA estimation - predict completion time

### Medium Priority
- [ ] File tree navigation
- [ ] Multi-file context
- [ ] Conversation branching

### Low Priority
- [ ] Sound notifications
- [ ] Custom themes
- [ ] Plugin system
