//! hyle-api: HTTP server for sketch submission and job orchestration
//!
//! Accepts sketch submissions, queues builds, returns live URLs.
//! Run with: HYLE_API_KEY=... cargo run --bin hyle-api

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
    sync::Arc,
};
use tokio::{
    process::Command,
    sync::RwLock,
};
use tower_http::cors::{Any, CorsLayer};
use uuid::Uuid;

/// Job status
#[derive(Debug, Clone, Serialize)]
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
    created_at: chrono::DateTime<chrono::Utc>,
}

/// Application state
struct AppState {
    jobs: RwLock<HashMap<String, Job>>,
    projects_dir: PathBuf,
    hyle_binary: PathBuf,
    api_key: Option<String>,
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
}

/// Health check
async fn health() -> &'static str {
    "ok"
}

/// Submit a new sketch
async fn submit_sketch(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SubmitRequest>,
) -> Result<Json<SubmitResponse>, (StatusCode, String)> {
    let sketch = req.sketch.trim();
    if sketch.len() < 20 {
        return Err((StatusCode::BAD_REQUEST, "Sketch too short".into()));
    }

    // Generate job ID and project name
    let job_id = Uuid::new_v4().to_string();
    let project_name = generate_project_name(sketch);

    let job = Job {
        id: job_id.clone(),
        status: JobStatus::Queued,
        sketch: sketch.to_string(),
        project_name: Some(project_name.clone()),
        url: None,
        error: None,
        created_at: chrono::Utc::now(),
    };

    // Store job
    {
        let mut jobs = state.jobs.write().await;
        jobs.insert(job_id.clone(), job);
    }

    // Spawn build task
    let state_clone = state.clone();
    let job_id_clone = job_id.clone();
    tokio::spawn(async move {
        run_build(state_clone, job_id_clone).await;
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
) -> Result<Json<JobResponse>, StatusCode> {
    let jobs = state.jobs.read().await;
    let job = jobs.get(&job_id).ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(JobResponse {
        status: match job.status {
            JobStatus::Queued => "queued".into(),
            JobStatus::Building => "building".into(),
            JobStatus::Deploying => "deploying".into(),
            JobStatus::Live => "live".into(),
            JobStatus::Failed => "failed".into(),
        },
        url: job.url.clone(),
        error: job.error.clone(),
    }))
}

/// Generate a project name from sketch
fn generate_project_name(sketch: &str) -> String {
    // Extract first significant word
    let words: Vec<&str> = sketch
        .split_whitespace()
        .filter(|w| w.len() > 3)
        .take(2)
        .collect();

    let base = if words.is_empty() {
        "project"
    } else {
        words[0]
    };

    // Sanitize
    let sanitized: String = base
        .chars()
        .filter(|c| c.is_alphanumeric())
        .take(12)
        .collect();

    let name = if sanitized.is_empty() {
        "project".to_string()
    } else {
        sanitized.to_lowercase()
    };

    // Add short random suffix
    format!("{}-{}", name, &Uuid::new_v4().to_string()[..4])
}

/// Run the build process
async fn run_build(state: Arc<AppState>, job_id: String) {
    // Update status to building
    {
        let mut jobs = state.jobs.write().await;
        if let Some(job) = jobs.get_mut(&job_id) {
            job.status = JobStatus::Building;
        }
    }

    let (sketch, project_name) = {
        let jobs = state.jobs.read().await;
        let job = jobs.get(&job_id).unwrap();
        (job.sketch.clone(), job.project_name.clone().unwrap_or_default())
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

    // Run hyle
    let result = Command::new(&state.hyle_binary)
        .arg("--trust")
        .arg(&sketch_file)
        .current_dir(&project_dir)
        .env("HYLE_MODEL", env::var("HYLE_MODEL").unwrap_or_else(|_| "meta-llama/llama-3.1-8b-instruct:free".into()))
        .env("OPENROUTER_API_KEY", state.api_key.clone().unwrap_or_default())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await;

    match result {
        Ok(output) => {
            if output.status.success() {
                // Update status to deploying
                {
                    let mut jobs = state.jobs.write().await;
                    if let Some(job) = jobs.get_mut(&job_id) {
                        job.status = JobStatus::Deploying;
                    }
                }

                // Check if index.html was created
                let index_path = project_dir.join("index.html");
                if index_path.exists() {
                    // Build succeeded - set live URL
                    let url = format!("https://{}.hyperstitious.org", project_name);
                    let mut jobs = state.jobs.write().await;
                    if let Some(job) = jobs.get_mut(&job_id) {
                        job.status = JobStatus::Live;
                        job.url = Some(url);
                    }
                } else {
                    update_job_error(&state, &job_id, "Build completed but no index.html created").await;
                }
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                update_job_error(&state, &job_id, &format!("Build failed: {}", stderr)).await;
            }
        }
        Err(e) => {
            update_job_error(&state, &job_id, &format!("Failed to run hyle: {}", e)).await;
        }
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
    // Configuration from environment
    let port: u16 = env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);

    let projects_dir = PathBuf::from(
        env::var("HYLE_PROJECTS_DIR").unwrap_or_else(|_| "/var/www/drops".into())
    );

    let hyle_binary = PathBuf::from(
        env::var("HYLE_BINARY").unwrap_or_else(|_| "/usr/local/bin/hyle".into())
    );

    let api_key = env::var("OPENROUTER_API_KEY").ok();

    let state = Arc::new(AppState {
        jobs: RwLock::new(HashMap::new()),
        projects_dir,
        hyle_binary,
        api_key,
    });

    // CORS for cross-origin requests from hyle.lol
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST])
        .allow_headers([header::CONTENT_TYPE]);

    let app = Router::new()
        .route("/health", get(health))
        .route("/api/sketch", post(submit_sketch))
        .route("/api/jobs/:job_id", get(get_job))
        .layer(cors)
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
    eprintln!("hyle-api listening on port {}", port);

    axum::serve(listener, app).await?;
    Ok(())
}
