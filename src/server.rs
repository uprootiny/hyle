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
}

impl ServerState {
    pub fn new(api_key: String, model: String, work_dir: PathBuf) -> Self {
        Self {
            api_key,
            model,
            work_dir,
            busy: false,
        }
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
    println!("  GET  /status     - Server status");
    println!("  POST /prompt     - Run agent with prompt");
    println!("  POST /complete   - Simple completion (no tools)");
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
                    }
                }
                headers.push(line);
            }

            // Read body
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
                ("POST", "/prompt") => handle_prompt(&state, &body).await,
                ("POST", "/complete") => handle_complete(&state, &body).await,
                ("GET", "/") => Ok(json_response(200, &serde_json::json!({
                    "name": "hyle",
                    "version": env!("CARGO_PKG_VERSION"),
                    "endpoints": ["/status", "/prompt", "/complete"]
                }))),
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

async fn handle_status(state: &Arc<RwLock<ServerState>>) -> Result<String> {
    let state = state.read().await;
    let response = StatusResponse {
        version: env!("CARGO_PKG_VERSION").into(),
        model: state.model.clone(),
        work_dir: state.work_dir.display().to_string(),
        ready: !state.busy,
    };
    Ok(json_response(200, &serde_json::to_value(response)?))
}

async fn handle_prompt(state: &Arc<RwLock<ServerState>>, body: &str) -> Result<String> {
    let request: PromptRequest = serde_json::from_str(body)?;

    // Check if busy
    {
        let mut state = state.write().await;
        if state.busy {
            return Ok(json_response(503, &serde_json::json!({
                "error": "Server is busy processing another request"
            })));
        }
        state.busy = true;
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

    // Mark not busy
    {
        let mut state = state.write().await;
        state.busy = false;
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
