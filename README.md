# hyle

[![hyle.lol](https://img.shields.io/badge/web-hyle.lol-7aa2f7?style=flat-square)](https://hyle.lol)
[![Rust](https://img.shields.io/badge/rust-1.75+-orange?style=flat-square)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/license-MIT-9ece6a?style=flat-square)](LICENSE)

**Rust-native autonomous code assistant.** Paste a sketch, get a project.

OpenRouter powered. No JS runtime. Single binary.

## Prerequisites

**Install Rust via rustup** (not apt/dnf/pacman):
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env
```

> **Note**: If you have apt-installed Rust, remove it first:
> ```bash
> sudo apt remove rustc cargo  # Ubuntu/Debian
> sudo apt autoremove
> ```

## Install

```bash
# One-liner install
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
hyle --nonfree [PATHS...]     # paid models only (excludes free tier)
hyle --new                    # start fresh session
hyle --handoff                # import context from Claude Code
hyle --model <id> [PATHS...]  # use specific model
hyle --task "..." [PATHS...]  # one-shot: produce diff, ask apply
hyle --backburner             # background maintenance daemon
hyle --serve [PORT]           # HTTP API server (default: 8420)
hyle orchestrate              # project orchestrator web UI
hyle benchmark [--model <id>] # profile LLM on housekeeping tasks
hyle doctor                   # check config, key, network
hyle models --refresh         # refresh models cache
hyle sessions --list          # list saved sessions
hyle sessions --clean         # cleanup old sessions
hyle config set key <value>   # set config value
```

## Slash Commands

Claude Code-style commands executed locally without LLM:

| Command | Description |
|---------|-------------|
| `/build` | Build project (Rust/Node/Python/Go) |
| `/test` | Run project tests |
| `/check` | Type check / lint |
| `/update` | Update dependencies |
| `/clean` | Clean build artifacts |
| `/git [args]` | Run git command |
| `/diff [staged]` | Show git diff |
| `/commit [msg]` | Create commit |
| `/status` | Project status |
| `/ls [path]` | List files |
| `/find [pattern]` | Find files by glob |
| `/grep [pattern]` | Search in files |
| `/view [file]` | Read file contents |
| `/edit [file]` | Open in $EDITOR |
| `/cd [path]` | Change directory |
| `/doctor` | Health check |
| `/model` | Show current model |
| `/cost` | Show token usage |
| `/help` | List all commands |
| `/analyze` | Codebase health analysis |
| `/improve` | Generate improvement prompts |
| `/deps` | Module dependency graph |

## Controls

| Key | Action |
|-----|--------|
| Enter | Send prompt |
| Up/Down | Browse prompt history |
| Ctrl-A | Jump to line start |
| Ctrl-E | Jump to line end |
| Ctrl-K | Kill to end of line |
| Ctrl-U | Kill to start of line |
| Ctrl-P | Prompt palette |
| PageUp/PageDown | Scroll conversation |
| End | Jump to bottom (auto-scroll) |
| Tab | Switch tabs (Chat/Telemetry/Log) |
| k | Kill current operation |
| t | Throttle mode |
| f | Full speed mode |
| n | Normal mode |
| Esc | Zoom out / Quit |

## Config

```
~/.config/hyle/config.json    # API key, preferences (0600)
~/.cache/hyle/models.json     # Cached model list (24h TTL)
~/.local/state/hyle/sessions/ # Session persistence
```

## Features

- **Agentic Loop**: Automatic tool execution and iteration
- **Session Persistence**: Resume conversations across restarts
- **Slash Commands**: 20+ Claude Code-style local commands
- **Free Models**: 35+ free models on OpenRouter
- **Fuzzy Picker**: Incremental search for models
- **SSE Streaming**: Real-time token display
- **Telemetry**: CPU, memory, token, latency traces
- **Auto-throttle**: Backs off under pressure
- **Readline Keys**: Full readline navigation support
- **Intent Tracking**: Multi-granularity goal management
- **Backburner Mode**: LLM-powered maintenance daemon

## Architecture

```
src/
├── main.rs       # CLI parsing, command dispatch
├── config.rs     # XDG paths, secure key storage
├── models.rs     # Model list caching, free filter
├── client.rs     # OpenRouter SSE streaming
├── session.rs    # Conversation persistence
├── ui.rs         # TUI, agentic loop, controls
├── agent.rs      # Tool parsing and execution
├── tools.rs      # File operations, diff generation
├── skills.rs     # Slash commands, skills
├── intent.rs     # Multi-granularity intent tracking
├── cognitive.rs  # Multi-LLM cognitive architecture
├── prompt.rs     # Dynamic context injection
├── project.rs    # Project detection and context
├── backburner.rs # Intelligent maintenance daemon
├── telemetry.rs  # CPU/mem sampling, pressure detection
├── traces.rs     # Token, context, memory, latency traces
├── tmux.rs       # Tmux integration for wide layouts
├── git.rs        # Git operations
├── eval.rs       # Model quality tracking
└── bootstrap.rs  # Self-update and bootstrapping
```

## Cognitive Architecture

hyle uses a multi-LLM architecture for intelligent context management:

- **Executor**: Main model for reasoning and code generation
- **Summarizer**: Free model for context compression
- **Sanity Checker**: Free model for loop detection and validation
- **Docs Watcher**: Free model for documentation maintenance

This allows efficient use of context windows while maintaining coherent long-running sessions.

### Salience-Aware Context

Context is managed across four tiers based on salience:

| Tier | Budget | Content |
|------|--------|---------|
| Focus (40%) | Full detail | Current task, last tool results, errors |
| Recent (30%) | High detail | Last 2-3 exchanges, active decisions |
| Summary (20%) | Compressed | Older exchanges, key facts extracted |
| Background (10%) | Minimal | Project structure, conventions |

Salience scoring considers:
- **Recency**: Recent content scores higher
- **Keywords**: Matches to current task boost score
- **Errors**: Errors and failures are highly salient
- **Decisions**: Confirmed decisions stay visible
- **File focus**: References to current files score higher

## Side Conversations

hyle supports parallel "side conversations" using free models for auxiliary tasks:

```bash
# Main coding session
hyle --free

# In another terminal: docs maintenance
hyle --backburner --watch-docs
```

Side conversations can:
- Watch for code changes and suggest doc updates
- Maintain a changelog from git history
- Keep README synchronized with code structure
- Generate API documentation stubs

## Self-Bootstrapping

hyle can analyze and improve its own codebase:

```bash
# Run self-analysis
hyle /analyze

# Generate improvement suggestions for LLM
hyle /improve

# View module dependency graph
hyle /deps
```

Analysis includes:
- **Health score**: Combined metric from tests, dead code, TODOs
- **Module breakdown**: Lines, functions, tests per module
- **TODO tracking**: High/medium/low priority items
- **Dependency graph**: Mermaid diagram of module relationships

The bootstrap system supports:
- Pre/post-flight test runs before/after changes
- Automatic commit of successful changes
- Issue detection and repair suggestions

## Status & Statistics

**Current Build**: 233 tests passing

| Metric | Value |
|--------|-------|
| Test Coverage | 233 tests |
| Modules | 26 source files |
| Lines of Code | ~15,000 |
| Free Models | 35+ available |
| Slash Commands | 20+ |

### Known Issues

- Multiline input joins lines with spaces (single-line input model)
- Session history navigation blocked during generation
- Context window estimation is approximate

### Safety Features

hyle includes guardrails learned from production incidents:

| Guard | Protection |
|-------|------------|
| `rm -rf` blocker | Blocks destructive bash patterns |
| Rate limit detection | Auto-switches to fallback models |
| Session auto-save | Preserves work on Ctrl-C |
| Tool timeout | 60s default, configurable |

## Failure Scenarios

### The rm -rf Disaster (2025-01-04)

A free model (gemma-2-9b-it) given tool access interpreted "take over development"
as "delete everything and start fresh", executing:

```bash
rm -rf /home/user/project
```

**Lesson learned**: Free models with tool access need guardrails.

**Fix applied**: `BLOCKED_PATTERNS` in tools.rs now blocks:
- `rm -rf`, `rm -r`, `rm --recursive`
- Fork bombs, disk overwrites
- Chained destructive commands
- Remote code execution patterns

### Rate Limit Cascade

Free models have aggressive rate limits. hyle auto-rotates through fallback models:
1. Detect rate limit response
2. Mark model as limited
3. Switch to next available model
4. Retry with exponential backoff

### Context Overflow

Large codebases can exceed context windows. hyle uses salience-aware pruning:
- Focus tier (40%): Current task
- Recent tier (30%): Last exchanges
- Summary tier (20%): Compressed history
- Background tier (10%): Project structure

## Tests

```bash
cargo test
```

## Web Interface

The orchestrator mode provides a web UI for submitting project sketches:

```bash
hyle orchestrate --port 8421 --domain hyperstitious.org
```

Open `http://localhost:8421` and paste a multi-thousand-line project sketch.
hyle will:

1. Parse the sketch and detect project type
2. Create directory structure and git repo
3. Scaffold appropriate boilerplate
4. Generate nginx/systemd deployment configs
5. Dispatch an autonomous hyle instance to build it out

## Roadmap

### Near-term
- [ ] Full async TUI with prompt queuing
- [ ] Better scrollback with search
- [ ] MCP server support
- [ ] Local model support (Ollama)

### Medium-term
- [ ] Visual diff preview before apply
- [ ] Project templates and scaffolding
- [ ] Team collaboration (shared sessions)
- [ ] Plugin/extension system

### Aspirational
- [ ] Self-bootstrapping: hyle develops hyle
- [ ] Multi-agent orchestration
- [ ] Continuous integration with git hooks
- [ ] Voice interface

## Claude Code Interoperability

hyle can import context from Claude Code sessions:

```bash
# In a project with recent Claude Code activity
hyle --handoff

# hyle will detect ~/.claude/ sessions and offer to import
```

The Sessions view (Tab to navigate) shows:
- Active hyle sessions
- Cold hyle sessions (can revive)
- Foreign sessions (Claude Code, Aider, etc.)

Press Enter on a session to restore or import its context.

## License

MIT

---

<p align="center">
  <a href="https://hyle.lol">hyle.lol</a> ·
  <a href="https://hyperstitious.org">hyperstitious.org</a>
</p>
