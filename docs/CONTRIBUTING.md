# Contributing to hyle

## Current State

hyle is early-stage. Most features shown on landing pages are unimplemented. This is a good time to contribute - there's lots of low-hanging fruit.

## Priority Areas

In order of impact:

### 1. Core CLI (High Impact)

**Slash commands** - Bypass LLM for common operations
```rust
// src/commands.rs
/build  -> cargo build / npm run build / make
/test   -> cargo test / npm test / pytest
/diff   -> git diff
/status -> git status + project info
/help   -> show available commands
```

**--pipe mode** - Unix composition
```rust
// When stdout is not a tty, or --pipe flag:
// - No TUI
// - No ANSI codes
// - Clean text output only
// - Read prompts from stdin
```

### 2. Session System (High Impact)

**Session persistence**
```
~/.local/state/hyle/sessions/{id}/
  session.json    # metadata
  messages.jsonl  # conversation history
  audit.jsonl     # operation log
```

**Auto-resume** - `hyle` with no args resumes last session for cwd

### 3. Safety (Medium Impact)

**Command filters** - Block obviously dangerous commands
**Backup before write** - Create .bak before modifying files
**Audit logging** - Log all tool operations to audit.jsonl

### 4. Local Models (Medium Impact)

**Ollama integration** - `hyle --model llama3`
**Cost tracking** - Show tokens used, estimated cost per session

## Development Setup

```bash
git clone https://github.com/uprootiny/hyle
cd hyle
cargo build
cargo test
```

## Code Style

- `cargo fmt` before committing
- `cargo clippy -- -D warnings` must pass
- Tests for new features
- No unsafe without justification

## Commit Messages

```
<type>: <description>

[optional body]

Co-Authored-By: <your-name> <your-email>
```

Types: `feat`, `fix`, `docs`, `test`, `refactor`, `chore`

## Pull Requests

1. Fork the repo
2. Create a branch: `git checkout -b feat/slash-commands`
3. Make changes
4. Run tests: `cargo test`
5. Push: `git push origin feat/slash-commands`
6. Open PR against `master`

## Good First Issues

Check [GitHub Issues](https://github.com/uprootiny/hyle/issues) for `good first issue` label.

If none exist, these are always welcome:
- Add a test for existing functionality
- Improve error messages
- Add `--version` or `--help` output
- Documentation improvements

## Questions?

Open an issue or start a discussion.
