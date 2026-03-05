//! OpenRouter API client with SSE streaming
//!
//! Provides:
//! - Shared HTTP client with connection pooling
//! - SSE streaming for chat completions
//! - Typed error handling with rate limit detection
//! - Retry with exponential backoff for transient failures

use anyhow::{Context, Result};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use tokio::sync::mpsc;

use crate::models::Model;

use crate::project::Project;
use crate::prompt::SystemPrompt;

// ═══════════════════════════════════════════════════════════════
// SHARED HTTP CLIENT
// ═══════════════════════════════════════════════════════════════

/// Shared reqwest client - reuses connection pools and TLS sessions
fn shared_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .pool_max_idle_per_host(4)
            .timeout(std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .build()
            .expect("Failed to build HTTP client")
    })
}

// ═══════════════════════════════════════════════════════════════
// TYPED ERRORS
// ═══════════════════════════════════════════════════════════════

/// Structured API errors for programmatic handling
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiError {
    /// 401/403 - Invalid or missing API key
    AuthFailed { status: u16, body: String },
    /// 429 - Rate limited, with optional retry-after hint
    RateLimited { retry_after_ms: Option<u64>, body: String },
    /// 5xx - Server error (transient, retryable)
    ServerError { status: u16, body: String },
    /// Network/connection failure (transient, retryable)
    Network(String),
    /// Request timed out (transient, retryable)
    Timeout,
    /// Stream interrupted mid-response
    StreamInterrupted(String),
    /// Unparseable response from API
    InvalidResponse(String),
}

impl ApiError {
    /// Whether this error is transient and worth retrying
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            ApiError::ServerError { .. }
                | ApiError::Network(_)
                | ApiError::Timeout
                | ApiError::StreamInterrupted(_)
        )
    }

    /// Whether this is a rate limit error
    pub fn is_rate_limited(&self) -> bool {
        matches!(self, ApiError::RateLimited { .. })
    }

    /// Classify an HTTP status + body into a typed error
    fn from_status(status: u16, body: String) -> Self {
        match status {
            401 | 403 => ApiError::AuthFailed { status, body },
            429 => {
                // Try to extract retry-after from body
                let retry_after_ms = extract_retry_after(&body);
                ApiError::RateLimited { retry_after_ms, body }
            }
            500..=599 => ApiError::ServerError { status, body },
            _ => ApiError::InvalidResponse(format!("HTTP {}: {}", status, body)),
        }
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiError::AuthFailed { status, body } => {
                write!(f, "Authentication failed ({}): {}", status, truncate_body(body))
            }
            ApiError::RateLimited { retry_after_ms, .. } => {
                if let Some(ms) = retry_after_ms {
                    write!(f, "Rate limited (retry after {}ms)", ms)
                } else {
                    write!(f, "Rate limited")
                }
            }
            ApiError::ServerError { status, body } => {
                write!(f, "Server error ({}): {}", status, truncate_body(body))
            }
            ApiError::Network(msg) => write!(f, "Network error: {}", msg),
            ApiError::Timeout => write!(f, "Request timed out"),
            ApiError::StreamInterrupted(msg) => write!(f, "Stream interrupted: {}", msg),
            ApiError::InvalidResponse(msg) => write!(f, "Invalid response: {}", msg),
        }
    }
}

impl std::error::Error for ApiError {}

/// Extract retry-after hint from error body (seconds -> ms)
fn extract_retry_after(body: &str) -> Option<u64> {
    // Try JSON: {"error": {"metadata": {"retry_after": 2}}}
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(body) {
        if let Some(secs) = v
            .pointer("/error/metadata/retry_after")
            .and_then(|v| v.as_f64())
        {
            return Some((secs * 1000.0) as u64);
        }
    }
    None
}

fn truncate_body(s: &str) -> &str {
    if s.len() <= 200 { s } else { &s[..200] }
}

// ═══════════════════════════════════════════════════════════════
// PUBLIC API
// ═══════════════════════════════════════════════════════════════

/// Build system prompt with optional project context
fn build_system_prompt(project: Option<&Project>) -> String {
    let mut builder = SystemPrompt::new();

    if let Some(p) = project {
        builder = builder.with_project(p.clone());
    }

    builder.build()
}

