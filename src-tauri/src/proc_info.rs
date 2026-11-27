use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const SHARED_PS_TTL: Duration = Duration::from_secs(2);

static SHARED_PS: Mutex<Option<(Instant, Arc<String>)>> = Mutex::new(None);

/// One coalesced `ps` table (`pid ppid state etime pcpu rss comm`) shared by
/// the telemetry, extras, and privacy collectors. The mutex is held across the
/// spawn, so concurrent callers wait for one `ps` instead of running several.
pub(crate) fn shared_ps_snapshot() -> Option<Arc<String>> {
    let mut guard = SHARED_PS.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((captured_at, table)) = guard.as_ref() {
        if captured_at.elapsed() < SHARED_PS_TTL {
            return Some(table.clone());
        }
    }
    let raw = cmd_stdout(
        "/bin/ps",
        &["-axo", "pid=,ppid=,state=,etime=,pcpu=,rss=,comm="],
    )?;
    let table = Arc::new(raw);
    *guard = Some((Instant::now(), table.clone()));
    Some(table)
}

pub(crate) fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

pub(crate) fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

pub(crate) fn cmd_stdout(prog: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(prog).args(args).output().ok()?;
    if out.stdout.is_empty() && !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

pub(crate) fn find_cli_binary<'a>(candidates: &[&'a str]) -> Option<&'a str> {
    candidates.iter().copied().find(|p| Path::new(p).exists())
}

pub(crate) fn http_get_localhost(
    port: u16,
    path: &str,
    max_bytes: usize,
    timeout_ms: u64,
) -> Option<String> {
    let addr: SocketAddr = ([127, 0, 0, 1], port).into();
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_millis(timeout_ms)).ok()?;
    let timeout = Some(Duration::from_millis(timeout_ms));
    let _ = stream.set_read_timeout(timeout);
    let _ = stream.set_write_timeout(timeout);
    let req = format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    stream.write_all(req.as_bytes()).ok()?;

    let mut response = Vec::with_capacity(max_bytes.min(8192));
    let mut chunk = [0_u8; 2048];
    while response.len() < max_bytes {
        let to_read = (max_bytes - response.len()).min(chunk.len());
        match stream.read(&mut chunk[..to_read]) {
            Ok(0) | Err(_) => break,
            Ok(n) => response.extend_from_slice(&chunk[..n]),
        }
    }
    if response.is_empty() {
        None
    } else {
        Some(String::from_utf8_lossy(&response).into_owned())
    }
}

pub(crate) fn canonical_process_name(comm: &str) -> String {
    let trimmed = comm.trim();
    if let Some(app_idx) = trimmed.find(".app/") {
        let app_prefix = &trimmed[..app_idx];
        let app_name = app_prefix.rsplit('/').next().unwrap_or(app_prefix);
        if !app_name.is_empty() {
            return app_name.to_string();
        }
    }
    let base = trimmed.rsplit('/').next().unwrap_or(trimmed);
    base.strip_prefix('-').unwrap_or(base).to_string()
}

pub(crate) fn classify_coding_agent(comm: &str) -> Option<&'static str> {
    let canonical = canonical_process_name(comm);
    let lower = canonical.to_ascii_lowercase();
    if lower == "claude" || lower == "claude code" {
        Some("Claude Code")
    } else if lower == "codex" {
        Some("Codex")
    } else if lower == "cursor" {
        Some("Cursor")
    } else if lower == "agy" || lower == "antigravity" {
        Some("Antigravity")
    } else if lower == "windsurf" {
        Some("Windsurf")
    } else if lower == "aider" {
        Some("Aider")
    } else if lower == "opencode" {
        Some("OpenCode")
    } else {
        None
    }
}

pub(crate) fn classify_ai_agent(comm: &str) -> Option<&'static str> {
    if let Some(coding) = classify_coding_agent(comm) {
        return Some(coding);
    }
    if canonical_process_name(comm).eq_ignore_ascii_case("ollama") {
        Some("Ollama")
    } else {
        None
    }
}

pub(crate) fn is_dev_server_binary(comm: &str) -> bool {
    let exe = std::path::Path::new(comm.trim())
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(comm)
        .to_ascii_lowercase();
    matches!(
        exe.as_str(),
        "node"
            | "vite"
            | "next-server"
            | "next"
            | "tsc"
            | "tsserver"
            | "vitest"
            | "jest"
            | "esbuild"
            | "webpack"
            | "turbopack"
            | "prisma"
            | "bun"
            | "deno"
            | "python"
            | "python3"
            | "ruby"
            | "uvicorn"
            | "gunicorn"
            | "flask"
            | "rails"
            | "cargo"
            | "rust-analyzer"
    )
}
