//! Orchestrator HTTP Server
//!
//! Web interface for submitting project sketches and monitoring builds.
//! Dispatches hyle instances to autonomously build projects.

use anyhow::Result;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::intake::INTAKE_HTML;
use crate::orchestrator::{
    build_dispatch_prompt, dispatch_hyle, generate_nginx_config, generate_systemd_service,
    scaffold_project, Orchestrator, Project, ProjectStatus,
};

/// Shared orchestrator state
pub struct OrchestratorState {
    pub orchestrator: Orchestrator,
    pub domain: String,
}

/// Run the orchestrator server
pub async fn run_orchestrator(port: u16, projects_root: PathBuf, domain: String) -> Result<()> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpListener;

    let hyle_binary = std::env::current_exe()?;
    let orchestrator = Orchestrator::new(projects_root.clone(), hyle_binary, domain.clone());

    let state = Arc::new(RwLock::new(OrchestratorState {
        orchestrator,
        domain: domain.clone(),
    }));

    // Bind to all interfaces for external access
    let addr: SocketAddr = ([0, 0, 0, 0], port).into();
    let listener = TcpListener::bind(addr).await?;

    println!("╔════════════════════════════════════════════════════════════╗");
    println!(
        "║  hyle orchestrator listening on http://0.0.0.0:{}         ║",
        port
    );
    println!("╠════════════════════════════════════════════════════════════╣");
    println!("║  Projects root: {}  ", projects_root.display());
    println!("║  Domain: {}  ", domain);
    println!("╠════════════════════════════════════════════════════════════╣");
    println!("║  Endpoints:                                                ║");
    println!("║    GET  /                 - Project intake UI              ║");
    println!("║    GET  /api/projects     - List all projects              ║");
    println!("║    POST /api/projects     - Submit new project             ║");
    println!("║    GET  /api/projects/:id - Get project details            ║");
    println!("╚════════════════════════════════════════════════════════════╝");
    println!("\nPress Ctrl-C to stop\n");

    loop {
        let (mut socket, peer) = listener.accept().await?;
        let state = state.clone();

        tokio::spawn(async move {
            let (reader, mut writer) = socket.split();
            let mut reader = BufReader::new(reader);
            let mut request = String::new();
            let mut content_length = 0usize;

            // Read request line
            if reader.read_line(&mut request).await.is_err() {
                return;
            }

            // Read headers
            const MAX_BODY_SIZE: usize = 50 * 1024 * 1024; // 50MB for large sketches
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
                        if content_length > MAX_BODY_SIZE {
                            let _ = writer
                                .write_all(b"HTTP/1.1 413 Payload Too Large\r\n\r\n")
                                .await;
                            return;
                        }
                    }
                }
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
                ("GET", "/") => html_response(INTAKE_HTML),
                ("GET", "/api/projects") => handle_list_projects(&state).await,
                ("POST", "/api/projects") => handle_create_project(&state, &body).await,
                ("OPTIONS", _) => cors_preflight(),
                (_, p) if p.starts_with("/api/projects/") => {
                    let id = p.trim_start_matches("/api/projects/");
                    if !id
                        .chars()
                        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
                    {
                        json_response(400, r#"{"error": "Invalid project ID"}"#)
                    } else {
                        handle_get_project(&state, id).await
                    }
                }
                _ => json_response(404, r#"{"error": "Not found"}"#),
            };

            let _ = writer.write_all(response.as_bytes()).await;
        });
    }
}

fn html_response(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\n\
        Content-Type: text/html; charset=utf-8\r\n\
        Content-Length: {}\r\n\
        Access-Control-Allow-Origin: *\r\n\
        \r\n\
        {}",
        body.len(),
        body
    )
}

fn json_response(status: u16, body: &str) -> String {
    let status_text = match status {
        200 => "OK",
        201 => "Created",
        400 => "Bad Request",
        404 => "Not Found",
        500 => "Internal Server Error",
        _ => "Unknown",
    };

    format!(
        "HTTP/1.1 {} {}\r\n\
        Content-Type: application/json\r\n\
        Content-Length: {}\r\n\
        Access-Control-Allow-Origin: *\r\n\
        Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n\
        Access-Control-Allow-Headers: Content-Type\r\n\
        \r\n\
        {}",
        status,
        status_text,
        body.len(),
        body
    )
}

fn cors_preflight() -> String {
    "HTTP/1.1 204 No Content\r\n\
    Access-Control-Allow-Origin: *\r\n\
    Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n\
    Access-Control-Allow-Headers: Content-Type\r\n\
    \r\n"
        .to_string()
}

async fn handle_list_projects(state: &Arc<RwLock<OrchestratorState>>) -> String {
    let state = state.read().await;
    let projects: Vec<&Project> = state.orchestrator.list_projects();

    let json = serde_json::json!({
        "projects": projects,
    });

    json_response(200, &json.to_string())
}