const OPENROUTER_API_URL: &str = "https://openrouter.ai/api/v1/chat/completions";
const OPENROUTER_MODELS_URL: &str = "https://openrouter.ai/api/v1/models";

/// Token usage statistics
#[derive(Debug, Clone, Default)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// Stream events from the API
#[derive(Debug)]
pub enum StreamEvent {
    /// A token/chunk of text
    Token(String),
    /// Stream finished with usage stats
    Done(TokenUsage),
    /// Error occurred
    Error(String),
}

/// Simple non-streaming chat completion (for benchmarks)
pub async fn chat_completion_simple(
    api_key: &str,
    model: &str,
    prompt: &str,
    max_tokens: u32,
) -> Result<String> {
    let mut rx = stream_completion_configurable(api_key, model, prompt, None, &[], Some(max_tokens), None).await?;
    let mut response = String::new();

    while let Some(event) = rx.recv().await {
        match event {
            StreamEvent::Token(t) => response.push_str(&t),
            StreamEvent::Done(_) => break,
            StreamEvent::Error(e) => anyhow::bail!("API error: {}", e),
        }
    }

    Ok(response)
}

/// Check connectivity to OpenRouter
pub async fn check_connectivity() -> Result<()> {
    shared_client()
        .get("https://openrouter.ai/api/v1/models")
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .context("Failed to connect to OpenRouter")?;
    Ok(())
}

/// Fetch models list from OpenRouter
pub async fn fetch_models(api_key: &str) -> Result<Vec<Model>> {
    let response = shared_client()
        .get(OPENROUTER_MODELS_URL)
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await
        .context("Failed to fetch models")?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        return Err(ApiError::from_status(status, body).into());
    }

    let data: ModelsResponse = response
        .json()
        .await
        .context("Failed to parse models response")?;

    let models: Vec<Model> = data
        .data
        .into_iter()
        .map(|m| {
            let (pricing_prompt, pricing_completion) = match m.pricing {
                Some(p) => (
                    p.prompt.parse().unwrap_or(0.0),
                    p.completion.parse().unwrap_or(0.0),
                ),
                None => (0.0, 0.0),
            };

            Model {
                id: m.id,
                name: m.name.unwrap_or_default(),
                context_length: m.context_length.unwrap_or(4096),
                pricing_prompt,
                pricing_completion,
            }
        })
        .collect();

    Ok(models)
}

/// Stream a chat completion from OpenRouter
pub async fn stream_completion(
    api_key: &str,
    model: &str,
    prompt: &str,
) -> Result<mpsc::Receiver<StreamEvent>> {
    stream_completion_with_context(api_key, model, prompt, None).await
}

/// Stream a chat completion with project context
pub async fn stream_completion_with_context(
    api_key: &str,
    model: &str,
    prompt: &str,
    project: Option<&Project>,
) -> Result<mpsc::Receiver<StreamEvent>> {
    stream_completion_full(api_key, model, prompt, project, &[]).await
}

/// Stream a chat completion with full context (project + history)
pub async fn stream_completion_full(
    api_key: &str,
    model: &str,
    prompt: &str,
    project: Option<&Project>,
    history: &[serde_json::Value],
) -> Result<mpsc::Receiver<StreamEvent>> {
    stream_completion_configurable(api_key, model, prompt, project, history, None, None).await
}

