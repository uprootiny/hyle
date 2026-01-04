//! Tmux integration utilities
//!
//! Provides detection, terminal info, and window management when running inside tmux.

use std::process::Command;
use std::sync::OnceLock;

/// Store original window name for restoration on exit
static ORIGINAL_WINDOW_NAME: OnceLock<String> = OnceLock::new();

/// Check if we're running inside tmux
pub fn is_tmux() -> bool {
    std::env::var("TMUX").is_ok()
}

/// Get current terminal width
pub fn term_width() -> u16 {
    std::env::var("COLUMNS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(80)
}

/// Check if terminal is wide enough for sidebar (160+ cols)
pub fn is_wide() -> bool {
    term_width() >= 160
}

// ═══════════════════════════════════════════════════════════════
// WINDOW NAMING
// ═══════════════════════════════════════════════════════════════

/// Get current tmux window name
pub fn get_window_name() -> Option<String> {
    if !is_tmux() {
        return None;
    }

    Command::new("tmux")
        .args(["display-message", "-p", "#W"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
            } else {
                None
            }
        })
}

/// Rename current tmux window
pub fn rename_window(name: &str) -> bool {
    if !is_tmux() {
        return false;
    }

    // Save original name on first call
    if ORIGINAL_WINDOW_NAME.get().is_none() {
        if let Some(original) = get_window_name() {
            let _ = ORIGINAL_WINDOW_NAME.set(original);
        }
    }

    Command::new("tmux")
        .args(["rename-window", name])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Set window title with project context
/// Format: "hyle:project" or "hyle:project:status"
pub fn set_window_title(project: Option<&str>, status: Option<&str>) {
    let title = match (project, status) {
        (Some(p), Some(s)) => format!("hyle:{}:{}", truncate_path(p), s),
        (Some(p), None) => format!("hyle:{}", truncate_path(p)),
        (None, Some(s)) => format!("hyle:{}", s),
        (None, None) => "hyle".to_string(),
    };
    rename_window(&title);
}

/// Set status in window name (keeps project prefix)
pub fn set_status(status: &str) {
    if !is_tmux() {
        return;
    }

    // Get current project from window name if present
    if let Some(current) = get_window_name() {
        if current.starts_with("hyle:") {
            let parts: Vec<&str> = current.splitn(3, ':').collect();
            if parts.len() >= 2 {
                let project = parts.get(1).unwrap_or(&"");
                rename_window(&format!("hyle:{}:{}", project, status));
                return;
            }
        }
    }
    rename_window(&format!("hyle:{}", status));
}

/// Clear status, keep just project name
pub fn clear_status() {
    if !is_tmux() {
        return;
    }

    if let Some(current) = get_window_name() {
        if current.starts_with("hyle:") {
            let parts: Vec<&str> = current.splitn(3, ':').collect();
            if let Some(project) = parts.get(1) {
                if !project.is_empty() {
                    rename_window(&format!("hyle:{}", project));
                    return;
                }
            }
        }
    }
    rename_window("hyle");
}

/// Restore original window name (call on exit)
pub fn restore_window_name() {
    if let Some(original) = ORIGINAL_WINDOW_NAME.get() {
        if is_tmux() {
            let _ = Command::new("tmux")
                .args(["rename-window", original])
                .output();
        }
    }
}

/// Initialize window with project name
pub fn init_window(work_dir: &std::path::Path) {
    if !is_tmux() {
        return;
    }

    let project = work_dir.file_name().and_then(|n| n.to_str()).unwrap_or("~");

    set_window_title(Some(project), None);
}

/// Truncate path to last component or reasonable length
fn truncate_path(path: &str) -> String {
    // If it's a path, get last component
    if path.contains('/') {
        path.rsplit('/').next().unwrap_or(path).to_string()
    } else if path.len() > 20 {
        format!("{}…", &path[..19])
    } else {
        path.to_string()
    }
}

// ═══════════════════════════════════════════════════════════════
// NOTIFICATIONS & ALERTS
// ═══════════════════════════════════════════════════════════════

/// Display a notification message in tmux status line
pub fn notify(message: &str) {
    if !is_tmux() {
        return;
    }

    let _ = Command::new("tmux")
        .args(["display-message", message])
        .output();
}

/// Display a notification that auto-dismisses
pub fn notify_briefly(message: &str, duration_ms: u32) {
    if !is_tmux() {
        return;
    }

    let _ = Command::new("tmux")
        .args(["display-message", "-d", &duration_ms.to_string(), message])
        .output();
}

/// Send bell/alert (useful when task completes in background)
pub fn bell() {
    if is_tmux() {
        // Send bell to trigger tmux visual/audio bell
        print!("\x07");
    }
}

// ═══════════════════════════════════════════════════════════════
// CLIPBOARD / PASTE BUFFER
// ═══════════════════════════════════════════════════════════════

/// Copy text to tmux paste buffer
pub fn copy_to_buffer(text: &str) -> bool {
    if !is_tmux() {
        return false;
    }

    Command::new("tmux")
        .args(["set-buffer", text])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Copy text to tmux buffer with a name
pub fn copy_to_named_buffer(name: &str, text: &str) -> bool {
    if !is_tmux() {
        return false;
    }

    Command::new("tmux")
        .args(["set-buffer", "-b", name, text])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Get text from tmux paste buffer
pub fn get_buffer() -> Option<String> {
    if !is_tmux() {
        return None;
    }

    Command::new("tmux")
        .args(["show-buffer"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                Some(String::from_utf8_lossy(&o.stdout).to_string())
            } else {
                None
            }
        })
}

// ═══════════════════════════════════════════════════════════════
// PANE MANAGEMENT
// ═══════════════════════════════════════════════════════════════

/// Split current pane and run a command
pub fn split_run(command: &str, vertical: bool, size_percent: u8) -> bool {
    if !is_tmux() {
        return false;
    }

    let split_flag = if vertical { "-v" } else { "-h" };
    let size = format!("{}%", size_percent.min(90));

    Command::new("tmux")
        .args(["split-window", split_flag, "-p", &size, command])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Show output in a temporary popup (tmux 3.2+)
pub fn popup(title: &str, command: &str, width: u8, height: u8) -> bool {
    if !is_tmux() {
        return false;
    }

    let w = format!("{}%", width.min(95));
    let h = format!("{}%", height.min(95));

    Command::new("tmux")
        .args([
            "display-popup",
            "-T",
            title,
            "-w",
            &w,
            "-h",
            &h,
            "-E",
            command,
        ])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Show text in a popup viewer
pub fn popup_text(title: &str, text: &str) -> bool {
    // Use echo + less for viewing
    let escaped = text.replace('\'', "'\"'\"'");
    let cmd = format!("echo '{}' | less -R", escaped);
    popup(title, &cmd, 80, 80)
}

// ═══════════════════════════════════════════════════════════════
// SESSION INFO
// ═══════════════════════════════════════════════════════════════

/// Get current tmux session name
pub fn session_name() -> Option<String> {
    if !is_tmux() {
        return None;
    }

    Command::new("tmux")
        .args(["display-message", "-p", "#S"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
            } else {
                None
            }
        })
}

/// Get current pane ID
pub fn pane_id() -> Option<String> {
    if !is_tmux() {
        return None;
    }

    Command::new("tmux")
        .args(["display-message", "-p", "#{pane_id}"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
            } else {
                None
            }
        })
}

/// Get number of panes in current window
pub fn pane_count() -> usize {
    if !is_tmux() {
        return 1;
    }

    Command::new("tmux")
        .args(["display-message", "-p", "#{window_panes}"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8_lossy(&o.stdout).trim().parse().ok()
            } else {
                None
            }
        })
        .unwrap_or(1)
}

// ═══════════════════════════════════════════════════════════════
// ENVIRONMENT
// ═══════════════════════════════════════════════════════════════

/// Set a tmux environment variable (available to new panes)
pub fn set_env(key: &str, value: &str) -> bool {
    if !is_tmux() {
        return false;
    }

    Command::new("tmux")
        .args(["setenv", key, value])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Get a tmux environment variable
pub fn get_env(key: &str) -> Option<String> {
    if !is_tmux() {
        return None;
    }

    Command::new("tmux")
        .args(["showenv", key])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                let output = String::from_utf8_lossy(&o.stdout);
                // Output is "KEY=value", extract value
                output.trim().split_once('=').map(|(_, v)| v.to_string())
            } else {
                None
            }
        })
}

