# Security Audit Report

**Auditor:** Security Architect
**Date:** 2025-12-28
**Scope:** `src/tools.rs`, `src/config.rs`, `src/server.rs`

---

## Executive Summary

This audit identifies **5 critical security vulnerabilities** that enable injection attacks, credential exposure, and denial-of-service. Immediate remediation required before production deployment.

---

## Findings

### 1. Command Injection via Unvalidated Bash Execution

**Severity:** CRITICAL
**Location:** `src/tools.rs:542-592`

The bash executor accepts arbitrary shell commands with zero input validation:

```rust
fn exec_bash(&self, call: &mut ToolCall, kill: Arc<AtomicBool>) -> Result<()> {
    let command = call.args.get("command")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("bash: missing 'command' argument"))?;

    let mut child = std::process::Command::new("bash")
        .arg("-c")
        .arg(command)  // <-- DIRECT PASS-THROUGH, NO VALIDATION
```

**Risk:** Any attacker-controlled string becomes a shell command. Combined with the HTTP server accepting user input, this enables **remote code execution**.

**Recommendation:**
- Implement command allowlisting
- Sanitize shell metacharacters
- Consider sandboxed execution (firejail, nsjail)

---

### 2. Regex Denial-of-Service (ReDoS)

**Severity:** HIGH
**Location:** `src/tools.rs:477-495`

```rust
let regex = regex::Regex::new(pattern)?;  // <-- NO VALIDATION
```

**Risk:** Malicious patterns like `(a+)+b` cause exponential backtracking, freezing the thread.

**Recommendation:**
- Set regex compilation timeout
- Limit pattern complexity
- Use `regex::RegexBuilder::size_limit()`

---

### 3. Credential Exposure - TOCTOU Race

**Severity:** HIGH
**Location:** `src/config.rs:411-426`

```rust
let content = serde_json::to_string_pretty(self)?;
fs::write(&path, &content)?;  // <-- API KEY WRITTEN IN PLAIN TEXT

// Permissions set AFTER write:
perms.set_mode(0o600);
fs::set_permissions(&path, perms)?;
```

**Risk:** Between file creation and permission setting, the API key is world-readable. Race condition window enables credential theft on multi-user systems.

**Recommendation:**
- Create file with restricted permissions from the start using `OpenOptions`
- Or write to temp file + atomic rename

---

### 4. Path Traversal in Session Loading

**Severity:** HIGH
**Location:** `src/server.rs:253-256`

```rust
let id = p.trim_start_matches("/session/");
handle_session(id).await
```

**Risk:** Session IDs extracted with minimal validation. Attacker requests `/session/../../../etc/passwd` to access arbitrary files.

**Recommendation:**
- Validate session ID format (alphanumeric + dash only)
- Canonicalize paths and verify they remain within session directory

---

### 5. Memory Exhaustion via Content-Length

**Severity:** HIGH
**Location:** `src/server.rs:209-224`

```rust
content_length = len.trim().parse().unwrap_or(0);
let mut body = vec![0u8; content_length];  // <-- UNBOUNDED ALLOCATION
```

**Risk:** Attacker sends `Content-Length: 999999999999` to allocate gigabytes of RAM, causing DoS.

**Recommendation:**
- Cap `content_length` at reasonable maximum (e.g., 10MB)
- Implement streaming body parsing for large requests

---

## Summary Table

| Severity | Issue | Location | Impact |
|----------|-------|----------|--------|
| CRITICAL | Command Injection | tools.rs:552-554 | RCE via arbitrary bash |
| HIGH | Path Traversal | server.rs:253-256 | Unauthorized file access |
| HIGH | Credential Exposure | config.rs:417-423 | API key leak via TOCTOU |
| HIGH | ReDoS Attack | tools.rs:487 | DoS via malicious regex |
| HIGH | Memory Exhaustion | server.rs:211, 218 | DoS via Content-Length |

---

## Remediation Priority

1. **Immediate:** Command injection (tools.rs)
2. **Immediate:** Memory exhaustion cap (server.rs)
3. **High:** Path traversal validation (server.rs)
4. **High:** Credential file permissions (config.rs)
5. **Medium:** ReDoS protection (tools.rs)