/// Stream a chat completion with all options configurable
fn stream_completion_configurable<'a>(
    api_key: &'a str,
    model: &'a str,
    prompt: &'a str,
    project: Option<&'a Project>,
    history: &'a [serde_json::Value],
    max_tokens: Option<u32>,
    temperature: Option<f32>,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<mpsc::Receiver<StreamEvent>>> + Send + 'a>> {
    let max_tokens = max_tokens.unwrap_or(4096);
    let temperature = temperature.unwrap_or(0.7);

    Box::pin(async move {
        let (tx, rx) = mpsc::channel(256);

        let system_prompt = build_system_prompt(project);

        // Build messages: system + history + current user message
        let mut messages = vec![ChatMessage {
            role: "system".to_string(),
            content: system_prompt,
        }];

        // Add conversation history
        for msg in history {
            if let (Some(role), Some(content)) = (
                msg.get("role").and_then(|v| v.as_str()),
                msg.get("content").and_then(|v| v.as_str()),
            ) {
                messages.push(ChatMessage {
                    role: role.to_string(),
                    content: content.to_string(),
                });
            }
        }

        // Add current user message
        messages.push(ChatMessage {
            role: "user".to_string(),
            content: prompt.to_string(),
        });

        let request = ChatRequest {
            model: model.to_string(),
            messages,
            stream: true,
            max_tokens: Some(max_tokens),
            temperature: Some(temperature),
        };

        let client = shared_client().clone();
        let api_key = api_key.to_string();

        tokio::spawn(async move {
            match do_stream(&client, &api_key, &request, &tx).await {
                Ok(usage) => {
                    let _ = tx.send(StreamEvent::Done(usage)).await;
                }
                Err(e) => {
                    let _ = tx.send(StreamEvent::Error(e.to_string())).await;
                }
            }
        });

        Ok(rx)
    })
}

// ═══════════════════════════════════════════════════════════════
// STREAMING INTERNALS
// ═══════════════════════════════════════════════════════════════

/// Request timeout in seconds
const REQUEST_TIMEOUT_SECS: u64 = 120;
/// Max retries for transient errors
const MAX_RETRIES: u32 = 3;
/// Base delay for exponential backoff (ms)
const RETRY_BASE_DELAY_MS: u64 = 500;

/// Perform the actual streaming request with retry
async fn do_stream(
    client: &reqwest::Client,
    api_key: &str,
    request: &ChatRequest,
    tx: &mpsc::Sender<StreamEvent>,
) -> Result<TokenUsage> {
    let mut last_error = None;

    for attempt in 0..MAX_RETRIES {
        if attempt > 0 {
            // Exponential backoff: 500ms, 1s, 2s
            let delay = RETRY_BASE_DELAY_MS * (1 << (attempt - 1));
            let _ = tx
                .send(StreamEvent::Token(format!(
                    "\n[Retrying in {}ms...]\n",
                    delay
                )))
                .await;
            tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
        }

        match do_stream_attempt(client, api_key, request, tx).await {
            Ok(usage) => return Ok(usage),
            Err(e) => {
                // Check if it's a typed ApiError
                if let Some(api_err) = e.downcast_ref::<ApiError>() {
                    if !api_err.is_retryable() {
                        return Err(e);
                    }
                    last_error = Some(e);
                    continue;
                }
                // Fallback: classify by string for reqwest errors
                let err_str = e.to_string();
                if err_str.contains("timeout")
                    || err_str.contains("connect")
                    || err_str.contains("reset")
                    || err_str.contains("closed")
                {
                    last_error = Some(e);
                    continue;
                }
                return Err(e);
            }
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("Max retries exceeded")))
}

/// Single attempt at streaming request
async fn do_stream_attempt(
    client: &reqwest::Client,
    api_key: &str,
    request: &ChatRequest,
    tx: &mpsc::Sender<StreamEvent>,
) -> Result<TokenUsage> {
    let response = client
        .post(OPENROUTER_API_URL)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .header("HTTP-Referer", "https://github.com/uprootiny/hyle")
        .header("X-Title", "hyle")
        .timeout(std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .json(request)
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                ApiError::Timeout
            } else if e.is_connect() {
                ApiError::Network(e.to_string())
            } else {
                ApiError::Network(e.to_string())
            }
        })?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        return Err(ApiError::from_status(status, body).into());
    }

    let mut usage = TokenUsage::default();
    let mut bytes_stream = response.bytes_stream();

    // Buffer for incomplete SSE lines
    let mut buffer = String::new();

    while let Some(chunk) = bytes_stream.next().await {
        let chunk = chunk.map_err(|e| ApiError::StreamInterrupted(e.to_string()))?;
        let text = String::from_utf8_lossy(&chunk);
        buffer.push_str(&text);

        // Process complete lines
        while let Some(newline_pos) = buffer.find('\n') {
            let line = buffer[..newline_pos].trim().to_string();
            buffer = buffer[newline_pos + 1..].to_string();

            if line.is_empty() {
                continue;
            }

            if let Some(data) = line.strip_prefix("data: ") {
                if data == "[DONE]" {
                    continue;
                }

                if let Ok(chunk) = serde_json::from_str::<StreamChunk>(data) {
                    // Extract content from choices
                    if let Some(choice) = chunk.choices.first() {
                        if let Some(delta) = &choice.delta {
                            if let Some(content) = &delta.content {
                                if !content.is_empty() {
                                    let _ = tx.send(StreamEvent::Token(content.clone())).await;
                                }
                            }
                        }
                    }

                    // Extract usage if present
                    if let Some(u) = chunk.usage {
                        usage.prompt_tokens = u.prompt_tokens;
                        usage.completion_tokens = u.completion_tokens;
                        usage.total_tokens = u.total_tokens;
                    }
                }
                // Silently skip unparseable SSE data lines (common with some providers)
            }
        }
    }

    Ok(usage)
}

