//! Web intake interface for project orchestration
//!
//! Design posture: functional, honest, no decoration.
//! User task: paste sketch → understand status → leave.

/// Project intake HTML - follows meta-stylebook principles
pub const INTAKE_HTML: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>hyle orchestrator</title>
    <meta name="description" content="Submit project sketches for autonomous building">
    <style>
        /*
         * DESIGN SYSTEM (inherited from landing page)
         * Semantic colors, 8px spacing, fluid typography
         */
        :root {
            --bg: #111;
            --fg: #e8e8e8;
            --muted: #888;
            --accent: #6b9fff;
            --accent-hover: #8bb4ff;
            --border: #333;
            --code-bg: #1a1a1a;
            --success: #6bcc6b;
            --error: #cc6b6b;
            --warning: #ccaa6b;

            --sp-1: 0.5rem;
            --sp-2: 1rem;
            --sp-3: 1.5rem;
            --sp-4: 2rem;

            --text-sm: clamp(0.8125rem, 0.75rem + 0.2vw, 0.875rem);
            --text-base: clamp(0.9375rem, 0.85rem + 0.3vw, 1rem);
        }

        *, *::before, *::after { box-sizing: border-box; margin: 0; padding: 0; }

        html {
            -webkit-font-smoothing: antialiased;
            height: 100%;
        }

        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui, sans-serif;
            font-size: var(--text-base);
            line-height: 1.5;
            color: var(--fg);
            background: var(--bg);
            min-height: 100%;
            display: flex;
            flex-direction: column;
        }

        /* LAYOUT: two-column on wide screens, stack on narrow */
        .container {
            display: grid;
            grid-template-columns: 1fr 300px;
            gap: var(--sp-3);
            max-width: 1200px;
            margin: 0 auto;
            padding: var(--sp-3);
            flex: 1;
            min-height: 0;
        }

        @media (max-width: 800px) {
            .container {
                grid-template-columns: 1fr;
            }
            .sidebar { order: -1; }
        }

        /* HEADER: minimal, identifies context */
        header {
            padding: var(--sp-2) var(--sp-3);
            border-bottom: 1px solid var(--border);
            display: flex;
            justify-content: space-between;
            align-items: center;
        }

        header h1 {
            font-size: var(--text-base);
            font-weight: 500;
        }

        header h1 span {
            color: var(--muted);
            font-weight: 400;
        }

        header a {
            color: var(--muted);
            text-decoration: none;
            font-size: var(--text-sm);
        }

        header a:hover { color: var(--accent); }

        /* MAIN PANEL: sketch input */
        .main-panel {
            display: flex;
            flex-direction: column;
            min-height: 0;
        }

        .input-area {
            flex: 1;
            display: flex;
            flex-direction: column;
            background: var(--code-bg);
            border: 1px solid var(--border);
            border-radius: 4px;
            overflow: hidden;
        }

        .input-header {
            padding: var(--sp-1) var(--sp-2);
            border-bottom: 1px solid var(--border);
            display: flex;
            justify-content: space-between;
            align-items: center;
            font-size: var(--text-sm);
            color: var(--muted);
        }

        textarea {
            flex: 1;
            background: transparent;
            color: var(--fg);
            border: none;
            padding: var(--sp-2);
            font-family: 'SF Mono', 'Fira Code', 'Consolas', monospace;
            font-size: var(--text-sm);
            line-height: 1.5;
            resize: none;
            min-height: 400px;
        }

        textarea:focus { outline: none; }

        textarea::placeholder { color: var(--muted); }

        .input-footer {
            padding: var(--sp-2);
            border-top: 1px solid var(--border);
            display: flex;
            gap: var(--sp-2);
            align-items: center;
        }

        /* BUTTONS */
        button {
            font-family: inherit;
            font-size: var(--text-sm);
            padding: var(--sp-1) var(--sp-2);
            border-radius: 4px;
            cursor: pointer;
            transition: background-color 0.1s, border-color 0.1s;
        }

        button:focus-visible {
            outline: 2px solid var(--accent);
            outline-offset: 2px;
        }

        .btn-primary {
            background: var(--accent);
            color: var(--bg);
            border: none;
            font-weight: 500;
        }

        .btn-primary:hover:not(:disabled) { background: var(--accent-hover); }
        .btn-primary:disabled { opacity: 0.5; cursor: not-allowed; }

        .btn-secondary {
            background: transparent;
            color: var(--muted);
            border: 1px solid var(--border);
        }

        .btn-secondary:hover { border-color: var(--muted); color: var(--fg); }

        /* SIDEBAR: project status */
        .sidebar {
            display: flex;
            flex-direction: column;
            gap: var(--sp-2);
        }

        .sidebar h2 {
            font-size: var(--text-sm);
            font-weight: 500;
            color: var(--muted);
            text-transform: uppercase;
            letter-spacing: 0.05em;
        }

        .project-list {
            background: var(--code-bg);
            border: 1px solid var(--border);
            border-radius: 4px;
            flex: 1;
            overflow-y: auto;
            min-height: 200px;
        }

        .project-item {
            padding: var(--sp-2);
            border-bottom: 1px solid var(--border);
            cursor: pointer;
        }

        .project-item:last-child { border-bottom: none; }
        .project-item:hover { background: rgba(255,255,255,0.02); }

        .project-name {
            font-weight: 500;
            margin-bottom: 0.25rem;
        }

        .project-meta {
            font-size: var(--text-sm);
            color: var(--muted);
            display: flex;
            gap: var(--sp-2);
            align-items: center;
        }

        /* STATUS INDICATORS - semantic colors */
        .status {
            display: inline-block;
            padding: 0.125rem 0.5rem;
            border-radius: 3px;
            font-size: 0.75rem;
            text-transform: uppercase;
            letter-spacing: 0.03em;
        }

        .status-pending { background: var(--border); color: var(--fg); }
        .status-scaffolding { background: var(--warning); color: var(--bg); }
        .status-building { background: var(--accent); color: var(--bg); }
        .status-running { background: var(--success); color: var(--bg); }
        .status-failed { background: var(--error); color: var(--bg); }
        .status-completed { background: var(--success); color: var(--bg); }

        .empty-state {
            padding: var(--sp-4);
            text-align: center;
            color: var(--muted);
            font-size: var(--text-sm);
        }

        /* MODAL - for project details */
        .modal {
            display: none;
            position: fixed;
            inset: 0;
            background: rgba(0,0,0,0.8);
            align-items: center;
            justify-content: center;
            padding: var(--sp-3);
        }

        .modal.active { display: flex; }

        .modal-content {
            background: var(--bg);
            border: 1px solid var(--border);
            border-radius: 4px;
            width: 100%;
            max-width: 600px;
            max-height: 80vh;
            overflow: auto;
        }

        .modal-header {
            padding: var(--sp-2);
            border-bottom: 1px solid var(--border);
            display: flex;
            justify-content: space-between;
            align-items: center;
        }

        .modal-header h3 { font-weight: 500; }

        .modal-close {
            background: none;
            border: none;
            color: var(--muted);
            font-size: 1.25rem;
            cursor: pointer;
            padding: 0;
            line-height: 1;
        }

        .modal-close:hover { color: var(--fg); }

        .modal-body {
            padding: var(--sp-2);
        }

        .log-entry {
            font-family: 'SF Mono', monospace;
            font-size: var(--text-sm);
            padding: 0.25rem 0;
            border-bottom: 1px solid var(--border);
        }

        .log-entry:last-child { border-bottom: none; }

        .log-time { color: var(--muted); margin-right: var(--sp-1); }
        .log-kind { color: var(--accent); }

        /* REDUCED MOTION */
        @media (prefers-reduced-motion: reduce) {
            * { transition: none !important; }
        }
    </style>
