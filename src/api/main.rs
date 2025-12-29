//! hyle-api: HTTP server for sketch submission and job orchestration
//!
//! Accepts sketch submissions, queues builds, returns live URLs.
//! Supports multi-model round-robin with automatic fallback on rate limits.
//!
//! Environment variables:
//!   PORT                 - HTTP port (default: 3000)
//!   OPENROUTER_API_KEY   - OpenRouter API key
//!   HYLE_MODELS          - Comma-separated list of models to use
//!   HYLE_PROJECTS_DIR    - Where to create projects (default: /var/www/drops)
//!   HYLE_BINARY          - Path to hyle binary (default: /usr/local/bin/hyle)

use axum::{
    extract::{Path, State},
    http::{header, Method, StatusCode},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    env,
    path::PathBuf,
    process::Stdio,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{process::Command, sync::RwLock, time::timeout};
use tower_http::cors::{Any, CorsLayer};
use uuid::Uuid;

/// Default free models sorted by context length and coding capability
/// Verified against OpenRouter API 2025-12-29
const DEFAULT_MODELS: &[&str] = &[
    "google/gemini-2.0-flash-exp:free",      // 1M ctx - best for large projects
    "qwen/qwen3-coder:free",                 // 262K ctx - coding-optimized
    "mistralai/devstral-2512:free",          // 262K ctx - dev-focused
    "kwaipilot/kat-coder-pro:free",          // 256K ctx - coding-specific
    "meta-llama/llama-3.3-70b-instruct:free", // 131K ctx - large model
    "google/gemma-3-27b-it:free",            // 131K ctx - good quality
    "deepseek/deepseek-r1-0528:free",        // 164K ctx - reasoning model
    "mistralai/mistral-small-3.1-24b-instruct:free", // 128K ctx
];

/// Build timeout per model attempt (5 minutes)
const MODEL_TIMEOUT_SECS: u64 = 300;

/// Delay between model fallback attempts
const FALLBACK_DELAY_MS: u64 = 2000;

/// Job status
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
enum JobStatus {
    Queued,
    Building,
    Deploying,
    Live,
    Failed,
}

/// A build job
#[derive(Debug, Clone, Serialize)]
struct Job {
    id: String,
    status: JobStatus,
    sketch: String,
    project_name: Option<String>,
    url: Option<String>,
    error: Option<String>,
    model_used: Option<String>,
    models_tried: Vec<String>,
    created_at: chrono::DateTime<chrono::Utc>,
}

/// Application state
struct AppState {
    jobs: RwLock<HashMap<String, Job>>,
    projects_dir: PathBuf,
    hyle_binary: PathBuf,
    api_key: Option<String>,
    models: Vec<String>,
    /// Round-robin index for load balancing across models
    model_index: AtomicUsize,
}

impl AppState {
    /// Get next model in round-robin order
    fn next_model(&self) -> &str {
        let idx = self.model_index.fetch_add(1, Ordering::Relaxed) % self.models.len();
        &self.models[idx]
    }

    /// Get all models starting from a random position for better distribution
    fn get_model_rotation(&self) -> Vec<&str> {
        let start = self.model_index.fetch_add(1, Ordering::Relaxed) % self.models.len();
        let mut rotation = Vec::with_capacity(self.models.len());
        for i in 0..self.models.len() {
            rotation.push(self.models[(start + i) % self.models.len()].as_str());
        }
        rotation
    }
}

/// Request to submit a sketch
#[derive(Debug, Deserialize)]
struct SubmitRequest {
    sketch: String,
}

/// Response after submitting
#[derive(Debug, Serialize)]
struct SubmitResponse {
    status: String,
    job_id: String,
    poll_url: String,
}

/// Response for job status
#[derive(Debug, Serialize)]
struct JobResponse {
    status: String,
    url: Option<String>,
    error: Option<String>,
    model_used: Option<String>,
    models_tried: Vec<String>,
}

/// Models list response
#[derive(Debug, Serialize)]
struct ModelsResponse {
    models: Vec<String>,
    active_index: usize,
}

/// Health check
async fn health() -> &'static str {
    "ok"
}

/// List available models
async fn list_models(State(state): State<Arc<AppState>>) -> Json<ModelsResponse> {
    Json(ModelsResponse {
        models: state.models.clone(),
        active_index: state.model_index.load(Ordering::Relaxed) % state.models.len(),
    })
}

/// Submit a new sketch
async fn submit_sketch(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SubmitRequest>,
) -> Result<Json<SubmitResponse>, (StatusCode, String)> {
    let sketch = req.sketch.trim();
    if sketch.len() < 20 {
        return Err((StatusCode::BAD_REQUEST, "Sketch too short (min 20 chars)".into()));
    }

    if state.api_key.is_none() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "API key not configured".into(),
        ));
    }

    let job_id = Uuid::new_v4().to_string();
    let project_name = generate_project_name(sketch);

    let job = Job {
        id: job_id.clone(),
        status: JobStatus::Queued,
        sketch: sketch.to_string(),
        project_name: Some(project_name.clone()),
        url: None,
        error: None,
        model_used: None,
        models_tried: Vec::new(),
        created_at: chrono::Utc::now(),
    };

    {
        let mut jobs = state.jobs.write().await;
        jobs.insert(job_id.clone(), job);
    }

    // Spawn build task
    let state_clone = state.clone();
    let job_id_clone = job_id.clone();
    tokio::spawn(async move {
        run_build_with_fallback(state_clone, job_id_clone).await;
    });

    Ok(Json(SubmitResponse {
        status: "queued".into(),
        job_id: job_id.clone(),
        poll_url: format!("/api/jobs/{}", job_id),
    }))
}

