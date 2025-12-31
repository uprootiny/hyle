# hyle Development Journal

## 2025-12-31: hyle.lol Reactive Cascade UI

### Problem
The hyle.lol landing page had several UX issues:
- Submit button did nothing with short prompts (silent failure)
- No feedback during long builds (some models take 2+ minutes)
- Jobs could get "lost" with no indication
- URLs weren't clickable
- Completed projects didn't appear in gallery
- Basic spinner instead of the fancy cascade visualization from CLI

### Solution
Complete overhaul of the frontend with reactive cascade display mirroring `src/cascade.rs`.

#### Braille Spinners
Replaced CSS rotation with the same braille animation from the CLI:
```javascript
const SPINNERS = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
// Cycles at 80ms intervals for smooth animation
```

#### Log Prefixes
Matching the Rust `LogLevel` enum:
```javascript
const LOG_PREFIX = {
    info: '·',    // General info
    model: '⚡',   // Model selection
    token: '↳',   // Token activity
    check: '✓',   // Success
    warn: '⚠',    // Warning
    error: '✗'    // Error
};
```

#### Robust Polling
```javascript
const POLL_TIMEOUT_MS = 120000;  // 2 minute max
const maxErrors = 5;             // Consecutive errors before abort

// Heartbeat every 10s during generation
if (pollCount % 5 === 0 && lastStatus === 'building') {
    log(`still generating... (${elapsed}s)`, 'info');
}

// Handle job expiration
if (data.status === 'not_found') {
    errorCount++;
    if (errorCount >= maxErrors) {
        log('job lost: server may have restarted', 'error');
        endBuild();
    }
}
```

#### Dynamic Gallery
Completed projects now appear at the top of the gallery:
```javascript
function addProjectToGallery({ url, name, desc }) {
    const card = document.createElement('article');
    card.className = 'project-card user-created';
    // ... create card HTML
    projectGrid.insertBefore(card, projectGrid.firstChild);
    // Animate in with scale effect
    saveRecentProjects(); // Persist to localStorage
}
```

#### Clickable URLs
```javascript
function linkify(text) {
    return text.replace(
        /(https?:\/\/[^\s]+)/g,
        '<a href="$1" target="_blank">$1</a>'
    );
}
```

### Visual Polish
- Refined color palette with `--accent-dim`, `--blue-dim` for glows
- Stage unfold animation when new stages appear
- Smooth progress bar interpolation using `requestAnimationFrame`
- Shake animation on validation failure
- Loading spinner on submit button
- Breathing animation on idle status indicator

### UX Improvements
- Minimum 10 characters (down from 20) with clear feedback
- Keyboard shortcut: `Cmd/Ctrl+Enter` to submit
- Project cards clickable to open in new tab
- localStorage persistence for recent projects (max 10)

### Model Performance Observed
From testing, model response times varied significantly:
- `mistral-small-3.1-24b-instruct`: ~10s (fastest)
- `deepseek-r1-0528`: ~60s
- `gemma-3-27b-it`: ~156s (slowest)

The 2-minute timeout accommodates slow models while preventing infinite hangs.

### Git Commits
- `24d6e27`: ui: reactive cascade status display on landing page
- `866dff8`: ui: polish and tune reactive landing page
- `b3d794e`: fix: add validation feedback when sketch too short
- `18ad4be`: fix: improve cascade feedback and error handling
- `1cc302f`: ui: braille spinners, clickable URLs, cascade polish
- `a5bce60`: feat: dynamic gallery with completed projects

---

## 2025-12-31: LLM Failure Taxonomy and Tiered Recovery

### Problem
When dispatching tasks to LLMs in agentic loops, failures were handled ad-hoc:
- Rate limits (429) just triggered immediate retry
- No structured classification of error types
- No exponential backoff
- No circuit breaker pattern
- Model fallback was primitive

### Solution
Created `src/failures.rs` with comprehensive failure taxonomy and recovery strategies.

