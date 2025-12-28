# Hyle Documentation

A Rust-native code assistant following Unix philosophy.

---

## Code Reviews

Multi-perspective analysis of the hyle codebase:

| Review | Focus | Findings |
|--------|-------|----------|
| [Security Audit](reviews/security-audit.md) | Vulnerabilities, injection, DoS | 5 critical issues |
| [Unix Philosophy](reviews/unix-philosophy.md) | Composability, simplicity | 6 architecture issues |
| [Distributed Systems](reviews/distributed-systems.md) | Concurrency, race conditions | 5 failure modes |
| [UX Analysis](reviews/ux-analysis.md) | Discoverability, feedback | 6 usability issues |

---

## Architecture

### Module Structure

```
src/
├── main.rs        Entry point, CLI parsing
├── agent.rs       LLM agent loop, tool orchestration
├── client.rs      OpenRouter HTTP client
├── config.rs      XDG config, permissions
├── environ.rs     Environment awareness
├── github.rs      GitHub CLI wrapper
├── models.rs      Model definitions, fallbacks
├── prompts.rs     System prompts library
├── server.rs      HTTP server mode
├── session.rs     Session persistence
├── skills.rs      Slash commands, tool registry
├── tmux.rs        Tmux integration
├── tools.rs       Tool executor
└── ui.rs          TUI interface
```

### Key Concepts

- **Sessions:** Persisted conversations with message history
- **Tools:** File I/O, shell, git operations available to LLM
- **Skills:** Slash commands for user shortcuts
- **Models:** OpenRouter-based with automatic fallback

---

## Quick Start

```bash
# Build
cargo build --release

# Run interactive mode
./target/release/hyle

# Run with prompt
./target/release/hyle "explain this code"

# Server mode
./target/release/hyle server --port 8420

# Slash commands
/help           # Full command list
/map            # Environment awareness
/build          # Build project
/test           # Run tests
```

---

## Design Principles

Following [suckless philosophy](https://suckless.org/philosophy/):

> *"Designing simple and elegant software is far more difficult than letting ad-hoc or over-ambitious features obscure the code over time."*

- **Minimal:** Do one thing well
- **Composable:** Output becomes input
- **Text-based:** Universal interface
- **Testable:** Every function provable