async fn handle_get_project(state: &Arc<RwLock<OrchestratorState>>, id: &str) -> String {
    let state = state.read().await;

    match state.orchestrator.get_project(id) {
        Some(project) => json_response(200, &serde_json::to_string(project).unwrap()),
        None => json_response(404, r#"{"error": "Project not found"}"#),
    }
}

async fn handle_create_project(state: &Arc<RwLock<OrchestratorState>>, body: &str) -> String {
    #[derive(serde::Deserialize)]
    struct CreateRequest {
        sketch: String,
    }

    let req: CreateRequest = match serde_json::from_str(body) {
        Ok(r) => r,
        Err(e) => return json_response(400, &format!(r#"{{"error": "Invalid JSON: {}"}}"#, e)),
    };

    if req.sketch.len() < 50 {
        return json_response(400, r#"{"error": "Sketch too short (min 50 chars)"}"#);
    }

    let mut state = state.write().await;

    // Extract values we need before getting mutable project reference
    let hyle_binary = state.orchestrator.hyle_binary.clone();
    let domain = state.domain.clone();
    let projects_root = state.orchestrator.projects_root.clone();

    // Submit project to orchestrator
    let project_id = match state.orchestrator.submit_project(&req.sketch) {
        Ok(id) => id,
        Err(e) => return json_response(500, &format!(r#"{{"error": "{}"}}"#, e)),
    };

    // Get project and start building
    let project = state.orchestrator.projects.get_mut(&project_id).unwrap();
    project.status = ProjectStatus::Scaffolding;

    // Clone project for scaffolding (which doesn't mutate)
    let project_clone = project.clone();

    // Scaffold project synchronously (fast)
    // INVARIANT: projects_root must exist and project_dir must be under it
    if let Err(e) = scaffold_project(&project_clone, &projects_root) {
        project.status = ProjectStatus::Failed;
        project.log.push(crate::orchestrator::ProjectEvent {
            timestamp: chrono::Utc::now(),
            kind: "error".into(),
            message: format!("Scaffolding failed: {}", e),
        });
        return json_response(500, &format!(r#"{{"error": "Scaffolding failed: {}"}}"#, e));
    }

    project.status = ProjectStatus::Building;
    project.log.push(crate::orchestrator::ProjectEvent {
        timestamp: chrono::Utc::now(),
        kind: "scaffold".into(),
        message: "Project scaffolded successfully".into(),
    });

    // Generate deployment configs
    let subdomain_clone = project.spec.subdomain.clone();
    let port = project.spec.port.unwrap_or(3000);
    let project_dir = project.project_dir.clone();

    if let Some(ref subdomain) = subdomain_clone {
        let nginx_conf = generate_nginx_config(subdomain, &domain, port);
        let deploy_dir = project_dir.join("deploy");
        let _ = std::fs::create_dir_all(&deploy_dir);
        let _ = std::fs::write(deploy_dir.join("nginx.conf"), nginx_conf);

        let systemd_conf = generate_systemd_service(&project_clone);
        let _ = std::fs::write(deploy_dir.join("service.unit"), systemd_conf);

        project.log.push(crate::orchestrator::ProjectEvent {
            timestamp: chrono::Utc::now(),
            kind: "deploy".into(),
            message: format!(
                "Generated nginx and systemd configs for {}.{}",
                subdomain, domain
            ),
        });
    }

    // Build dispatch prompt
    let prompt = build_dispatch_prompt(&project_clone);

    // Spawn hyle instance in background
    match dispatch_hyle(&hyle_binary, &project_dir, &prompt) {
        Ok(child) => {
            let pid = child.id();
            project.hyle_pid = Some(pid);
            project.log.push(crate::orchestrator::ProjectEvent {
                timestamp: chrono::Utc::now(),
                kind: "dispatch".into(),
                message: format!("Dispatched hyle instance (PID: {:?})", pid),
            });
        }
        Err(e) => {
            project.status = ProjectStatus::Failed;
            project.log.push(crate::orchestrator::ProjectEvent {
                timestamp: chrono::Utc::now(),
                kind: "error".into(),
                message: format!("Failed to dispatch hyle: {}", e),
            });
        }
    }

    let response = serde_json::json!({
        "success": true,
        "project_id": project_id,
        "status": "building",
    });

    json_response(201, &response.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_response() {
        let resp = json_response(200, r#"{"foo": "bar"}"#);
        assert!(resp.contains("200 OK"));
        assert!(resp.contains("application/json"));
    }

    #[test]
    fn test_html_response() {
        let resp = html_response("<html></html>");
        assert!(resp.contains("200 OK"));
        assert!(resp.contains("text/html"));
    }
}