</head>
<body>
    <header>
        <h1>hyle <span>orchestrator</span></h1>
        <a href="/">← back to hyle.lol</a>
    </header>

    <div class="container">
        <div class="main-panel">
            <div class="input-area">
                <div class="input-header">
                    <span>SKETCH.md</span>
                    <span id="charCount">0 chars</span>
                </div>
                <textarea
                    id="sketch"
                    placeholder="# Project Name

Describe what you want to build.

## Features
- Feature one
- Feature two

## Code

```rust
fn main() {
    // Starting point
}
```

subdomain = your-app
port = 3000"
                    spellcheck="false"
                    aria-label="Project sketch input"
                ></textarea>
                <div class="input-footer">
                    <button class="btn-primary" id="submitBtn" onclick="submitProject()" disabled>
                        Launch
                    </button>
                    <button class="btn-secondary" onclick="clearSketch()">Clear</button>
                    <span style="flex:1"></span>
                    <span style="color: var(--muted); font-size: var(--text-sm);">
                        min 50 chars
                    </span>
                </div>
            </div>
        </div>

        <div class="sidebar">
            <h2>Projects</h2>
            <div class="project-list" id="projectList">
                <div class="empty-state">No projects yet</div>
            </div>
        </div>
    </div>

    <div class="modal" id="projectModal">
        <div class="modal-content">
            <div class="modal-header">
                <h3 id="modalTitle">Project</h3>
                <button class="modal-close" onclick="closeModal()" aria-label="Close">×</button>
            </div>
            <div class="modal-body" id="modalBody"></div>
        </div>
    </div>

    <script>
        const textarea = document.getElementById('sketch');
        const charCount = document.getElementById('charCount');
        const submitBtn = document.getElementById('submitBtn');

        // Update character count and button state
        textarea.addEventListener('input', () => {
            const len = textarea.value.length;
            charCount.textContent = len.toLocaleString() + ' chars';
            submitBtn.disabled = len < 50;
        });

        async function submitProject() {
            const sketch = textarea.value;
            if (sketch.length < 50) return;

            submitBtn.disabled = true;
            submitBtn.textContent = 'Launching…';

            try {
                const res = await fetch('/api/projects', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ sketch })
                });
                const data = await res.json();

                if (data.success) {
                    textarea.value = '';
                    charCount.textContent = '0 chars';
                    loadProjects();
                } else {
                    alert('Error: ' + (data.error || 'Unknown error'));
                }
            } catch (err) {
                alert('Error: ' + err.message);
            } finally {
                submitBtn.disabled = textarea.value.length < 50;
                submitBtn.textContent = 'Launch';
            }
        }

        function clearSketch() {
            textarea.value = '';
            charCount.textContent = '0 chars';
            submitBtn.disabled = true;
        }

        async function loadProjects() {
            try {
                const res = await fetch('/api/projects');
                const data = await res.json();
                const list = document.getElementById('projectList');

                if (!data.projects || data.projects.length === 0) {
                    list.innerHTML = '<div class="empty-state">No projects yet</div>';
                    return;
                }

                list.innerHTML = data.projects.map(p => `
                    <div class="project-item" onclick="showProject('${p.id}')">
                        <div class="project-name">${escapeHtml(p.spec.name)}</div>
                        <div class="project-meta">
                            <span class="status status-${p.status}">${p.status}</span>
                            <span>${p.spec.project_type}</span>
                        </div>
                    </div>
                `).join('');
            } catch (err) {
                console.error('Failed to load projects:', err);
            }
        }

        async function showProject(id) {
            try {
                const res = await fetch('/api/projects/' + id);
                const p = await res.json();

                document.getElementById('modalTitle').textContent = p.spec.name;
                document.getElementById('modalBody').innerHTML = `
                    <p><strong>Status:</strong> <span class="status status-${p.status}">${p.status}</span></p>
                    <p><strong>Type:</strong> ${p.spec.project_type}</p>
                    <p><strong>Dir:</strong> <code>${escapeHtml(p.project_dir)}</code></p>
                    ${p.url ? `<p><strong>URL:</strong> <a href="${p.url}">${p.url}</a></p>` : ''}
                    <h4 style="margin-top: 1rem; color: var(--muted);">Log</h4>
                    ${p.log.map(e => `
                        <div class="log-entry">
                            <span class="log-time">${new Date(e.timestamp).toLocaleTimeString()}</span>
                            <span class="log-kind">[${e.kind}]</span>
                            ${escapeHtml(e.message)}
                        </div>
                    `).join('')}
                `;
                document.getElementById('projectModal').classList.add('active');
            } catch (err) {
                alert('Failed to load project');
            }
        }

        function closeModal() {
            document.getElementById('projectModal').classList.remove('active');
        }

        function escapeHtml(str) {
            const div = document.createElement('div');
            div.textContent = str;
            return div.innerHTML;
        }

        // Close modal on escape
        document.addEventListener('keydown', e => {
            if (e.key === 'Escape') closeModal();
        });

        // Poll for updates every 5s
        setInterval(loadProjects, 5000);
        loadProjects();
    </script>
</body>
</html>
"##;
