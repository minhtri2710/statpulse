use std::process::Command;

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

pub(crate) fn classify_ai_agent(comm: &str) -> Option<&'static str> {
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
    } else if lower == "ollama" {
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
