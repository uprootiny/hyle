# hyle Development Journal

## 2025-12-29 (Part 3): Frontend Progress Visualization

### Problem
The hyle.lol form submission UI showed minimal progress during builds. Users complained about:
- No visibility into which models were being tried
- No feedback during rate limiting (429 fallbacks)
- Lack of animated progress indicators
- No elapsed time tracking

### Solution
Replaced the simple step text with a comprehensive progress visualization:

1. **Process Graph**: Visual pipeline with 5 nodes (submit→queue→build→deploy→live)
   - Nodes pulse when active, turn green when done
   - Edges animate with flowing gradient during transitions

2. **Model Tracker**: Scrollable log showing LLM model attempts
   - Shows model name with status icon (◎ trying, ✓ success, ✗ failed)
   - Rate-limited models marked with "(429)"
   - Auto-scrolls to latest entry

3. **Stats Bar**: Real-time metrics
   - Elapsed time (auto-updating every second)
   - Models tried count (with animation on increment)
   - Poll count

### CSS Animations
- `node-pulse`: Pulsing box-shadow on active nodes
- `edge-flow`: Gradient flowing along edges
- `entry-appear`: Slide-in animation for log entries
- `counter-tick`: Scale+color flash on counter updates

### Files Changed
- `index.html`: Added progress-viz HTML structure, CSS animations, JavaScript state management

### Git Commit
- `99494fc`: ux: add animated progress visualization to hyle.lol

---

## 2025-12-29 (Part 2): --watch-docs Implementation

### Problem
README claimed `hyle --backburner --watch-docs` existed, but it wasn't implemented.
The `--watch-docs` flag was being parsed as a path argument.

### Solution
Implemented proper `--watch-docs` mode in backburner:

1. Added `watch_docs: bool` field to `Command::Backburner`
2. Added `--watch-docs` flag parsing in `parse_args()`
3. Created `run_docs_mode()` in backburner.rs with:
   - `scan_codebase()` - finds documentation files
   - `analyze_readme()` - analyzes README structure with LLM
   - `generate_docs()` - creates README if missing
   - `check_doc_staleness()` - compares file timestamps

### Usage
```bash
hyle --backburner --watch-docs
```

Shows:
```
HYLE DOCS WATCHER - Documentation Maintenance Daemon
[timestamp] Scanning codebase for documentation needs...
  Found 43 relevant files
  [x] README.md exists
  [x] Cargo.toml found (Rust project)
```

---

## 2025-12-29: API Server Deployment and Bug Fixes

### Summary
Deployed hyle-api to hyperstitious.org, fixed multiple issues with form submission and headless mode.

### Work Completed

#### 1. hyle-api Server (src/api/main.rs)
- Created HTTP API server using axum for sketch submission
- Endpoints: `/health`, `/api/models`, `/api/sketch`, `/api/jobs/{id}`
- Multi-model fallback with round-robin: tries 8 free OpenRouter models
- On rate limit (429), automatically falls back to next model

#### 2. Model List (verified 2025-12-29)
```
google/gemini-2.0-flash-exp:free   # 1M context
qwen/qwen3-coder:free              # 262K context - coding optimized
mistralai/devstral-2512:free       # 262K context - dev focused
kwaipilot/kat-coder-pro:free       # 256K context - coding specific
meta-llama/llama-3.3-70b-instruct:free # 131K context
google/gemma-3-27b-it:free         # 131K context
deepseek/deepseek-r1-0528:free     # 164K context
mistralai/mistral-small-3.1-24b-instruct:free # 128K context
```

**Removed**: `google/gemma-2-9b-it:free` (404 - doesn't exist)

#### 3. Bug Fixes

**Poll URL Issue**
- Problem: `poll_url` returned as relative path (`/api/jobs/...`)
- When fetched from hyle.lol (GitHub Pages), resolved to wrong origin
- Fix: JavaScript now constructs full URL with `HYLE_BASE` prefix

**HYLE_MODEL Environment Variable**
- Problem: `run_task()` ignored HYLE_MODEL env var, only used config
- Fix: Now checks env var first, then config, then default

**TTY Requirement**
- Problem: hyle requires TTY for TUI mode, systemd service has no TTY
- Error: "No such device or address (os error 6)"
- Fix: Use `--task` mode for headless operation

**File Writing**
- Problem: Small models don't reliably use write() tool
- Fix: Added explicit prompt wrapper instructing agent to write files

#### 4. Deployment

**Static Binaries**
```bash
cargo build --release --target x86_64-unknown-linux-musl --bin hyle --bin hyle-api
```
Both binaries are statically linked (no nix dependencies).

**Systemd Service**
- Location: `/etc/systemd/system/hyle-api.service`
- Runs as: uprootiny (not www-data)
- Reads env from: `/etc/hyle/env`

**nginx Proxy**
- Domain: hyle.hyperstitious.org
- TLS via certbot/Let's Encrypt
- Proxies to localhost:3000

### Current Architecture

```
hyle.lol (GitHub Pages)
    |
    | POST /api/sketch
    v
hyle.hyperstitious.org (nginx)
    |
    | proxy_pass
    v
hyle-api (localhost:3000)
    |
    | spawns with --task mode
    v
hyle binary
    |
    | calls OpenRouter API
    v
LLM (round-robin models)
    |
    | writes files
    v
/var/www/drops/{project}/index.html
```

### Known Issues

1. **Subdomain Routing**: New projects need manual nginx config
   - `/var/www/drops/{project}` exists but `{project}.hyperstitious.org` not auto-routed
   - Consider: wildcard subdomain + dynamic nginx config

2. **Job Persistence**: Jobs stored in memory, lost on restart
   - Consider: SQLite/file-based storage for job tracking

3. **Progress Streaming**: No real-time progress from hyle to API
   - Current: just polls for completion
   - Consider: WebSocket or SSE for live updates

### Files Changed

- `src/api/main.rs`: Model list, --task mode, prompt wrapper
- `src/main.rs`: HYLE_MODEL env var support in run_task()
- `index.html`: Absolute poll URL fix
- `deploy/hyle-api.service`: User/group settings
- `/etc/hyle/env`: Model list, API key

### Git Commits

- `e9568fa`: fix: improve hyle-api headless mode
- `223780c`: fix: use absolute URL for job polling from hyle.lol
