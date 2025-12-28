//! Session persistence - logs, history, context
//!
//! Sessions are stored in ~/.local/state/hyle/sessions/
//! Each session is a directory with:
//! - meta.json: Session metadata (model, start time, etc.)
//! - messages.jsonl: Conversation history (append-only)
//! - log.jsonl: Event log (tool calls, errors, etc.)

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

use crate::config;

/// Session metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMeta {
    pub id: String,
    pub model: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub message_count: usize,
    pub total_tokens: u64,
    pub working_dir: String,
    pub description: Option<String>,
}

/// A message in the conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String, // "user", "assistant", "system"
    pub content: String,
    pub timestamp: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens: Option<u32>,
}

/// A log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp: DateTime<Utc>,
    pub kind: String, // "request", "response", "tool", "error"
    pub data: serde_json::Value,
}

/// Active session manager
pub struct Session {
    pub meta: SessionMeta,
    pub messages: Vec<Message>,
    session_dir: PathBuf,
    log_file: Option<File>,
}

impl Session {
    /// Create a new session
    pub fn new(model: &str) -> Result<Self> {
        let id = generate_session_id();
        let session_dir = sessions_dir()?.join(&id);
        fs::create_dir_all(&session_dir)?;

        let meta = SessionMeta {
            id: id.clone(),
            model: model.to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            message_count: 0,
            total_tokens: 0,
            working_dir: std::env::current_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| ".".to_string()),
            description: None,
        };

        let mut session = Self {
            meta,
            messages: vec![],
            session_dir,
            log_file: None,
        };

        // Add system message
        session.add_message(Message {
            role: "system".into(),
            content: "You are a helpful coding assistant. Be concise.".into(),
            timestamp: Utc::now(),
            tokens: None,
        })?;

        session.save_meta()?;
        session.open_log()?;

        Ok(session)
    }

    /// Load an existing session
    pub fn load(id: &str) -> Result<Self> {
        let session_dir = sessions_dir()?.join(id);
        if !session_dir.exists() {
            anyhow::bail!("Session not found: {}", id);
        }

        // Load metadata
        let meta_path = session_dir.join("meta.json");
        let meta: SessionMeta = serde_json::from_str(
            &fs::read_to_string(&meta_path).context("Failed to read meta.json")?
        ).context("Failed to parse meta.json")?;

        // Load messages
        let messages_path = session_dir.join("messages.jsonl");
        let messages = if messages_path.exists() {
            let file = File::open(&messages_path)?;
            let reader = BufReader::new(file);
            reader.lines()
                .filter_map(|line| line.ok())
                .filter_map(|line| serde_json::from_str(&line).ok())
                .collect()
        } else {
            vec![]
        };

        let mut session = Self {
            meta,
            messages,
            session_dir,
            log_file: None,
        };

        session.open_log()?;
        Ok(session)
    }

    /// Load the most recent session, or create new
    pub fn load_or_create(model: &str) -> Result<Self> {
        if let Some(recent) = most_recent_session()? {
            // Only resume if same model and less than 1 hour old
            let age = Utc::now() - recent.updated_at;
            if recent.model == model && age.num_hours() < 1 {
                return Session::load(&recent.id);
            }
        }
        Session::new(model)
    }

    /// Add a message to the conversation
    pub fn add_message(&mut self, msg: Message) -> Result<()> {
        // Append to messages file
        let messages_path = self.session_dir.join("messages.jsonl");
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&messages_path)?;

        writeln!(file, "{}", serde_json::to_string(&msg)?)?;

        self.messages.push(msg);
        self.meta.message_count = self.messages.len();
        self.meta.updated_at = Utc::now();

        Ok(())
    }

    /// Add user message
    pub fn add_user_message(&mut self, content: &str) -> Result<()> {
        self.add_message(Message {
            role: "user".into(),
            content: content.to_string(),
            timestamp: Utc::now(),
            tokens: None,
        })
    }

    /// Add assistant message
    pub fn add_assistant_message(&mut self, content: &str, tokens: Option<u32>) -> Result<()> {
        if let Some(t) = tokens {
            self.meta.total_tokens += t as u64;
        }
        self.add_message(Message {
            role: "assistant".into(),
            content: content.to_string(),
            timestamp: Utc::now(),
            tokens,
        })
    }

    /// Log an event
    pub fn log(&mut self, kind: &str, data: serde_json::Value) -> Result<()> {
        let entry = LogEntry {
            timestamp: Utc::now(),
            kind: kind.to_string(),
            data,
        };

        if let Some(ref mut file) = self.log_file {
            writeln!(file, "{}", serde_json::to_string(&entry)?)?;
            file.flush()?;
        }

        Ok(())
    }

    /// Save metadata
    pub fn save_meta(&self) -> Result<()> {
        let meta_path = self.session_dir.join("meta.json");
        let content = serde_json::to_string_pretty(&self.meta)?;
        fs::write(&meta_path, content)?;
        Ok(())
    }

    /// Open log file for appending
    fn open_log(&mut self) -> Result<()> {
        let log_path = self.session_dir.join("log.jsonl");
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)?;
        self.log_file = Some(file);
        Ok(())
    }

    /// Get messages for API request (excluding tokens field)
    pub fn messages_for_api(&self) -> Vec<serde_json::Value> {
        self.messages.iter().map(|m| {
            serde_json::json!({
                "role": m.role,
                "content": m.content
            })
        }).collect()
    }

    /// Get conversation summary for display
    pub fn summary(&self) -> String {
        let user_msgs: Vec<_> = self.messages.iter()
            .filter(|m| m.role == "user")
            .collect();

        if user_msgs.is_empty() {
            return "(empty session)".to_string();
        }

        // First user message, truncated
        let first = &user_msgs[0].content;
        let truncated = if first.len() > 50 {
            format!("{}...", &first[..50])
        } else {
            first.clone()
        };

        format!("{} ({} messages)", truncated, self.messages.len())
    }
}

