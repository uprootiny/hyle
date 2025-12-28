# hyle Feature Tree Map

Status: [ ] untested, [x] tested, [!] needs fix

## 1. Core CLI
```
hyle
├── --help, -h                    [ ] Show help text
├── --free, -f                    [ ] Free models only mode
├── --new, -n                     [ ] Start fresh session
├── --model, -m <id>              [ ] Use specific model
├── --task, -t <text>             [ ] One-shot task mode
├── --backburner, -b              [ ] Background maintenance
└── (default)                     [ ] Resume last session
```

## 2. Subcommands
```
hyle <subcommand>
├── doctor                        [ ] Health check
│   ├── Config file check         [ ]
│   ├── API key check             [ ]
│   ├── Models cache check        [ ]
│   └── Network connectivity      [ ]
├── models                        [ ] List models
│   └── --refresh                 [ ] Force refresh cache
├── sessions                      [ ] Session management
│   ├── --list, -l                [ ] List sessions
│   └── --clean                   [ ] Cleanup old sessions
└── config set <key> <value>      [ ] Configure
    ├── key                       [ ] Set API key
    └── model                     [ ] Set default model
```

## 3. Interactive Mode (TUI)
```
TUI Features
├── Model picker                  [ ] Fuzzy search model list
├── Tabs                          [ ]
│   ├── Chat                      [ ] Conversation view
│   ├── Telemetry                 [ ] System metrics
│   └── Log                       [ ] Event log
├── Controls                      [ ]
│   ├── Enter                     [ ] Send prompt
│   ├── Esc                       [ ] Quit
│   ├── Tab                       [ ] Switch tabs
│   ├── k                         [ ] Kill operation
│   ├── t                         [ ] Throttle mode
│   ├── f                         [ ] Full speed mode
│   └── n                         [ ] Normal mode
└── Status bar                    [ ] Pressure indicator
```

## 4. Session Persistence
```
Sessions (~/.local/state/hyle/sessions/)
├── Session creation              [ ] New session on --new
├── Session resume                [ ] Auto-resume on restart
├── User message saving           [ ] Persist user prompts
├── Assistant message saving      [ ] Persist responses
├── Token tracking                [ ] Track token usage
├── Session listing               [ ] List all sessions
├── Session cleanup               [ ] Remove old sessions
└── Cross-session context         [ ] Resume conversation
```

## 5. Telemetry & Traces
```
Telemetry
├── CPU monitoring                [ ] Real-time CPU %
├── Memory monitoring             [ ] RSS memory tracking
├── Pressure detection            [ ]
│   ├── Normal level              [ ]
│   ├── Medium level              [ ]
│   ├── High level                [ ]
│   └── Critical level            [ ]
├── Auto-throttle                 [ ] Reduce on pressure
├── Sparkline visualization       [ ]
│   ├── CPU sparkline             [ ]
│   └── Memory sparkline          [ ]
└── Token traces                  [ ]
    ├── Prompt tokens             [ ]
    ├── Completion tokens         [ ]
    ├── Context usage             [ ]
    └── Latency (TTFT, total)     [ ]
```

## 6. Backburner Mode
```
Maintenance Daemon
├── Session cleanup               [ ] Keep last 10 sessions
├── Git status check              [ ] Uncommitted changes
├── Git garbage collection        [ ] git gc --auto
├── Cargo outdated check          [ ] Rust deps (if project)
├── npm audit check               [ ] Node deps (if project)
├── Graceful shutdown             [ ] Ctrl-C handling
└── Non-intrusive timing          [ ] 5-min intervals
```

## 7. API Integration
```
OpenRouter API
├── API key from config           [ ] ~/.config/hyle/config.json
├── API key from env              [ ] OPENROUTER_API_KEY
├── API key prompt                [ ] Interactive input
├── SSE streaming                 [ ] Token-by-token
├── Model list fetch              [ ] /api/v1/models
├── Model cache                   [ ] ~/.cache/hyle/models.json
├── Free model filter             [ ] Zero-cost models
├── Token usage tracking          [ ] prompt + completion
└── Error handling                [ ] Rate limits, network
```

## 8. Skills/Tools (Scaffolded)
```
Tool System
├── read_file                     [ ] Read file contents
├── write_file                    [ ] Write/overwrite file
├── glob                          [ ] Pattern matching
├── shell                         [ ] Execute commands
├── git.*                         [ ]
│   ├── status                    [ ]
│   ├── diff                      [ ]
│   ├── add                       [ ]
│   ├── commit                    [ ]
│   └── log                       [ ]
├── Skill definitions             [ ] Extensible skills
└── Subagent definitions          [ ] Task delegation
```

## 9. File Operations
```
Diff & Patch
├── Unified diff generation       [ ] similar crate
├── Diff preview                  [ ] Color-coded
└── Patch application             [ ] Apply changes
```

## Testing Matrix

| Feature | Unit Test | Integration | Manual |
|---------|-----------|-------------|--------|
| CLI parsing | - | - | - |
| Config | PASS | - | - |
| Models | PASS | - | - |
| Telemetry | PASS | - | - |
| Traces | PASS | - | - |
| Session | PASS | - | - |
| Skills | PASS | - | - |
| Tools | PASS | - | - |
| TUI | - | - | - |
| Streaming | - | - | - |