/// Parse SSE lines from a text buffer, extracting complete data payloads.
/// Returns parsed payloads and any remaining incomplete buffer content.
fn parse_sse_lines(buffer: &mut String) -> Vec<SsePayload> {
    let mut payloads = Vec::new();

    while let Some(newline_pos) = buffer.find('\n') {
        let line = buffer[..newline_pos].trim().to_string();
        *buffer = buffer[newline_pos + 1..].to_string();

        if line.is_empty() {
            continue;
        }

        if let Some(data) = line.strip_prefix("data: ") {
            if data == "[DONE]" {
                payloads.push(SsePayload::Done);
            } else if let Ok(chunk) = serde_json::from_str::<StreamChunk>(data) {
                payloads.push(SsePayload::Chunk(chunk));
            }
            // Skip unparseable lines
        }
    }

    payloads
}

/// Parsed SSE payload
#[derive(Debug)]
enum SsePayload {
    Chunk(StreamChunk),
    Done,
}

// ═══════════════════════════════════════════════════════════════
// API Types
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

#[derive(Debug, Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct StreamChunk {
    #[serde(default)]
    choices: Vec<StreamChoice>,
    usage: Option<StreamUsage>,
}

#[derive(Debug, Deserialize)]
struct StreamChoice {
    delta: Option<StreamDelta>,
}

#[derive(Debug, Deserialize)]
struct StreamDelta {
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct StreamUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct ModelsResponse {
    data: Vec<ApiModel>,
}

#[derive(Debug, Deserialize)]
struct ApiModel {
    id: String,
    name: Option<String>,
    context_length: Option<u32>,
    pricing: Option<ApiPricing>,
}

#[derive(Debug, Deserialize)]
struct ApiPricing {
    prompt: String,
    completion: String,
}

// ═══════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // --- SSE parsing tests ---

    #[test]
    fn test_parse_stream_chunk() {
        let json = r#"{"choices":[{"delta":{"content":"Hello"}}]}"#;
        let chunk: StreamChunk = serde_json::from_str(json).unwrap();
        assert_eq!(
            chunk.choices[0]
                .delta
                .as_ref()
                .unwrap()
                .content
                .as_ref()
                .unwrap(),
            "Hello"
        );
    }

    #[test]
    fn test_parse_stream_chunk_with_usage() {
        let json = r#"{"choices":[],"usage":{"prompt_tokens":10,"completion_tokens":20,"total_tokens":30}}"#;
        let chunk: StreamChunk = serde_json::from_str(json).unwrap();
        let usage = chunk.usage.unwrap();
        assert_eq!(usage.prompt_tokens, 10);
        assert_eq!(usage.completion_tokens, 20);
        assert_eq!(usage.total_tokens, 30);
    }

    #[test]
    fn test_parse_stream_chunk_empty_choices() {
        let json = r#"{"choices":[]}"#;
        let chunk: StreamChunk = serde_json::from_str(json).unwrap();
        assert!(chunk.choices.is_empty());
        assert!(chunk.usage.is_none());
    }

