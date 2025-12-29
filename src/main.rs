// Allow dead code during rapid development - clean up before v1.0
#![allow(dead_code)]
#![allow(unused_imports)]
#![allow(unused_variables)]

//! hyle - Rust-native code assistant
//!
//! USAGE:
//!   hyle --free [PATHS...]        # choose free model, interactive loop
//!   hyle --model <id> [PATHS...]  # specific model
//!   hyle --task "..." [PATHS...]  # one-shot: produce diff, ask apply
//!   hyle doctor                   # check config, key, network
//!   hyle models --refresh         # refresh models cache
//!   hyle config set key <value>   # non-interactive config

mod config;
mod models;
mod client;
mod telemetry;
mod traces;
mod skills;
mod session;
mod ui;
mod tools;
mod backburner;
mod agent;
mod git;
mod eval;
mod project;
mod bootstrap;
mod intent;
mod prompt;
mod prompts;
mod tmux;
mod cognitive;
mod docs;
mod environ;
mod github;
mod server;
mod orchestrator;
mod orchestrator_server;
mod intake;

use anyhow::{Context, Result};
use std::path::PathBuf;

// ═══════════════════════════════════════════════════════════════
// CLI
// ═══════════════════════════════════════════════════════════════

#[derive(Debug)]
enum Command {
    Interactive {
        free_only: bool,
        model: Option<String>,
        paths: Vec<PathBuf>,
        resume: bool,
    },
    Task {
        task: String,
        paths: Vec<PathBuf>,
    },
    Backburner {
        paths: Vec<PathBuf>,
        watch_docs: bool,
    },
    Server {
        port: u16,
    },
    Orchestrate {
        port: u16,
        projects_root: PathBuf,
        domain: String,
    },
    Doctor,
    Models {
        refresh: bool,
    },
    ConfigSet {
        key: String,
        value: String,
    },
    Sessions {
        list: bool,
        clean: bool,
    },
    Help,
}

