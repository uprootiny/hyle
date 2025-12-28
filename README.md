# hyle

Rust-native code assistant. OpenRouter powered, no JS runtime.

## One-Liner Install

```bash
# Install with cargo (requires Rust toolchain)
cargo install --git https://github.com/uprootiny/hyle && hyle config set key YOUR_KEY
```

Or build from source:

```bash
curl -sSL https://raw.githubusercontent.com/uprootiny/hyle/master/install.sh | bash
```

Get a free API key at [openrouter.ai/keys](https://openrouter.ai/keys)

## Quick Start

```bash
# Interactive mode with free models
hyle --free

# Resume last session (default)
hyle

# Start fresh session
hyle --new

# Background maintenance daemon
hyle --backburner
```

## Usage

```
hyle                          # resume last session
hyle --free [PATHS...]        # choose free model, interactive loop
hyle --new                    # start fresh session
hyle --model <id> [PATHS...]  # use specific model
hyle --task "..." [PATHS...]  # one-shot: produce diff, ask apply
hyle --backburner             # background maintenance daemon
hyle doctor                   # check config, key, network
hyle models --refresh         # refresh models cache
hyle sessions --list          # list saved sessions
hyle sessions --clean         # cleanup old sessions
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
~/.local/state/hyle/sessions/ # Session persistence
```

## Features

- **Free models**: 35+ free models on OpenRouter
- **Session persistence**: Resume conversations across restarts
- **Fuzzy picker**: Incremental search for models
- **SSE streaming**: Real-time token display
- **Telemetry**: CPU, memory, token, latency traces
- **Auto-throttle**: Backs off under pressure
- **Backburner mode**: LLM-powered maintenance daemon
- **Git hygiene**: Commit message analysis, atomic commit suggestions
- **Skills system**: Extensible tools and subagents

## Backburner Mode

Run `hyle --backburner` for intelligent background maintenance:

- CLI feature testing
- Session cleanup
- Git status and hygiene checks
- Cargo build/check monitoring
- LLM-powered improvement suggestions
- Feature progress dashboard

## Architecture

```
src/
├── main.rs       # CLI parsing, command dispatch
├── config.rs     # XDG paths, secure key storage
├── models.rs     # Model list caching, free filter
├── client.rs     # OpenRouter SSE streaming
├── session.rs    # Conversation persistence
├── backburner.rs # Intelligent maintenance daemon
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
