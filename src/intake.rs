//! Web intake interface for project orchestration
//!
//! Provides the HTML/CSS/JS for the project submission form and status tracking.

/// Full HTML page for project intake at hyle.hyperstitious.org
pub const INTAKE_HTML: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>hyle - Project Orchestrator</title>
    <style>
        :root {
            --bg: #1a1b26;
            --bg-dark: #16161e;
            --bg-highlight: #292e42;
            --fg: #c0caf5;
            --fg-dark: #a9b1d6;
            --fg-gutter: #3b4261;
            --blue: #7aa2f7;
            --cyan: #7dcfff;
            --green: #9ece6a;
            --magenta: #bb9af7;
            --red: #f7768e;
            --orange: #ff9e64;
            --yellow: #e0af68;
        }

        * { box-sizing: border-box; margin: 0; padding: 0; }

        body {
            font-family: 'JetBrains Mono', 'Fira Code', monospace;
            background: var(--bg);
            color: var(--fg);
            min-height: 100vh;
            display: flex;
            flex-direction: column;
        }

        header {
            background: var(--bg-dark);
            padding: 1rem 2rem;
            border-bottom: 1px solid var(--fg-gutter);
            display: flex;
            justify-content: space-between;
            align-items: center;
        }

        header h1 {
            color: var(--blue);
            font-size: 1.5rem;
            font-weight: normal;
        }

        header h1 span { color: var(--fg-dark); }

        nav a {
            color: var(--fg-dark);
            text-decoration: none;
            margin-left: 1.5rem;
            transition: color 0.2s;
        }

        nav a:hover { color: var(--cyan); }
        nav a.active { color: var(--green); }

        main {
            flex: 1;
            display: flex;
            max-width: 1400px;
            margin: 0 auto;
            width: 100%;
            padding: 2rem;
            gap: 2rem;
        }

        .intake-panel {
            flex: 2;
            display: flex;
            flex-direction: column;
        }

        .status-panel {
            flex: 1;
            background: var(--bg-dark);
            border-radius: 8px;
            padding: 1.5rem;
            border: 1px solid var(--fg-gutter);
        }

        h2 {
            color: var(--fg-dark);
            font-size: 0.9rem;
            text-transform: uppercase;
            letter-spacing: 0.1em;
            margin-bottom: 1rem;
        }

        .sketch-input {
            flex: 1;
            display: flex;
            flex-direction: column;
            background: var(--bg-dark);
            border-radius: 8px;
            border: 1px solid var(--fg-gutter);
            overflow: hidden;
        }

        .sketch-header {
            display: flex;
            justify-content: space-between;
            align-items: center;
            padding: 0.75rem 1rem;
            border-bottom: 1px solid var(--fg-gutter);
            background: var(--bg-highlight);
        }

        .sketch-header .filename {
            color: var(--fg-dark);
            font-size: 0.85rem;
        }

        .sketch-header .lang-badge {
            background: var(--blue);
            color: var(--bg);
            padding: 0.2rem 0.5rem;
            border-radius: 4px;
            font-size: 0.75rem;
        }

        textarea#sketch {
            flex: 1;
            background: var(--bg-dark);
            color: var(--fg);
            border: none;
            padding: 1rem;
            font-family: inherit;
            font-size: 0.9rem;
            line-height: 1.6;
            resize: none;
            min-height: 400px;
        }

        textarea#sketch:focus {
            outline: none;
        }

        textarea#sketch::placeholder {
            color: var(--fg-gutter);
        }

        .actions {
            display: flex;
            gap: 1rem;
            padding: 1rem;
            background: var(--bg-highlight);
            border-top: 1px solid var(--fg-gutter);
        }

        button {
            font-family: inherit;
            font-size: 0.9rem;
            padding: 0.75rem 1.5rem;
            border: none;
            border-radius: 6px;
            cursor: pointer;
            transition: all 0.2s;
        }

        button.primary {
            background: var(--green);
            color: var(--bg);
        }

        button.primary:hover {
            background: #b5e076;
        }

        button.secondary {
            background: var(--bg);
            color: var(--fg-dark);
            border: 1px solid var(--fg-gutter);
        }

        button.secondary:hover {
            border-color: var(--fg-dark);
        }

        button:disabled {
            opacity: 0.5;
            cursor: not-allowed;
        }

        .project-list {
            list-style: none;
        }

        .project-item {
            padding: 1rem;
            border-bottom: 1px solid var(--fg-gutter);
            cursor: pointer;
            transition: background 0.2s;
        }

        .project-item:hover {
            background: var(--bg-highlight);
        }

        .project-item:last-child {
            border-bottom: none;
        }

        .project-name {
            color: var(--cyan);
            font-weight: 500;
            margin-bottom: 0.25rem;
        }

        .project-meta {
            font-size: 0.8rem;
            color: var(--fg-gutter);
            display: flex;
            gap: 1rem;
        }

        .status-badge {
            display: inline-block;
            padding: 0.15rem 0.5rem;
            border-radius: 4px;
            font-size: 0.75rem;
            text-transform: uppercase;
        }

        .status-pending { background: var(--fg-gutter); color: var(--fg); }
        .status-scaffolding { background: var(--yellow); color: var(--bg); }
        .status-building { background: var(--blue); color: var(--bg); }
        .status-testing { background: var(--magenta); color: var(--bg); }
        .status-deploying { background: var(--orange); color: var(--bg); }
        .status-running { background: var(--green); color: var(--bg); }
        .status-failed { background: var(--red); color: var(--bg); }
        .status-completed { background: var(--green); color: var(--bg); }

        .empty-state {
            color: var(--fg-gutter);
            text-align: center;
            padding: 2rem;
            font-size: 0.9rem;
        }

        .modal {
            display: none;
            position: fixed;
            top: 0;
            left: 0;
            right: 0;
            bottom: 0;
            background: rgba(0,0,0,0.8);
            z-index: 100;
            align-items: center;
            justify-content: center;
        }

        .modal.active { display: flex; }

        .modal-content {
            background: var(--bg-dark);
            border: 1px solid var(--fg-gutter);
            border-radius: 8px;
            width: 90%;
            max-width: 800px;
            max-height: 80vh;
            overflow: auto;
        }

        .modal-header {
            display: flex;
            justify-content: space-between;
            align-items: center;
            padding: 1rem 1.5rem;
            border-bottom: 1px solid var(--fg-gutter);
        }

        .modal-header h3 {
            color: var(--cyan);
        }

        .modal-close {
            background: none;
            border: none;
            color: var(--fg-dark);
            font-size: 1.5rem;
            cursor: pointer;
            padding: 0;
        }

        .modal-body {
            padding: 1.5rem;
        }

        .log-entry {
            padding: 0.5rem 0;
            border-bottom: 1px solid var(--bg-highlight);
            font-size: 0.85rem;
        }

        .log-entry:last-child { border-bottom: none; }

        .log-time {
            color: var(--fg-gutter);
            margin-right: 1rem;
        }

        .log-kind {
            color: var(--blue);
            margin-right: 0.5rem;
        }

        .char-count {
            color: var(--fg-gutter);
            font-size: 0.8rem;
        }

        footer {
            padding: 1rem 2rem;
            text-align: center;
            color: var(--fg-gutter);
            font-size: 0.8rem;
            border-top: 1px solid var(--fg-gutter);
        }

        footer a {
            color: var(--blue);
            text-decoration: none;
        }

        @keyframes pulse {
            0%, 100% { opacity: 1; }
            50% { opacity: 0.5; }
        }

        .building .status-badge {
            animation: pulse 1.5s infinite;
        }
    </style>