/// Get sessions directory
pub fn sessions_dir() -> Result<PathBuf> {
    let dir = config::state_dir()?.join("sessions");
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Generate a unique session ID
fn generate_session_id() -> String {
    let now = Utc::now();
    format!("{}", now.format("%Y%m%d-%H%M%S"))
}

/// List all sessions, sorted by updated_at (newest first)
pub fn list_sessions() -> Result<Vec<SessionMeta>> {
    let dir = sessions_dir()?;
    let mut sessions = Vec::new();

    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let meta_path = entry.path().join("meta.json");
            if meta_path.exists() {
                if let Ok(content) = fs::read_to_string(&meta_path) {
                    if let Ok(meta) = serde_json::from_str::<SessionMeta>(&content) {
                        sessions.push(meta);
                    }
                }
            }
        }
    }

    // Sort by updated_at, newest first
    sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(sessions)
}

/// Get most recent session
pub fn most_recent_session() -> Result<Option<SessionMeta>> {
    let sessions = list_sessions()?;
    Ok(sessions.into_iter().next())
}

/// Clean up old sessions (keep last N)
pub fn cleanup_sessions(keep: usize) -> Result<usize> {
    let sessions = list_sessions()?;
    let mut removed = 0;

    for session in sessions.into_iter().skip(keep) {
        let session_dir = sessions_dir()?.join(&session.id);
        if fs::remove_dir_all(&session_dir).is_ok() {
            removed += 1;
        }
    }

    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_id_format() {
        let id = generate_session_id();
        assert!(id.len() >= 15); // YYYYMMDD-HHMMSS
        assert!(id.contains('-'));
    }

    #[test]
    fn test_message_serialize() {
        let msg = Message {
            role: "user".into(),
            content: "Hello".into(),
            timestamp: Utc::now(),
            tokens: Some(5),
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("user"));
        assert!(json.contains("Hello"));
    }
}
