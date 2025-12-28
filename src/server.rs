//! HTTP Server mode for IDE integration
//!
//! Provides a REST API that IDE plugins can use to interact with hyle.
//! Similar to how Language Servers work, but for AI assistance.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::agent::{AgentCore, AgentEvent, AgentConfig};
use crate::config;

// ═══════════════════════════════════════════════════════════════
// API TYPES
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Serialize, Deserialize)]
pub struct PromptRequest {
    pub prompt: String,
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(default)]
    pub stream: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PromptResponse {
    pub success: bool,
    pub response: String,
    pub iterations: usize,
    pub tool_calls: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ToolCallInfo {
    pub name: String,
    pub success: bool,
    pub output: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StatusResponse {
    pub version: String,
    pub model: String,
    pub work_dir: String,
    pub ready: bool,
    pub rate_limits: RateLimitInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RateLimitInfo {
    pub requests_per_minute: u32,
    pub requests_used: u32,
    pub tokens_used: u64,
    pub context_window: u64,
}

/// Session/conversation for web UI
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: String,
    pub title: String,
    pub messages: Vec<ConversationMessage>,
    pub created: String,
    pub tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationMessage {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
}

/// List sessions request
#[derive(Debug, Serialize, Deserialize)]
pub struct SessionsResponse {
    pub sessions: Vec<SessionInfo>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: String,
    pub model: String,
    pub messages: usize,
    pub tokens: u64,
    pub created: String,
    pub updated: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StreamEvent {
    pub event_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<ToolCallInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iteration: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

// ═══════════════════════════════════════════════════════════════
// SERVER STATE
// ═══════════════════════════════════════════════════════════════

pub struct ServerState {
    api_key: String,
    model: String,
    work_dir: PathBuf,
    busy: bool,
    rate_limits: RateLimitInfo,
    request_times: Vec<std::time::Instant>,
}

impl ServerState {
    pub fn new(api_key: String, model: String, work_dir: PathBuf) -> Self {
        Self {
            api_key,
            model,
            work_dir,
            busy: false,
            rate_limits: RateLimitInfo {
                requests_per_minute: 20, // Conservative default
                requests_used: 0,
                tokens_used: 0,
                context_window: 128000,
            },
            request_times: Vec::new(),
        }
    }

    fn record_request(&mut self) {
        let now = std::time::Instant::now();
        // Clean up old requests (older than 1 minute)
        self.request_times.retain(|t| now.duration_since(*t).as_secs() < 60);
        self.request_times.push(now);
        self.rate_limits.requests_used = self.request_times.len() as u32;
    }

    fn add_tokens(&mut self, tokens: u64) {
        self.rate_limits.tokens_used += tokens;
    }
}

// ═══════════════════════════════════════════════════════════════
// SIMPLE HTTP SERVER (no external deps)
// ═══════════════════════════════════════════════════════════════

/// Run the HTTP server
pub async fn run_server(port: u16) -> Result<()> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpListener;

    let api_key = config::get_api_key()?;
    let cfg = config::Config::load()?;
    let model = cfg.default_model.unwrap_or_else(|| "meta-llama/llama-3.2-3b-instruct:free".into());
    let work_dir = std::env::current_dir()?;

    let state = Arc::new(RwLock::new(ServerState::new(api_key, model, work_dir)));

    let addr: SocketAddr = ([127, 0, 0, 1], port).into();
    let listener = TcpListener::bind(addr).await?;

    println!("hyle server listening on http://{}", addr);
    println!("Endpoints:");
    println!("  GET  /status      - Server status + rate limits");
    println!("  GET  /sessions    - List saved sessions");
    println!("  GET  /session/:id - Get session by ID");
    println!("  POST /prompt      - Run agent with prompt");
    println!("  POST /complete    - Simple completion (no tools)");
    println!("  POST /stream      - SSE streaming completion");
    println!("Press Ctrl-C to stop\n");

    loop {
        let (mut socket, peer) = listener.accept().await?;
        let state = state.clone();

        tokio::spawn(async move {
            let (reader, mut writer) = socket.split();
            let mut reader = BufReader::new(reader);
            let mut request = String::new();
            let mut headers = Vec::new();
            let mut content_length = 0usize;

            // Read request line
            if reader.read_line(&mut request).await.is_err() {
                return;
            }

            // Read headers
            const MAX_BODY_SIZE: usize = 10 * 1024 * 1024; // 10MB cap
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).await.is_err() {
                    return;
                }
                if line.trim().is_empty() {
                    break;
                }
                if line.to_lowercase().starts_with("content-length:") {
                    if let Some(len) = line.split(':').nth(1) {
                        content_length = len.trim().parse().unwrap_or(0);
                        // Cap to prevent memory exhaustion DoS
                        if content_length > MAX_BODY_SIZE {
                            let _ = writer.write_all(b"HTTP/1.1 413 Payload Too Large\r\n\r\n").await;
                            return;
                        }
                    }
                }
                headers.push(line);
            }

            // Read body (size already validated)
            let mut body = vec![0u8; content_length];
            if content_length > 0 {
                use tokio::io::AsyncReadExt;
                if reader.read_exact(&mut body).await.is_err() {
                    return;
                }
            }
            let body = String::from_utf8_lossy(&body).to_string();

            // Parse request
            let parts: Vec<&str> = request.split_whitespace().collect();
            let (method, path) = match parts.as_slice() {
                [m, p, ..] => (*m, *p),
                _ => {
                    let _ = writer.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n").await;
                    return;
                }
            };

            println!("[{}] {} {}", peer, method, path);

            // Route request
            let response = match (method, path) {
                ("GET", "/status") => handle_status(&state).await,
                ("GET", "/sessions") => handle_sessions().await,
                ("POST", "/prompt") => handle_prompt(&state, &body).await,
                ("POST", "/complete") => handle_complete(&state, &body).await,
                ("OPTIONS", _) => Ok(cors_preflight()),
                ("GET", "/") => Ok(html_response(WEB_UI_HTML)),
                ("GET", "/api") => Ok(json_response(200, &serde_json::json!({
                    "name": "hyle",
                    "version": env!("CARGO_PKG_VERSION"),
                    "endpoints": ["/status", "/sessions", "/prompt", "/complete"],
                    "docs": "POST /prompt with {\"prompt\": \"...\", \"files\": [...]} for agent mode"
                }))),
                (_, p) if p.starts_with("/session/") => {
                    let id = p.trim_start_matches("/session/");
                    // Validate session ID format to prevent path traversal
                    if !id.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
                        Ok(json_response(400, &serde_json::json!({"error": "Invalid session ID format"})))
                    } else {
                        handle_session(id).await
                    }
                }
                _ => Ok(json_response(404, &serde_json::json!({"error": "Not found"}))),
            };

            let response = response.unwrap_or_else(|e| {
                json_response(500, &serde_json::json!({"error": e.to_string()}))
            });

            let _ = writer.write_all(response.as_bytes()).await;
        });
    }
}

fn json_response(status: u16, body: &serde_json::Value) -> String {
    let status_text = match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "Unknown",
    };
    let body_str = serde_json::to_string(body).unwrap_or_else(|_| "{}".into());
    format!(
        "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\n\r\n{}",
        status, status_text, body_str.len(), body_str
    )
}

