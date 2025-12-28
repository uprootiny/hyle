# Distributed Systems Analysis

**Reviewer:** Distributed Systems Engineer
**Date:** 2025-12-28
**Scope:** `src/session.rs`, `src/server.rs`, `src/agent.rs`

---

## Executive Summary

5 critical failure modes will cause data loss, inconsistency, or state corruption when running multiple hyle instances. No coordination mechanisms exist between instances.

---

## Findings

### 1. Session ID Collision

**Severity:** HIGH
**Location:** `src/session.rs:275-278`

```rust
fn generate_session_id() -> String {
    let now = Utc::now();
    format!("{}", now.format("%Y%m%d-%H%M%S"))  // 1-second granularity
}
```

**Problem:** Two instances started in the same second generate identical IDs. Both write to same session directory, corrupting JSON.

**Recommendation:**
- Add random suffix: `format!("{}-{}", now.format("%Y%m%d-%H%M%S"), rand_suffix())`
- Or use UUID v4

---

### 2. TOCTOU Race in Session Selection

**Severity:** MEDIUM
**Location:** `src/session.rs:136-144`

```rust
pub fn load_or_create(model: &str) -> Result<Self> {
    if let Some(recent) = most_recent_session()? {  // CHECK
        let age = Utc::now() - recent.updated_at;
        if recent.model == model && age.num_hours() < 1 {
            return Session::load(&recent.id);  // USE
        }
    }
    Session::new(model)
}
```

**Problem:** Between check and use:
- Instance A reads session metadata
- Instance B deletes that session (cleanup)
- Instance A load fails, creates new session in same second as B

**Recommendation:**
- Lock session directory during load
- Retry with backoff on ENOENT

---

### 3. Concurrent File Writes Without Locking

**Severity:** CRITICAL
**Location:** `src/session.rs:148-162`

```rust
let mut file = OpenOptions::new()
    .create(true)
    .append(true)
    .open(&messages_path)?;

writeln!(file, "{}", serde_json::to_string(&msg)?)?;  // NO LOCK
```

**Problem:** `messages.jsonl` uses append mode without file locking. Two instances writing simultaneously can:
- Interleave JSON writes mid-line
- Corrupt JSONL format (not recoverable)
- Lose messages if kernel buffers collide

**Recommendation:**
- Use `flock()` or `fcntl()` advisory locks
- Or append via atomic write (temp + rename)

---

### 4. In-Memory State Desynchronization

**Severity:** MEDIUM-HIGH
**Location:** `src/server.rs:116-123, 603-609`

```rust
pub struct ServerState {
    busy: bool,                    // IN-MEMORY ONLY
    rate_limits: RateLimitInfo,    // IN-MEMORY ONLY
    request_times: Vec<Instant>,   // IN-MEMORY ONLY
}
```

**Problem:** Two server instances have independent `Arc<RwLock<ServerState>>`:
- Instance A sets `busy=true` in its lock
- Instance B has separate `busy=false`
- Both execute simultaneously, violating mutual exclusion
- Rate limits lost on restart

**Recommendation:**
- Move state to shared store (SQLite, Redis)
- Or implement distributed lock via filesystem

---

### 5. Session Metadata Corruption via Partial Writes

**Severity:** HIGH
**Location:** `src/session.rs:215-220`

```rust
pub fn save_meta(&self) -> Result<()> {
    let meta_path = self.session_dir.join("meta.json");
    let content = serde_json::to_string_pretty(&self.meta)?;
    fs::write(&meta_path, content)?;  // NOT ATOMIC
    Ok(())
}
```

**Problem:**
1. Instance A writes `meta.json`
2. Instance B reads mid-write, gets truncated JSON
3. `serde_json::from_str()` fails
4. Entire session unloadable

**Recommendation:**
- Atomic write pattern: write to `.meta.json.tmp`, then `rename()`
- `rename()` is atomic on POSIX

---

## Network Partition Scenario

When two instances run with no coordination (failover):

1. **Instance A** creates session "20250129-150000", writes 50 messages
2. **Network partition** — A unreachable
3. **Instance B** starts, creates session "20250129-150001"
4. **Partition heals** — both instances live
5. **User request to B** loads A's session (most recent)
6. **B appends messages** to A's `messages.jsonl`
7. **A resumes**, updates `meta.json` with `message_count=50`
8. **Result:** Split-brain — file has 100 messages, meta says 50

---

## Summary Table

| Issue | Location | Root Cause | Impact |
|-------|----------|------------|--------|
| Session ID collision | session.rs:277 | 1-second granularity | Data overwrite |
| TOCTOU race | session.rs:137-141 | Check-then-use | Session orphaning |
| Concurrent writes | session.rs:151-156 | No file locking | JSONL corruption |
| State desync | server.rs:120-123 | Per-instance RwLock | Mutex bypass |
| Partial writes | session.rs:219 | No atomic rename | Meta corruption |

---

## Recommendations

| Priority | Fix | Effort |
|----------|-----|--------|
| P0 | Add UUID to session IDs | Low |
| P0 | Atomic meta.json writes (temp+rename) | Low |
| P1 | File locking for messages.jsonl | Medium |
| P2 | Persistent state store for server | High |
| P2 | Distributed session ownership | High |