    #[test]
    fn test_parse_stream_chunk_null_delta_content() {
        let json = r#"{"choices":[{"delta":{}}]}"#;
        let chunk: StreamChunk = serde_json::from_str(json).unwrap();
        assert!(chunk.choices[0].delta.as_ref().unwrap().content.is_none());
    }

    // --- SSE line parser tests ---

    #[test]
    fn test_parse_sse_lines_single_chunk() {
        let mut buf = "data: {\"choices\":[{\"delta\":{\"content\":\"Hi\"}}]}\n".to_string();
        let payloads = parse_sse_lines(&mut buf);
        assert_eq!(payloads.len(), 1);
        assert!(matches!(payloads[0], SsePayload::Chunk(_)));
        assert!(buf.is_empty());
    }

    #[test]
    fn test_parse_sse_lines_done() {
        let mut buf = "data: [DONE]\n".to_string();
        let payloads = parse_sse_lines(&mut buf);
        assert_eq!(payloads.len(), 1);
        assert!(matches!(payloads[0], SsePayload::Done));
    }

    #[test]
    fn test_parse_sse_lines_multiple() {
        let mut buf = "data: {\"choices\":[{\"delta\":{\"content\":\"A\"}}]}\n\ndata: {\"choices\":[{\"delta\":{\"content\":\"B\"}}]}\ndata: [DONE]\n".to_string();
        let payloads = parse_sse_lines(&mut buf);
        assert_eq!(payloads.len(), 3);
        assert!(matches!(payloads[0], SsePayload::Chunk(_)));
        assert!(matches!(payloads[1], SsePayload::Chunk(_)));
        assert!(matches!(payloads[2], SsePayload::Done));
    }

    #[test]
    fn test_parse_sse_lines_incomplete_buffer() {
        let mut buf = "data: {\"choices\":[{\"delta\":{\"content\":\"partial".to_string();
        let payloads = parse_sse_lines(&mut buf);
        assert!(payloads.is_empty());
        // Incomplete data stays in buffer
        assert!(buf.contains("partial"));
    }

    #[test]
    fn test_parse_sse_lines_skips_invalid_json() {
        let mut buf = "data: not-json\ndata: {\"choices\":[]}\n".to_string();
        let payloads = parse_sse_lines(&mut buf);
        // Invalid JSON is silently skipped, valid one is parsed
        assert_eq!(payloads.len(), 1);
        assert!(matches!(payloads[0], SsePayload::Chunk(_)));
    }

    #[test]
    fn test_parse_sse_lines_skips_non_data_lines() {
        let mut buf = "event: heartbeat\nid: 123\ndata: {\"choices\":[]}\n".to_string();
        let payloads = parse_sse_lines(&mut buf);
        assert_eq!(payloads.len(), 1); // Only the data: line is parsed
    }

    // --- Models response tests ---

