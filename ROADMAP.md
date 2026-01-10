# hyle Roadmap

## Status Key

- [x] Done
- [~] Partial
- [ ] Not started

---

## v0.1 - Foundation

**Goal:** Honest, working CLI

- [~] Basic REPL with OpenRouter
- [ ] Slash commands: /build /test /diff /status /help
- [ ] --pipe mode (no TUI, clean output)
- [ ] --json output mode
- [ ] Detect non-tty and auto-switch to pipe mode
- [ ] README.md with honest feature list
- [ ] 20+ tests

---

## v0.2 - Sessions

**Goal:** Pick up where you left off

- [ ] Session save on exit
- [ ] Session resume on start (per-directory)
- [ ] Session list: `hyle --sessions`
- [ ] Session switch: `hyle --session <id>`
- [ ] Backup before write (.bak files)
- [ ] 40+ tests

---

## v0.3 - Safety & Local

**Goal:** Trust and cost control

- [ ] Command safety filters (block rm -rf /, etc)
- [ ] Audit logging (audit.jsonl)
- [ ] Ollama provider (local models)
- [ ] Cost tracking per session
- [ ] Budget limits: `hyle --max-cost 0.50`
- [ ] 60+ tests

---

## v0.4 - Power User

**Goal:** Flow state

- [ ] Prompt queuing (type while generating)
- [ ] Keyboard shortcuts in TUI
- [ ] /grep /ls /view commands
- [ ] /cost command (show session cost)
- [ ] /model command (switch models)
- [ ] Config file (~/.config/hyle/config.toml)
- [ ] 80+ tests

---

## v0.5 - Polish

**Goal:** Ready for daily use

- [ ] Man page
- [ ] Shell completions (bash, zsh, fish)
- [ ] --verbose and --quiet flags
- [ ] Error messages that help
- [ ] TUI themes
- [ ] 100+ tests

---

## v0.6 - Advanced TUI

**Goal:** Novel interaction patterns

- [ ] Pinned prompts (click to pin, float at top)
- [ ] Relevance decay indicator per pinned prompt
- [ ] Retrigger widget (re-run pinned prompt)
- [ ] Clickable elements in terminal output
- [ ] Prompt history browser with fuzzy search
- [ ] Split view: code diff + conversation
- [ ] 120+ tests

---

## v1.0 - Release

**Goal:** All landing page promises delivered

- [ ] Reproducible builds
- [ ] Signed releases
- [ ] SBOM
- [ ] Real documentation
- [ ] All 13 persona features implemented
- [ ] 150+ tests

---

## Non-Goals (for now)

- IDE plugins
- Web interface
- Multi-user / collaboration
- Custom model training
- Plugin system

---

## Feature Matrix by Persona

| Feature | Unix | Velocity | Reliable | Secure | Indie | Flow |
|---------|------|----------|----------|--------|-------|------|
| --pipe mode | v0.1 | - | - | - | - | - |
| Slash commands | - | v0.1 | - | - | - | - |
| Backups | - | - | v0.2 | - | - | - |
| Safety filters | - | - | - | v0.3 | - | - |
| Audit log | - | - | - | v0.3 | - | - |
| Ollama | - | - | - | - | v0.3 | - |
| Cost tracking | - | - | - | - | v0.3 | - |
| Sessions | - | - | v0.2 | - | - | v0.2 |
| Prompt queue | - | - | - | - | - | v0.4 |
| Keyboard shortcuts | - | - | - | - | - | v0.4 |

---

## Landing Page Debt

Features shown on hyle.lol that don't exist:

| Page | Claim | Target Version |
|------|-------|----------------|
| all | 364 tests | v1.0 (honest count) |
| depth.html | Formal verification | Never (remove claim) |
| community.html | Active community | Organic (can't force) |
| learn.html | Learning paths | Remove (link to real resources) |
| observable.html | Live dashboard | v0.4 or remove |

---

See [docs/PROMISES.md](./docs/PROMISES.md) for detailed gap analysis.