</head>
<body>
    <header>
        <h1>hyle <span>project orchestrator</span></h1>
        <nav>
            <a href="#" class="active">New Project</a>
            <a href="#projects">Projects</a>
            <a href="/api">API</a>
        </nav>
    </header>

    <main>
        <div class="intake-panel">
            <div class="sketch-input">
                <div class="sketch-header">
                    <span class="filename">SKETCH.md</span>
                    <span class="char-count" id="charCount">0 chars</span>
                </div>
                <textarea id="sketch" placeholder="Paste your project sketch here...

# Project Name

Description of what you want to build.

## Features
- Feature 1
- Feature 2

## Tech Stack
- Rust / Clojure / Node.js

## Code

```rust
fn main() {
    // Your code here
}
```

The more detail you provide, the better hyle can build it."></textarea>
                <div class="actions">
                    <button class="primary" id="submitBtn" onclick="submitProject()">
                        Launch Project
                    </button>
                    <button class="secondary" onclick="clearSketch()">
                        Clear
                    </button>
                    <button class="secondary" onclick="loadExample()">
                        Load Example
                    </button>
                </div>
            </div>
        </div>

        <div class="status-panel">
            <h2>Recent Projects</h2>
            <ul class="project-list" id="projectList">
                <li class="empty-state">No projects yet</li>
            </ul>
        </div>
    </main>

    <div class="modal" id="projectModal">
        <div class="modal-content">
            <div class="modal-header">
                <h3 id="modalTitle">Project Details</h3>
                <button class="modal-close" onclick="closeModal()">&times;</button>
            </div>
            <div class="modal-body" id="modalBody">
            </div>
        </div>
    </div>

    <footer>
        Powered by <a href="https://github.com/anthropics/claude-code">hyle</a> |
        <a href="https://hyperstitious.org">hyperstitious.org</a>
    </footer>

    <script>
        const textarea = document.getElementById('sketch');
        const charCount = document.getElementById('charCount');
        const submitBtn = document.getElementById('submitBtn');

        textarea.addEventListener('input', () => {
            const len = textarea.value.length;
            charCount.textContent = len.toLocaleString() + ' chars';
            submitBtn.disabled = len < 50;
        });

        async function submitProject() {
            const sketch = textarea.value;
            if (sketch.length < 50) {
                alert('Please provide a more detailed sketch (at least 50 characters)');
                return;
            }

            submitBtn.disabled = true;
            submitBtn.textContent = 'Launching...';

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
                    alert('Project launched! ID: ' + data.project_id);
                } else {
                    alert('Error: ' + (data.error || 'Unknown error'));
                }
            } catch (err) {
                alert('Error: ' + err.message);
            } finally {
                submitBtn.disabled = false;
                submitBtn.textContent = 'Launch Project';
            }
        }

        function clearSketch() {
            textarea.value = '';
            charCount.textContent = '0 chars';
        }

        function loadExample() {
            textarea.value = `# Simple HTTP API

A minimal REST API server in Rust.

## Features
- Health check endpoint
- JSON responses
- Graceful shutdown

## Tech
- Rust with tokio
- No external frameworks

\`\`\`rust
use std::net::SocketAddr;

#[tokio::main]
async fn main() {
    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    println!("Listening on {}", addr);
    // TODO: implement server
}
\`\`\`

subdomain = "api-demo"
port = 3000
`;
            charCount.textContent = textarea.value.length.toLocaleString() + ' chars';
        }

        async function loadProjects() {
            try {
                const res = await fetch('/api/projects');
                const data = await res.json();

                const list = document.getElementById('projectList');

                if (!data.projects || data.projects.length === 0) {
                    list.innerHTML = '<li class="empty-state">No projects yet</li>';
                    return;
                }

                list.innerHTML = data.projects.map(p => `
                    <li class="project-item ${p.status === 'building' ? 'building' : ''}"
                        onclick="showProject('${p.id}')">
                        <div class="project-name">${p.spec.name}</div>
                        <div class="project-meta">
                            <span class="status-badge status-${p.status}">${p.status}</span>
                            <span>${p.spec.project_type}</span>
                            <span>${new Date(p.created_at).toLocaleTimeString()}</span>
                        </div>
                    </li>
                `).join('');
            } catch (err) {
                console.error('Failed to load projects:', err);
            }
        }

        async function showProject(id) {
            try {
                const res = await fetch('/api/projects/' + id);
                const project = await res.json();

                document.getElementById('modalTitle').textContent = project.spec.name;
                document.getElementById('modalBody').innerHTML = `
                    <p><strong>Status:</strong>
                        <span class="status-badge status-${project.status}">${project.status}</span>
                    </p>
                    <p><strong>Type:</strong> ${project.spec.project_type}</p>
                    <p><strong>Directory:</strong> <code>${project.project_dir}</code></p>
                    ${project.url ? `<p><strong>URL:</strong> <a href="${project.url}" target="_blank">${project.url}</a></p>` : ''}
                    <h4 style="margin-top: 1rem; color: var(--fg-dark);">Log</h4>
                    <div class="log">
                        ${project.log.map(e => `
                            <div class="log-entry">
                                <span class="log-time">${new Date(e.timestamp).toLocaleTimeString()}</span>
                                <span class="log-kind">[${e.kind}]</span>
                                ${e.message}
                            </div>
                        `).join('')}
                    </div>
                `;

                document.getElementById('projectModal').classList.add('active');
            } catch (err) {
                alert('Failed to load project: ' + err.message);
            }
        }

        function closeModal() {
            document.getElementById('projectModal').classList.remove('active');
        }

        // Close modal on escape
        document.addEventListener('keydown', (e) => {
            if (e.key === 'Escape') closeModal();
        });

        // Poll for updates
        setInterval(loadProjects, 5000);
        loadProjects();
    </script>
</body>
</html>
"##;