fn html_response(html: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\n\r\n{}",
        html.len(), html
    )
}

// ═══════════════════════════════════════════════════════════════
// WEB UI
// ═══════════════════════════════════════════════════════════════

const WEB_UI_HTML: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>hyle</title>
  <style>
    :root {
      --bg: #1a1b26;
      --fg: #c0caf5;
      --dim: #565f89;
      --accent: #7aa2f7;
      --success: #9ece6a;
      --error: #f7768e;
      --surface: #24283b;
      --border: #414868;
    }
    * { box-sizing: border-box; margin: 0; padding: 0; }
    body {
      font-family: 'SF Mono', 'Fira Code', 'Consolas', monospace;
      font-size: 14px;
      background: var(--bg);
      color: var(--fg);
      height: 100vh;
      display: flex;
      flex-direction: column;
    }
    header {
      padding: 12px 20px;
      border-bottom: 1px solid var(--border);
      display: flex;
      justify-content: space-between;
      align-items: center;
    }
    header h1 { font-size: 18px; color: var(--accent); font-weight: 500; }
    #status { color: var(--dim); font-size: 12px; }
    #status.ready { color: var(--success); }
    #status.busy { color: var(--error); }
    main {
      flex: 1;
      overflow-y: auto;
      padding: 20px;
    }
    .msg {
      margin-bottom: 16px;
      padding: 12px 16px;
      border-radius: 8px;
      max-width: 85%;
      line-height: 1.5;
      white-space: pre-wrap;
      word-break: break-word;
    }
    .msg.user {
      background: var(--surface);
      margin-left: auto;
      border: 1px solid var(--border);
    }
    .msg.assistant {
      background: transparent;
      border-left: 3px solid var(--accent);
      padding-left: 16px;
    }
    .msg.error {
      background: rgba(247, 118, 142, 0.1);
      border-left-color: var(--error);
      color: var(--error);
    }
    .msg .meta {
      font-size: 11px;
      color: var(--dim);
      margin-top: 8px;
    }
    footer {
      padding: 16px 20px;
      border-top: 1px solid var(--border);
      background: var(--surface);
    }
    #input-form {
      display: flex;
      gap: 12px;
    }
    #prompt {
      flex: 1;
      background: var(--bg);
      border: 1px solid var(--border);
      border-radius: 6px;
      padding: 12px 16px;
      color: var(--fg);
      font-family: inherit;
      font-size: 14px;
      resize: none;
      min-height: 44px;
      max-height: 200px;
    }
    #prompt:focus {
      outline: none;
      border-color: var(--accent);
    }
    button {
      background: var(--accent);
      color: var(--bg);
      border: none;
      border-radius: 6px;
      padding: 12px 24px;
      font-family: inherit;
      font-size: 14px;
      font-weight: 500;
      cursor: pointer;
      transition: opacity 0.2s;
    }
    button:hover { opacity: 0.9; }
    button:disabled {
      opacity: 0.5;
      cursor: not-allowed;
    }
    .tools {
      margin-top: 8px;
      font-size: 12px;
      color: var(--dim);
    }
    code {
      background: var(--surface);
      padding: 2px 6px;
      border-radius: 4px;
      font-size: 13px;
    }
    pre {
      background: var(--surface);
      padding: 12px;
      border-radius: 6px;
      overflow-x: auto;
      margin: 8px 0;
    }
  </style>
