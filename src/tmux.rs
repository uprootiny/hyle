//! Tmux integration utilities
//!
//! Provides detection and terminal info when running inside tmux.

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
}