// ═══════════════════════════════════════════════════════════════
// CONVENIENCE
// ═══════════════════════════════════════════════════════════════

/// Notify when a long-running task completes
pub fn task_complete(task_name: &str, success: bool) {
    let icon = if success { "✓" } else { "✗" };
    notify_briefly(&format!("{} {}", icon, task_name), 3000);
    if !success {
        bell();
    }
    clear_status();
}

/// Set up hyle tmux environment
pub fn setup(work_dir: &std::path::Path) {
    if !is_tmux() {
        return;
    }

    init_window(work_dir);

    // Store hyle session info in tmux env
    set_env("HYLE_ACTIVE", "1");
    if let Some(dir) = work_dir.to_str() {
        set_env("HYLE_WORKDIR", dir);
    }
}

/// Clean up on exit
pub fn cleanup() {
    if !is_tmux() {
        return;
    }

    restore_window_name();

    // Clear hyle env vars
    let _ = Command::new("tmux")
        .args(["setenv", "-u", "HYLE_ACTIVE"])
        .output();
    let _ = Command::new("tmux")
        .args(["setenv", "-u", "HYLE_WORKDIR"])
        .output();
}

// ═══════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_tmux() {
        // Just test it doesn't panic
        let _ = is_tmux();
    }

    #[test]
    fn test_term_width() {
        let width = term_width();
        assert!(width > 0);
    }

    #[test]
    fn test_is_wide() {
        // Just test it doesn't panic
        let _ = is_wide();
    }

    #[test]
    fn test_truncate_path() {
        assert_eq!(truncate_path("project"), "project");
        assert_eq!(truncate_path("/home/user/project"), "project");
        assert_eq!(
            truncate_path("a_very_long_project_name_here"),
            "a_very_long_project…"
        );
    }

    #[test]
    fn test_get_window_name() {
        // Just test it doesn't panic (may return None if not in tmux)
        let _ = get_window_name();
    }

    #[test]
    fn test_rename_window() {
        // Just test it doesn't panic
        let _ = rename_window("test");
    }
}
