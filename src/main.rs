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
mod ui;
mod tools;

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
    },
    Task {
        task: String,
        paths: Vec<PathBuf>,
    },
    Doctor,
    Models {
        refresh: bool,
    },
    ConfigSet {
        key: String,
        value: String,
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

    if args.first().map(|s| s.as_str()) == Some("config") {
        if args.get(1).map(|s| s.as_str()) == Some("set") {
            return Command::ConfigSet {
                key: args.get(2).cloned().unwrap_or_default(),
                value: args.get(3).cloned().unwrap_or_default(),
            };
        }
    }

    // Parse flags and paths
    let mut free_only = false;
    let mut model = None;
    let mut task = None;
    let mut paths = Vec::new();
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "--free" | "-f" => free_only = true,
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

    if let Some(task_str) = task {
        Command::Task { task: task_str, paths }
    } else {
        Command::Interactive { free_only, model, paths }
    }
}

fn print_help() {
    println!(r#"hyle - Rust-native code assistant (OpenRouter powered)

USAGE:
    hyle --free [PATHS...]        # choose free model, interactive loop
    hyle --model <id> [PATHS...]  # use specific model
    hyle --task "..." [PATHS...]  # one-shot: produce diff, ask apply
    hyle doctor                   # check config, key, network
    hyle models --refresh         # refresh models cache
    hyle config set key <value>   # set config value

FLAGS:
    -f, --free              Only show free models in picker
    -m, --model <id>        Use specific model ID
    -t, --task <text>       One-shot task mode
    -h, --help              Show this help

CONFIG:
    ~/.config/hyle/config.json    API key, preferences
    ~/.cache/hyle/models.json     Cached model list
    ~/.local/state/hyle/          Session logs

ENVIRONMENT:
    OPENROUTER_API_KEY              Override API key from config

CONTROLS (interactive mode):
    k       Kill current operation
    t       Throttle (give it time)
    f       Full speed
    Tab     Switch tabs
    Esc     Quit
"#);
}

// ═══════════════════════════════════════════════════════════════
// MAIN
// ═══════════════════════════════════════════════════════════════

#[tokio::main]
async fn main() -> Result<()> {
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
        Command::ConfigSet { key, value } => {
            run_config_set(&key, &value)
        }
        Command::Task { task, paths } => {
            run_task(&task, &paths).await
        }
        Command::Interactive { free_only, model, paths } => {
            run_interactive(free_only, model, paths).await
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
    let api_key = config::get_api_key()?;
    let cfg = config::Config::load()?;

    // Get model
    let model = cfg.default_model.clone()
        .unwrap_or_else(|| "meta-llama/llama-3.2-3b-instruct:free".to_string());

    println!("Task: {}", task);
    println!("Model: {}", model);
    println!("Paths: {:?}\n", paths);

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
        format!("Given these files:\n{}\n\nTask: {}\n\nProvide your changes as a unified diff.", context, task)
    };

    // Stream response
    println!("Generating...\n");
    let mut response = String::new();
    let mut stream = client::stream_completion(&api_key, &model, &prompt).await?;

    while let Some(chunk) = stream.recv().await {
        match chunk {
            client::StreamEvent::Token(t) => {
                print!("{}", t);
                response.push_str(&t);
            }
            client::StreamEvent::Done(usage) => {
                println!("\n\n[{} prompt + {} completion tokens]",
                    usage.prompt_tokens, usage.completion_tokens);
            }
            client::StreamEvent::Error(e) => {
                eprintln!("\nError: {}", e);
            }
        }
    }

    Ok(())
}

async fn run_interactive(free_only: bool, model: Option<String>, paths: Vec<PathBuf>) -> Result<()> {
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

    // Run TUI
    ui::run_tui(&api_key, &selected_model, paths).await
}