</head>
<body>
  <header>
    <h1>hyle</h1>
    <span id="status">connecting...</span>
  </header>
  <main id="messages"></main>
  <footer>
    <form id="input-form">
      <textarea id="prompt" placeholder="Ask anything..." rows="1"></textarea>
      <button type="submit" id="send">Send</button>
    </form>
  </footer>
  <script>
    const messages = document.getElementById('messages');
    const form = document.getElementById('input-form');
    const prompt = document.getElementById('prompt');
    const sendBtn = document.getElementById('send');
    const status = document.getElementById('status');

    // Auto-resize textarea
    prompt.addEventListener('input', () => {
      prompt.style.height = 'auto';
      prompt.style.height = Math.min(prompt.scrollHeight, 200) + 'px';
    });

    // Submit on Enter (Shift+Enter for newline)
    prompt.addEventListener('keydown', (e) => {
      if (e.key === 'Enter' && !e.shiftKey) {
        e.preventDefault();
        form.dispatchEvent(new Event('submit'));
      }
    });

    // Check status
    async function checkStatus() {
      try {
        const res = await fetch('/status');
        const data = await res.json();
        status.textContent = data.ready ? `ready | ${data.model.split('/').pop()}` : 'busy';
        status.className = data.ready ? 'ready' : 'busy';
      } catch {
        status.textContent = 'offline';
        status.className = '';
      }
    }
    checkStatus();
    setInterval(checkStatus, 5000);

    function addMessage(role, content, meta = null) {
      const div = document.createElement('div');
      div.className = `msg ${role}`;
      div.textContent = content;
      if (meta) {
        const metaDiv = document.createElement('div');
        metaDiv.className = 'meta';
        metaDiv.textContent = meta;
        div.appendChild(metaDiv);
      }
      messages.appendChild(div);
      messages.scrollTop = messages.scrollHeight;
    }

    form.addEventListener('submit', async (e) => {
      e.preventDefault();
      const text = prompt.value.trim();
      if (!text) return;

      addMessage('user', text);
      prompt.value = '';
      prompt.style.height = 'auto';
      sendBtn.disabled = true;
      status.textContent = 'thinking...';
      status.className = 'busy';

      try {
        const res = await fetch('/prompt', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ prompt: text, files: [] })
        });
        const data = await res.json();

        if (data.success) {
          const meta = `${data.iterations} iterations, ${data.tool_calls} tool calls`;
          addMessage('assistant', data.response, meta);
        } else {
          addMessage('error', data.error || 'Unknown error');
        }
      } catch (err) {
        addMessage('error', 'Connection error: ' + err.message);
      }

      sendBtn.disabled = false;
      checkStatus();
    });

    // Focus input
    prompt.focus();
  </script>
</body>
</html>
"##;

async fn handle_status(state: &Arc<RwLock<ServerState>>) -> Result<String> {
    let state = state.read().await;
    let response = StatusResponse {
        version: env!("CARGO_PKG_VERSION").into(),
        model: state.model.clone(),
        work_dir: state.work_dir.display().to_string(),
        ready: !state.busy,
        rate_limits: state.rate_limits.clone(),
    };
    Ok(json_response(200, &serde_json::to_value(response)?))
}

fn cors_preflight() -> String {
    "HTTP/1.1 204 No Content\r\n\
     Access-Control-Allow-Origin: *\r\n\
     Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n\
     Access-Control-Allow-Headers: Content-Type\r\n\
     Access-Control-Max-Age: 86400\r\n\r\n".to_string()
}