/// Get job status
async fn get_job(
    State(state): State<Arc<AppState>>,
    Path(job_id): Path<String>,
) -> Result<Json<JobResponse>, (StatusCode, Json<JobResponse>)> {
    let jobs = state.jobs.read().await;

    match jobs.get(&job_id) {
        Some(job) => Ok(Json(JobResponse {
            status: match job.status {
                JobStatus::Queued => "queued".into(),
                JobStatus::Building => "building".into(),
                JobStatus::Deploying => "deploying".into(),
                JobStatus::Live => "live".into(),
                JobStatus::Failed => "failed".into(),
            },
            url: job.url.clone(),
            error: job.error.clone(),
            model_used: job.model_used.clone(),
            models_tried: job.models_tried.clone(),
        })),
        None => {
            eprintln!("[{}] Job not found (may have completed and expired)", job_id);
            Err((StatusCode::NOT_FOUND, Json(JobResponse {
                status: "not_found".into(),
                url: None,
                error: Some(format!("Job {} not found - may have completed or expired", job_id)),
                model_used: None,
                models_tried: vec![],
            })))
        }
    }
}

/// Generate a project name from sketch
fn generate_project_name(sketch: &str) -> String {
    let words: Vec<&str> = sketch
        .split_whitespace()
        .filter(|w| w.len() > 3 && w.chars().all(|c| c.is_alphanumeric()))
        .take(2)
        .collect();

    let base = words.first().copied().unwrap_or("project");

    let sanitized: String = base
        .chars()
        .filter(|c| c.is_alphanumeric())
        .take(12)
        .collect::<String>()
        .to_lowercase();

    let name = if sanitized.is_empty() {
        "project".to_string()
    } else {
        sanitized
    };

    format!("{}-{}", name, &Uuid::new_v4().to_string()[..4])
}

/// Run build with multi-model fallback
async fn run_build_with_fallback(state: Arc<AppState>, job_id: String) {
    // Update status to building
    {
        let mut jobs = state.jobs.write().await;
        if let Some(job) = jobs.get_mut(&job_id) {
            job.status = JobStatus::Building;
        }
    }

    let (sketch, project_name) = {
        let jobs = state.jobs.read().await;
        match jobs.get(&job_id) {
            Some(job) => (job.sketch.clone(), job.project_name.clone().unwrap_or_default()),
            None => return,
        }
    };

    // Create project directory
    let project_dir = state.projects_dir.join(&project_name);
    if let Err(e) = tokio::fs::create_dir_all(&project_dir).await {
        update_job_error(&state, &job_id, &format!("Failed to create dir: {}", e)).await;
        return;
    }

    // Write sketch file
    let sketch_file = project_dir.join("sketch.md");
    if let Err(e) = tokio::fs::write(&sketch_file, &sketch).await {
        update_job_error(&state, &job_id, &format!("Failed to write sketch: {}", e)).await;
        return;
    }

    // Try each model in rotation
    let models = state.get_model_rotation();
    let mut last_error = String::new();

    for model in &models {
        // Record that we tried this model
        {
            let mut jobs = state.jobs.write().await;
            if let Some(job) = jobs.get_mut(&job_id) {
                job.models_tried.push(model.to_string());
            }
        }

        eprintln!("[{}] Trying model: {}", job_id, model);

        match try_build_with_model(&state, &project_dir, &sketch_file, model).await {
            Ok(()) => {
                // Check if index.html was created
                let index_path = project_dir.join("index.html");
                if index_path.exists() {
                    // Success! Use HTTP until wildcard SSL is set up
                    let url = format!("http://{}.hyperstitious.org", project_name);
                    {
                        let mut jobs = state.jobs.write().await;
                        if let Some(job) = jobs.get_mut(&job_id) {
                            job.status = JobStatus::Live;
                            job.url = Some(url);
                            job.model_used = Some(model.to_string());
                        }
                    }
                    eprintln!("[{}] Success with model: {}", job_id, model);
                    return;
                }
                last_error = "Build completed but no index.html created".to_string();
            }
            Err(e) => {
                last_error = e;
                eprintln!("[{}] Model {} failed: {}", job_id, model, last_error);

                // Check if it's a rate limit error - if so, try next model
                if last_error.contains("429")
                    || last_error.contains("rate")
                    || last_error.contains("throttl")
                    || last_error.contains("limit")
                {
                    eprintln!("[{}] Rate limited, trying next model...", job_id);
                    tokio::time::sleep(Duration::from_millis(FALLBACK_DELAY_MS)).await;
                    continue;
                }

                // For other errors, still try next model but with delay
                tokio::time::sleep(Duration::from_millis(FALLBACK_DELAY_MS)).await;
            }
        }
    }

    // All models exhausted
    update_job_error(
        &state,
        &job_id,
        &format!(
            "All {} models failed. Last error: {}",
            models.len(),
            last_error
        ),
    )
    .await;
}

