# hyle

Rust-native code assistant. OpenRouter powered, no JS runtime.

## Quick Start

```bash
# Build
cargo build --release

# Install
cp target/release/hyle ~/.local/bin/

# Set API key (get free key at openrouter.ai/keys)
hyle config set key sk-or-v1-...

# Run with free models only
hyle --free
```

## Usage

```
hyle --free [PATHS...]        # choose free model, interactive loop
hyle --model <id> [PATHS...]  # use specific model
hyle --task "..." [PATHS...]  # one-shot: produce diff, ask apply
hyle doctor                   # check config, key, network
hyle models --refresh         # refresh models cache
hyle config set key <value>   # set config value
```

## Controls

| Key | Action |
|-----|--------|
| Enter | Send prompt |
| Esc | Quit |
| Tab | Switch tabs (Chat/Telemetry/Log) |
| k | Kill current operation |
| t | Throttle mode |
| f | Full speed mode |
| n | Normal mode |

## Config

```
~/.config/hyle/config.json    # API key, preferences (0600)
~/.cache/hyle/models.json     # Cached model list (24h TTL)
~/.local/state/hyle/          # Session logs
```

## Features

- **Free models**: 35+ free models on OpenRouter
- **Fuzzy picker**: Incremental search for models
- **SSE streaming**: Real-time token display
- **Telemetry**: CPU, memory, token, latency traces
- **Auto-throttle**: Backs off under pressure
- **Skills system**: Extensible tools and subagents

## Architecture

```
src/
├── main.rs       # CLI parsing, command dispatch
├── config.rs     # XDG paths, secure key storage
├── models.rs     # Model list caching, free filter
├── client.rs     # OpenRouter SSE streaming
├── telemetry.rs  # CPU/mem sampling, pressure detection
├── traces.rs     # Token, context, memory, latency traces
├── skills.rs     # Tools, skills, subagents
├── ui.rs         # Fuzzy picker, TUI, controls
└── tools.rs      # File operations, diff generation
```

## Tests

```bash
cargo test
```

## License

MIT