async fn handle_sessions() -> Result<String> {
    let sessions = crate::session::list_sessions().unwrap_or_default();
    let session_infos: Vec<SessionInfo> = sessions.iter().map(|s| SessionInfo {
        id: s.id.clone(),
        model: s.model.clone(),
        messages: s.message_count,
        tokens: s.total_tokens,
        created: s.created_at.to_rfc3339(),
        updated: s.updated_at.to_rfc3339(),
    }).collect();

    let response = SessionsResponse { sessions: session_infos };
    Ok(json_response(200, &serde_json::to_value(response)?))
}

async fn handle_session(id: &str) -> Result<String> {
    match crate::session::Session::load(id) {
        Ok(session) => {
            let messages: Vec<ConversationMessage> = session.messages.iter().map(|m| {
                ConversationMessage {
                    role: m.role.clone(),
                    content: m.content.clone(),
                    tool: None,
                    timestamp: None,
                }
            }).collect();

            let conv = Conversation {
                id: session.meta.id.clone(),
                title: session.meta.description.clone().unwrap_or_else(|| {
                    format!("Session {}", &session.meta.id[..8])
                }),
                messages,
                created: session.meta.created_at.to_rfc3339(),
                tokens: session.meta.total_tokens,
            };
            Ok(json_response(200, &serde_json::to_value(conv)?))
        }
        Err(e) => Ok(json_response(404, &serde_json::json!({
            "error": format!("Session not found: {}", e)
        }))),
    }
}

async fn handle_prompt(state: &Arc<RwLock<ServerState>>, body: &str) -> Result<String> {
    let request: PromptRequest = serde_json::from_str(body)?;

    // Check if busy and record request
    {
        let mut state = state.write().await;
        if state.busy {
            return Ok(json_response(503, &serde_json::json!({
                "error": "Server is busy processing another request"
            })));
        }
        state.busy = true;
        state.record_request();
    }

    // Get state info
    let (api_key, model, work_dir) = {
        let state = state.read().await;
        (state.api_key.clone(), state.model.clone(), state.work_dir.clone())
    };

    // Build prompt with file context
    let mut full_prompt = request.prompt;
    for file_path in &request.files {
        if let Ok(content) = std::fs::read_to_string(file_path) {
            full_prompt = format!("{}\n\n--- {} ---\n{}", full_prompt, file_path, content);
        }
    }

    // Run agent
    let agent = AgentCore::new(&api_key, &model, &work_dir)
        .with_config(AgentConfig {
            max_iterations: 10,
            max_tool_calls_per_iteration: 5,
            timeout_per_tool_ms: 30000,
        });

    let mut last_response = String::new();
    let result = agent.run_with_callback(&full_prompt, |event| {
        if let AgentEvent::Token(t) = event {
            last_response.push_str(t);
        }
    }).await;

    // Mark not busy and record token usage
    {
        let mut state = state.write().await;
        state.busy = false;
        state.add_tokens(result.tokens_used as u64);
    }

    let response = PromptResponse {
        success: result.success,
        response: result.final_response,
        iterations: result.iterations,
        tool_calls: result.tool_calls_executed,
        error: result.error,
    };

    Ok(json_response(200, &serde_json::to_value(response)?))
}

async fn handle_complete(state: &Arc<RwLock<ServerState>>, body: &str) -> Result<String> {
    let request: PromptRequest = serde_json::from_str(body)?;

    let (api_key, model) = {
        let state = state.read().await;
        (state.api_key.clone(), state.model.clone())
    };

    // Simple completion without agent loop
    let mut response = String::new();
    let mut stream = crate::client::stream_completion(&api_key, &model, &request.prompt).await?;

    while let Some(event) = stream.recv().await {
        match event {
            crate::client::StreamEvent::Token(t) => response.push_str(&t),
            crate::client::StreamEvent::Done(_) => break,
            crate::client::StreamEvent::Error(e) => {
                return Ok(json_response(500, &serde_json::json!({"error": e})));
            }
        }
    }

    Ok(json_response(200, &serde_json::json!({
        "success": true,
        "response": response
    })))
}

// ═══════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_response() {
        let resp = json_response(200, &serde_json::json!({"test": true}));
        assert!(resp.contains("200 OK"));
        assert!(resp.contains("application/json"));
        assert!(resp.contains("\"test\":true"));
    }

    #[test]
    fn test_prompt_request_parse() {
        let json = r#"{"prompt": "hello", "files": ["test.rs"]}"#;
        let req: PromptRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.prompt, "hello");
        assert_eq!(req.files.len(), 1);
    }
}