    #[test]
    fn test_parse_models_response() {
        let json = r#"{"data":[{"id":"test/model","context_length":8192,"pricing":{"prompt":"0","completion":"0"}}]}"#;
        let resp: ModelsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.data.len(), 1);
        assert_eq!(resp.data[0].id, "test/model");
    }

    #[test]
    fn test_parse_models_response_no_pricing() {
        let json = r#"{"data":[{"id":"free/model","name":"Free Model","context_length":4096}]}"#;
        let resp: ModelsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.data.len(), 1);
        assert!(resp.data[0].pricing.is_none());
    }

    #[test]
    fn test_parse_models_response_empty() {
        let json = r#"{"data":[]}"#;
        let resp: ModelsResponse = serde_json::from_str(json).unwrap();
        assert!(resp.data.is_empty());
    }

    // --- ApiError tests ---

    #[test]
    fn test_api_error_from_status_auth() {
        let err = ApiError::from_status(401, "Unauthorized".into());
        assert!(matches!(err, ApiError::AuthFailed { status: 401, .. }));
        assert!(!err.is_retryable());
        assert!(!err.is_rate_limited());
    }

    #[test]
    fn test_api_error_from_status_rate_limit() {
        let err = ApiError::from_status(429, "Too many requests".into());
        assert!(matches!(err, ApiError::RateLimited { .. }));
        assert!(!err.is_retryable()); // Rate limits need special handling, not blind retry
        assert!(err.is_rate_limited());
    }

    #[test]
    fn test_api_error_from_status_rate_limit_with_retry_after() {
        let body = r#"{"error":{"message":"Rate limited","metadata":{"retry_after":2.5}}}"#;
        let err = ApiError::from_status(429, body.into());
        match err {
            ApiError::RateLimited { retry_after_ms, .. } => {
                assert_eq!(retry_after_ms, Some(2500));
            }
            _ => panic!("Expected RateLimited"),
        }
    }

    #[test]
    fn test_api_error_from_status_server_error() {
        let err = ApiError::from_status(500, "Internal Server Error".into());
        assert!(matches!(err, ApiError::ServerError { status: 500, .. }));
        assert!(err.is_retryable());
    }

    #[test]
    fn test_api_error_from_status_502() {
        let err = ApiError::from_status(502, "Bad Gateway".into());
        assert!(matches!(err, ApiError::ServerError { status: 502, .. }));
        assert!(err.is_retryable());
    }

    #[test]
    fn test_api_error_display() {
        let err = ApiError::Timeout;
        assert_eq!(err.to_string(), "Request timed out");

        let err = ApiError::RateLimited { retry_after_ms: Some(1000), body: String::new() };
        assert!(err.to_string().contains("1000ms"));

        let err = ApiError::Network("connection refused".into());
        assert!(err.to_string().contains("connection refused"));
    }

    #[test]
    fn test_api_error_retryable_classification() {
        assert!(ApiError::ServerError { status: 503, body: String::new() }.is_retryable());
        assert!(ApiError::Network("reset".into()).is_retryable());
        assert!(ApiError::Timeout.is_retryable());
        assert!(ApiError::StreamInterrupted("eof".into()).is_retryable());

        assert!(!ApiError::AuthFailed { status: 401, body: String::new() }.is_retryable());
        assert!(!ApiError::RateLimited { retry_after_ms: None, body: String::new() }.is_retryable());
        assert!(!ApiError::InvalidResponse("bad".into()).is_retryable());
    }

    // --- Retry-after extraction ---

    #[test]
    fn test_extract_retry_after_json() {
        let body = r#"{"error":{"message":"Rate limited","metadata":{"retry_after":5}}}"#;
        assert_eq!(extract_retry_after(body), Some(5000));
    }

    #[test]
    fn test_extract_retry_after_missing() {
        let body = r#"{"error":{"message":"Rate limited"}}"#;
        assert_eq!(extract_retry_after(body), None);
    }

    #[test]
    fn test_extract_retry_after_non_json() {
        assert_eq!(extract_retry_after("Rate limited"), None);
    }

    // --- Chat request serialization ---

    #[test]
    fn test_chat_request_serialization() {
        let req = ChatRequest {
            model: "test/model".into(),
            messages: vec![ChatMessage {
                role: "user".into(),
                content: "Hello".into(),
            }],
            stream: true,
            max_tokens: Some(1024),
            temperature: Some(0.5),
        };

        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["model"], "test/model");
        assert_eq!(json["stream"], true);
        assert_eq!(json["max_tokens"], 1024);
        assert_eq!(json["temperature"], 0.5);
        assert_eq!(json["messages"][0]["role"], "user");
        assert_eq!(json["messages"][0]["content"], "Hello");
    }

    #[test]
    fn test_chat_request_omits_none_fields() {
        let req = ChatRequest {
            model: "m".into(),
            messages: vec![],
            stream: false,
            max_tokens: None,
            temperature: None,
        };

        let json = serde_json::to_string(&req).unwrap();
        assert!(!json.contains("max_tokens"));
        assert!(!json.contains("temperature"));
    }

    // --- Shared client ---

    #[test]
    fn test_shared_client_returns_same_instance() {
        let a = shared_client() as *const reqwest::Client;
        let b = shared_client() as *const reqwest::Client;
        assert_eq!(a, b, "shared_client should return the same instance");
    }

    // --- Token usage ---

    #[test]
    fn test_token_usage_default() {
        let usage = TokenUsage::default();
        assert_eq!(usage.prompt_tokens, 0);
        assert_eq!(usage.completion_tokens, 0);
        assert_eq!(usage.total_tokens, 0);
    }
}