fn parse_args() -> Command {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.is_empty() {
        return Command::Interactive {
            free_only: false,
            model: None,
            paths: vec![],
            resume: true, // Default: resume last session
        };
    }

    if args.iter().any(|a| a == "--help" || a == "-h") {
        return Command::Help;
    }

    if args.first().map(|s| s.as_str()) == Some("doctor") {
        return Command::Doctor;
    }

    if args.first().map(|s| s.as_str()) == Some("models") {
        return Command::Models {
            refresh: args.iter().any(|a| a == "--refresh"),
        };
    }

    if args.first().map(|s| s.as_str()) == Some("sessions") {
        return Command::Sessions {
            list: args.iter().any(|a| a == "--list" || a == "-l"),
            clean: args.iter().any(|a| a == "--clean"),
        };
    }

    if args.first().map(|s| s.as_str()) == Some("config")
        && args.get(1).map(|s| s.as_str()) == Some("set") {
            return Command::ConfigSet {
                key: args.get(2).cloned().unwrap_or_default(),
                value: args.get(3).cloned().unwrap_or_default(),
            };
        }

    // Check for --backburner flag
    if args.iter().any(|a| a == "--backburner" || a == "-b") {
        let watch_docs = args.iter().any(|a| a == "--watch-docs");
        let paths: Vec<PathBuf> = args.iter()
            .filter(|a| !a.starts_with('-'))
            .map(PathBuf::from)
            .collect();
        return Command::Backburner { paths, watch_docs };
    }

    // Check for --serve flag
    if args.iter().any(|a| a == "--serve" || a == "-s") {
        let port = args.iter()
            .position(|a| a == "--serve" || a == "-s")
            .and_then(|i| args.get(i + 1))
            .and_then(|p| p.parse().ok())
            .unwrap_or(8420);
        return Command::Server { port };
    }

    // Check for orchestrate command
    if args.first().map(|s| s.as_str()) == Some("orchestrate") {
        let port = args.iter()
            .position(|a| a == "--port" || a == "-p")
            .and_then(|i| args.get(i + 1))
            .and_then(|p| p.parse().ok())
            .unwrap_or(8421);

        let projects_root = args.iter()
            .position(|a| a == "--root" || a == "-r")
            .and_then(|i| args.get(i + 1))
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                dirs::home_dir()
                    .map(|h| h.join("projects"))
                    .unwrap_or_else(|| PathBuf::from("/tmp/hyle-projects"))
            });

        let domain = args.iter()
            .position(|a| a == "--domain" || a == "-d")
            .and_then(|i| args.get(i + 1))
            .cloned()
            .unwrap_or_else(|| "hyperstitious.org".into());

        return Command::Orchestrate { port, projects_root, domain };
    }

    // Parse flags and paths
    let mut free_only = false;
    let mut model = None;
    let mut task = None;
    let mut paths = Vec::new();
    let mut resume = true;
    let mut trust_mode = false;
    let mut ask_mode = false;
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "--free" | "-f" => free_only = true,
            "--new" | "-n" => resume = false,
            "--trust" | "-y" => trust_mode = true,
            "--ask" | "-a" => ask_mode = true,
            "--model" | "-m" => {
                i += 1;
                model = args.get(i).cloned();
            }
            "--task" | "-t" => {
                i += 1;
                task = args.get(i).cloned();
            }
            s if !s.starts_with('-') => {
                paths.push(PathBuf::from(s));
            }
            _ => {}
        }
        i += 1;
    }

    // Apply permission modes to config
    if trust_mode || ask_mode {
        if let Ok(mut cfg) = config::Config::load() {
            if trust_mode {
                cfg.trust_mode = true;
            }
            if ask_mode {
                cfg.permissions = config::Permissions::restrictive();
            }
            // Save temporarily for this session
            let _ = cfg.save();
        }
    }

    if let Some(task_str) = task {
        Command::Task { task: task_str, paths }
    } else {
        Command::Interactive { free_only, model, paths, resume }
    }
}