#### Failure Categories
```
Transient       → Network blip, 503, timeout     → Immediate retry (3x)
RateLimit       → 429, quota exhausted           → Exponential backoff
ModelSpecific   → Overload, unavailable          → Fallback to different model
ContentRelated  → Context too long, policy       → Truncate/rephrase
Fatal           → Auth failed, API key invalid   → Circuit break, abort
```

#### Recovery Strategy Tiers
1. **Tier 1 (Transient)**: Immediate retry up to 3 times
2. **Tier 2 (Rate Limit)**: Exponential backoff with jitter (1s→2s→4s→...→60s max)
3. **Tier 3 (Model)**: Fallback to next model in rotation
4. **Tier 4 (Content)**: Adjust prompt (truncate, summarize, rephrase)
5. **Tier 5 (Fatal)**: Circuit break, stop retrying

#### Key Types
```rust
enum LlmFailure {
    NetworkError(String), Timeout, ServiceUnavailable,
    RateLimited { retry_after_ms: Option<u64> }, QuotaExhausted,
    ModelOverloaded(String), ContextTooLong { limit, actual },
    ContentPolicyViolation(String), MalformedResponse, ParseError,
    AuthenticationFailed, InvalidApiKey, AccountSuspended, ...
}

enum RecoveryOutcome {
    Retry { delay, attempt, max_attempts },
    Fallback { model, reason },
    Adjust { action: TruncateContext | SummarizeContext | ... },
    Abort { reason, is_permanent },
}
```

### Bug Fixes

#### 1. dispatch_hyle Missing --task Flag
`src/orchestrator.rs:700` was passing prompt as positional arg instead of using `--task`:
```rust
// Before (broken)
.arg(prompt)
.arg("--trust")

// After (fixed)
.arg("--task")
.arg(prompt)
.arg("--trust")
```

#### 2. Job Cleanup Memory Leak
Added background cleanup task to `src/api/main.rs` that removes completed/failed jobs older than 1 hour. Prevents unbounded HashMap growth.

#### 3. parse_test_output False Positive
`src/backburner.rs:98` was matching "test result: FAILED..." as a failed test name because it starts with "test " and contains "FAILED". Fixed by excluding lines starting with "test result:".

### Files Changed
- `src/failures.rs` (NEW): 700+ lines, 24 tests
- `src/main.rs`: Added `mod failures;`
- `src/api/main.rs`: Job cleanup task, path validation
- `src/orchestrator.rs`: Fixed --task flag order
- `src/backburner.rs`: Fixed parse_test_output bug
- `OBLIGATIONS.md` (NEW): Technical debt ledger

### Test Results
All 319 tests pass:
- 24 new failure taxonomy tests
- 19 backburner tests (including fixed parse_test_output)
- 64 cascade tests
- 7 hyle-api tests
- 12 user story tests

---

## 2025-12-30: Internet Artpiece Philosophy

### Insight
Hyle creates "internet artpieces" — not static webpages. The user's description:
> "something people can open up in their browser and interact with, something responsive, smooth, dynamic"

### Changes
Updated the system prompt in `src/api/main.rs` to reframe what hyle produces:

```
You are creating an INTERNET ARTPIECE — a self-contained, interactive browser experience.

This is NOT a static webpage. This is something people open in their browser and INTERACT with.
Think: generative art, data visualizations, audio toys, interactive fiction, creative tools.

Requirements:
- Single index.html file (all CSS/JS inline)
- Responsive: works on any screen size
- Smooth: 60fps animations, no jank
- Dynamic: responds to user input
- Self-contained: no external dependencies
- Delightful: surprising, playful, aesthetically considered

Make it something people want to share. Make it memorable.
```

Also added a new gallery example: datacenter infrastructure visualization with dymaxion projection.

### Git Commits
- `2d99bba`: prompt: reframe hyle as internet artpiece generator
- `5382123`: gallery: add datacenter infrastructure visualization example

---

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
