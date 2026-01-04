//! OpenRouter API client with SSE streaming
//!
//! Uses Chat Completions API with Server-Sent Events for streaming.

use anyhow::{Context, Result};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::models::Model;

use crate::project::Project;
use crate::prompt::SystemPrompt;

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
    let mut rx = stream_completion(api_key, model, prompt).await?;
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
    let client = reqwest::Client::new();
    client
        .get("https://openrouter.ai/api/v1/models")
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .context("Failed to connect to OpenRouter")?;
    Ok(())
}

/// Fetch models list from OpenRouter
pub async fn fetch_models(api_key: &str) -> Result<Vec<Model>> {
    let client = reqwest::Client::new();

    let response = client
        .get(OPENROUTER_MODELS_URL)
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await
        .context("Failed to fetch models")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("API error {}: {}", status, body);
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
        max_tokens: Some(4096),
        temperature: Some(0.7),
    };

    let client = reqwest::Client::new();
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
}

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
                let err_str = e.to_string();
                // Don't retry on auth errors or rate limits (handled elsewhere)
                if err_str.contains("401") || err_str.contains("403") || err_str.contains("429") {
                    return Err(e);
                }
                // Retry on network/timeout errors
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
        .header("HTTP-Referer", "https://github.com/hyle-org/hyle")
        .header("X-Title", "hyle")
        .timeout(std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .json(request)
        .send()
        .await
        .context("Failed to connect to OpenRouter")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("API error {}: {}", status, body);
    }

    let mut usage = TokenUsage::default();
    let mut bytes_stream = response.bytes_stream();

    // Buffer for incomplete SSE lines
    let mut buffer = String::new();

    while let Some(chunk) = bytes_stream.next().await {
        let chunk = chunk.context("Stream read error")?;
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
            }
        }
    }

    Ok(usage)
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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_parse_models_response() {
        let json = r#"{"data":[{"id":"test/model","context_length":8192,"pricing":{"prompt":"0","completion":"0"}}]}"#;
        let resp: ModelsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.data.len(), 1);
        assert_eq!(resp.data[0].id, "test/model");
    }
}