fn print_help() {
    println!(r#"hyle - Rust-native code assistant (OpenRouter powered)

USAGE:
    hyle                          # resume last session (or start new)
    hyle --free [PATHS...]        # choose free model, interactive loop
    hyle --new                    # start fresh session
    hyle --model <id> [PATHS...]  # use specific model
    hyle --task "..." [PATHS...]  # autonomous agent mode (no TUI)
    hyle --backburner [PATHS...]  # background maintenance daemon
    hyle --serve [PORT]           # HTTP API server (default: 8420)
    hyle orchestrate              # project orchestrator (default: 8421)
    hyle doctor                   # check config, key, network
    hyle models --refresh         # refresh models cache
    hyle sessions --list          # list saved sessions
    hyle sessions --clean         # clean old sessions
    hyle config set key <value>   # set config value

FLAGS:
    -f, --free              Only show free models in picker
    -n, --new               Start new session (don't resume)
    -m, --model <id>        Use specific model ID
    -t, --task <text>       One-shot task mode
    -b, --backburner        Run background maintenance daemon
    -s, --serve [port]      HTTP API server mode
    orchestrate             Project orchestrator mode
        -p, --port <port>   Orchestrator port (default: 8421)
        -r, --root <path>   Projects root directory
        -d, --domain <dom>  Domain for subdomains (default: hyperstitious.org)
    -y, --trust             Trust mode: auto-approve all tool operations
    -a, --ask               Ask mode: confirm before write/execute/git ops
    -h, --help              Show this help

CONFIG:
    ~/.config/hyle/config.json    API key, preferences
    ~/.cache/hyle/models.json     Cached model list
    ~/.local/state/hyle/sessions/ Session history

ENVIRONMENT:
    OPENROUTER_API_KEY              Override API key from config

CONTROLS (interactive mode):
    Enter      Send prompt
    Up/Down    Browse prompt history
    PageUp/Dn  Scroll conversation
    End        Jump to bottom (auto-scroll)
    Tab        Switch tabs
    k          Kill current operation
    t          Throttle mode
    f          Full speed mode
    n          Normal mode
    Esc        Quit

BACKBURNER MODE:
    Runs slow, non-intrusive maintenance tasks:
    - Git garbage collection and integrity checks
    - Session cleanup (keeps last 10)
    - Dependency audit suggestions
    - Code quality hints
    Send SIGINT/SIGTERM to stop gracefully.
"#);
}

// ═══════════════════════════════════════════════════════════════
// MAIN
// ═══════════════════════════════════════════════════════════════

#[tokio::main]
async fn main() -> Result<()> {
    // Set up tmux integration
    let work_dir = std::env::current_dir().unwrap_or_default();
    tmux::setup(&work_dir);

    // Ensure cleanup on exit
    let result = run_command().await;

    // Clean up tmux on exit
    tmux::cleanup();

    result
}

async fn run_command() -> Result<()> {
    match parse_args() {
        Command::Help => {
            print_help();
            Ok(())
        }
        Command::Doctor => {
            run_doctor().await
        }
        Command::Models { refresh } => {
            run_models(refresh).await
        }
        Command::Sessions { list, clean } => {
            run_sessions(list, clean)
        }
        Command::ConfigSet { key, value } => {
            run_config_set(&key, &value)
        }
        Command::Task { task, paths } => {
            tmux::set_status("task");
            let result = run_task(&task, &paths).await;
            tmux::task_complete("Task", result.is_ok());
            result
        }
        Command::Backburner { paths, watch_docs } => {
            tmux::set_status(if watch_docs { "docs" } else { "bg" });
            run_backburner(&paths, watch_docs).await
        }
        Command::Server { port } => {
            tmux::set_status("serve");
            server::run_server(port).await
        }
        Command::Orchestrate { port, projects_root, domain } => {
            tmux::set_status("orch");
            orchestrator_server::run_orchestrator(port, projects_root, domain).await
        }
        Command::Interactive { free_only, model, paths, resume } => {
            run_interactive(free_only, model, paths, resume).await
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// COMMANDS
// ═══════════════════════════════════════════════════════════════

async fn run_doctor() -> Result<()> {
    println!("hyle doctor\n");

    // Check config
    let cfg = config::Config::load()?;
    println!("[{}] Config: {}",
        if cfg.api_key.is_some() { "✓" } else { "✗" },
        config::config_path()?.display()
    );

    // Check API key
    let has_key = cfg.api_key.is_some() || std::env::var("OPENROUTER_API_KEY").is_ok();
    println!("[{}] API key: {}",
        if has_key { "✓" } else { "✗" },
        if has_key { "configured" } else { "missing" }
    );

    // Check models cache
    let models_path = config::cache_dir()?.join("models.json");
    let models_ok = models_path.exists();
    println!("[{}] Models cache: {}",
        if models_ok { "✓" } else { "✗" },
        models_path.display()
    );

    // Check tmux
    let in_tmux = tmux::is_tmux();
    let width = tmux::term_width();
    let wide = tmux::is_wide();
    println!("[{}] Tmux: {} ({}cols, {})",
        if in_tmux { "✓" } else { "○" },
        if in_tmux { "detected" } else { "not in tmux" },
        width,
        if wide { "wide layout available" } else { "narrow" }
    );

    // Check project
    let cwd = std::env::current_dir()?;
    if let Some(p) = project::Project::detect(&cwd) {
        println!("[✓] Project: {} ({:?}, {} files, {} lines)",
            p.name, p.project_type, p.files.len(), p.total_lines());
    } else {
        println!("[○] Project: not detected");
    }

    // Check network
    print!("[?] Network: checking...");
    match client::check_connectivity().await {
        Ok(()) => println!("\r[✓] Network: connected         "),
        Err(e) => println!("\r[✗] Network: {}", e),
    }

    Ok(())
}

async fn run_models(refresh: bool) -> Result<()> {
    let api_key = config::get_api_key()?;

    if refresh {
        println!("Fetching models from OpenRouter...");
        let models = client::fetch_models(&api_key).await?;
        models::save_cache(&models)?;
        println!("Cached {} models", models.len());
    }

    let models = models::load_or_fetch(&api_key).await?;
    let free: Vec<_> = models.iter().filter(|m| m.is_free()).collect();

    println!("\nFree models ({}):", free.len());
    for m in free.iter().take(20) {
        println!("  {} ({}k ctx)", m.id, m.context_length / 1000);
    }
    if free.len() > 20 {
        println!("  ... and {} more", free.len() - 20);
    }

    Ok(())
}

fn run_sessions(_list: bool, clean: bool) -> Result<()> {
    if clean {
        let removed = session::cleanup_sessions(10)?;
        println!("Cleaned up {} old sessions", removed);
        return Ok(());
    }

    // Default: list sessions
    let sessions = session::list_sessions()?;
    if sessions.is_empty() {
        println!("No sessions found");
        return Ok(());
    }

    println!("Sessions ({}):\n", sessions.len());
    for s in sessions.iter().take(10) {
        let age = chrono::Utc::now() - s.updated_at;
        let age_str = if age.num_hours() < 1 {
            format!("{}m ago", age.num_minutes())
        } else if age.num_days() < 1 {
            format!("{}h ago", age.num_hours())
        } else {
            format!("{}d ago", age.num_days())
        };

        println!("  {} | {} | {} msgs | {} tokens | {}",
            s.id,
            s.model.split('/').next_back().unwrap_or(&s.model),
            s.message_count,
            s.total_tokens,
            age_str,
        );
    }

    if sessions.len() > 10 {
        println!("  ... and {} more", sessions.len() - 10);
    }

    Ok(())
}

fn run_config_set(key: &str, value: &str) -> Result<()> {
    let mut cfg = config::Config::load()?;

    match key {
        "key" | "api_key" | "openrouter.key" => {
            cfg.api_key = Some(value.to_string());
            cfg.save()?;
            println!("API key saved to {}", config::config_path()?.display());
        }
        "model" => {
            cfg.default_model = Some(value.to_string());
            cfg.save()?;
            println!("Default model set to: {}", value);
        }
        _ => {
            anyhow::bail!("Unknown config key: {}. Valid keys: key, model", key);
        }
    }
    Ok(())
}

async fn run_task(task: &str, paths: &[PathBuf]) -> Result<()> {
    use agent::{AgentCore, AgentEvent};
    use std::io::Write;

    let api_key = config::get_api_key()?;
    let cfg = config::Config::load()?;

    // Get model - prefer HYLE_MODEL env var, then config, then default
    let model = std::env::var("HYLE_MODEL")
        .ok()
        .or(cfg.default_model.clone())
        .unwrap_or_else(|| "meta-llama/llama-3.2-3b-instruct:free".to_string());

    let work_dir = std::env::current_dir()?;

    println!("Task: {}", task);
    println!("Model: {}", model);
    println!("Mode: Agent (autonomous tool execution)");
    if !paths.is_empty() {
        println!("Paths: {:?}", paths);
    }
    println!();

    // Read file contents if paths provided
    let mut context = String::new();
    for path in paths {
        if path.exists() {
            let content = std::fs::read_to_string(path)
                .with_context(|| format!("Failed to read {}", path.display()))?;
            context.push_str(&format!("\n--- {} ---\n{}\n", path.display(), content));
        }
    }

    // Build prompt
    let prompt = if context.is_empty() {
        task.to_string()
    } else {
        format!("Given these files:\n{}\n\nTask: {}", context, task)
    };

    // Run agent with event printing
    let agent = AgentCore::new(&api_key, &model, &work_dir);

    let result = agent.run_with_callback(&prompt, |event| {
        match event {
            AgentEvent::Token(t) => {
                print!("{}", t);
                let _ = std::io::stdout().flush();
            }
            AgentEvent::Status(s) => {
                println!("\n[{}]", s);
            }
            AgentEvent::ToolExecuting { name, args: _ } => {
                println!("\n  → {}", name);
            }
            AgentEvent::ToolResult { name, success, output } => {
                let icon = if *success { "✓" } else { "✗" };
                println!("  {} {}", icon, name);
                // Show first few lines of output
                for line in output.lines().take(3) {
                    println!("    {}", line);
                }
                if output.lines().count() > 3 {
                    println!("    ...");
                }
            }
            AgentEvent::IterationComplete { iteration, tool_count } => {
                println!("\n─── Iteration {} ({} tools) ───\n", iteration, tool_count);
            }
            AgentEvent::Complete { iterations, success } => {
                let status = if *success { "completed" } else { "stopped" };
                println!("\n\n[Agent {} after {} iterations]", status, iterations);
            }
            AgentEvent::Error(e) => {
                eprintln!("\n[Error: {}]", e);
            }
            AgentEvent::ToolCallsParsed(_) => {}
        }
    }).await;

    if result.success {
        println!("\nTask completed successfully.");
    } else if let Some(err) = result.error {
        println!("\nTask failed: {}", err);
    }

    println!("[{} iterations, {} tool calls]", result.iterations, result.tool_calls_executed);

    Ok(())
}

async fn run_interactive(free_only: bool, model: Option<String>, paths: Vec<PathBuf>, resume: bool) -> Result<()> {
    // Ensure we have an API key
    let api_key = match config::get_api_key() {
        Ok(key) => key,
        Err(_) => {
            // Prompt for key
            println!("No API key found. Get a free key at: https://openrouter.ai/keys\n");
            let key = ui::prompt_api_key()?;
            let mut cfg = config::Config::load()?;
            cfg.api_key = Some(key.clone());
            cfg.save()?;
            println!("\nKey saved to {}\n", config::config_path()?.display());
            key
        }
    };

    // Detect project context
    let cwd = std::env::current_dir()?;
    let cwd_str = cwd.display().to_string();
    let project = project::Project::detect(&cwd);
    if let Some(ref p) = project {
        println!("Project: {} ({:?}, {} files)", p.name, p.project_type, p.files.len());
    }

    // Check for recent Claude Code session in this directory
    let claude_context = if session::has_recent_claude_session(&cwd_str, 24).unwrap_or(false) {
        println!("Detected recent Claude Code session in this directory");
        match session::import_claude_context(&cwd_str, 10) {
            Ok(msgs) if !msgs.is_empty() => {
                println!("  → Importing {} recent prompts as context", msgs.len());
                Some(msgs)
            }
            _ => None
        }
    } else {
        None
    };

    // Load or fetch models
    let models = models::load_or_fetch(&api_key).await?;

    // Select model
    let selected_model = if let Some(m) = model {
        m
    } else {
        let available: Vec<_> = if free_only {
            models.iter().filter(|m| m.is_free()).cloned().collect()
        } else {
            models.clone()
        };

        if available.is_empty() {
            anyhow::bail!("No models available");
        }

        ui::pick_model(&available)?
    };

    println!("Using model: {}", selected_model);

    // Run TUI with session, project context, and optional Claude import
    ui::run_tui(&api_key, &selected_model, paths, resume, project, claude_context).await
}

async fn run_backburner(paths: &[PathBuf], watch_docs: bool) -> Result<()> {
    let work_dir = if paths.is_empty() {
        std::env::current_dir()?
    } else {
        paths[0].clone()
    };

    let mut bb = backburner::Backburner::new(work_dir);
    if watch_docs {
        bb.run_docs_mode().await
    } else {
        bb.run().await
    }
}