/// Try to build with a specific model
async fn try_build_with_model(
    state: &Arc<AppState>,
    project_dir: &PathBuf,
    sketch_file: &PathBuf,
    model: &str,
) -> Result<(), String> {
    // Read sketch content for --task mode
    let sketch_content = match tokio::fs::read_to_string(sketch_file).await {
        Ok(content) => content,
        Err(e) => return Err(format!("Failed to read sketch: {}", e)),
    };

    // Wrap sketch with the hyle philosophy: internet artpieces
    let task_prompt = format!(
        r#"You are creating an INTERNET ARTPIECE — a self-contained, interactive browser experience.

This is NOT a static webpage. This is something people open in their browser and INTERACT with.
Think: generative art, data visualizations, audio toys, interactive fiction, creative tools.

Requirements:
- Single index.html file (all CSS/JS inline or embedded)
- Responsive: works on any screen size
- Smooth: 60fps animations, no jank
- Dynamic: responds to user input (mouse, touch, keyboard)
- Self-contained: no external dependencies, no build step
- Delightful: surprising, playful, aesthetically considered

The sketch describes what to build:
---
{}
---

Use the write() tool to create index.html with the complete artpiece.
IMPORTANT: write(path="index.html", content="<!DOCTYPE html>...")

Make it something people want to share. Make it memorable."#,
        sketch_content
    );

    let result = timeout(
        Duration::from_secs(MODEL_TIMEOUT_SECS),
        Command::new(&state.hyle_binary)
            .arg("--task")  // Headless mode - no TTY required
            .arg(&task_prompt)
            .arg("--trust")
            .current_dir(project_dir)
            .env("HYLE_MODEL", model)
            .env(
                "OPENROUTER_API_KEY",
                state.api_key.as_deref().unwrap_or(""),
            )
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output(),
    )
    .await;

    match result {
        Ok(Ok(output)) => {
            if output.status.success() {
                Ok(())
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                Err(format!("Exit {}: {}", output.status, stderr.trim()))
            }
        }
        Ok(Err(e)) => Err(format!("Failed to execute hyle: {}", e)),
        Err(_) => Err(format!("Timeout after {}s", MODEL_TIMEOUT_SECS)),
    }
}

async fn update_job_error(state: &Arc<AppState>, job_id: &str, error: &str) {
    let mut jobs = state.jobs.write().await;
    if let Some(job) = jobs.get_mut(job_id) {
        job.status = JobStatus::Failed;
        job.error = Some(error.to_string());
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let port: u16 = env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);

    let projects_dir =
        PathBuf::from(env::var("HYLE_PROJECTS_DIR").unwrap_or_else(|_| "/var/www/drops".into()));

    let hyle_binary =
        PathBuf::from(env::var("HYLE_BINARY").unwrap_or_else(|_| "/usr/local/bin/hyle".into()));

    let api_key = env::var("OPENROUTER_API_KEY").ok();

    // Load models from env or use defaults
    let models: Vec<String> = env::var("HYLE_MODELS")
        .map(|s| s.split(',').map(|m| m.trim().to_string()).collect())
        .unwrap_or_else(|_| DEFAULT_MODELS.iter().map(|s| s.to_string()).collect());

    eprintln!("hyle-api starting...");
    eprintln!("  Port: {}", port);
    eprintln!("  Projects dir: {}", projects_dir.display());
    eprintln!("  Hyle binary: {}", hyle_binary.display());
    eprintln!("  API key: {}", if api_key.is_some() { "set" } else { "NOT SET" });
    eprintln!("  Models ({}): {:?}", models.len(), models);

    let state = Arc::new(AppState {
        jobs: RwLock::new(HashMap::new()),
        projects_dir,
        hyle_binary,
        api_key,
        models,
        model_index: AtomicUsize::new(0),
    });

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION]);

    let app = Router::new()
        .route("/health", get(health))
        .route("/api/models", get(list_models))
        .route("/api/sketch", post(submit_sketch))
        .route("/api/jobs/:job_id", get(get_job))
        .layer(cors)
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
    eprintln!("hyle-api listening on http://0.0.0.0:{}", port);

    axum::serve(listener, app).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_project_name() {
        let name = generate_project_name("build a simple calculator app");
        assert!(name.starts_with("build-") || name.starts_with("simple-"));
        assert!(name.len() < 20);
    }

    #[test]
    fn test_generate_project_name_sanitizes() {
        let name = generate_project_name("foo/bar/../baz evil");
        assert!(!name.contains('/'));
        assert!(!name.contains('.'));
    }

    #[test]
    fn test_generate_project_name_handles_empty() {
        let name = generate_project_name("a b c");
        assert!(name.starts_with("project-"));
    }
}
