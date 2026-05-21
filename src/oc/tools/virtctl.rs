use serde_json::Value;
use tracing::debug;

use crate::config::Config;
use crate::mcp::protocol::ToolCallResult;

pub async fn virtctl_migrate(params: &Value, cfg: &Config) -> ToolCallResult {
    let vm_name = match params.get("vm_name").and_then(|v| v.as_str()) {
        Some(n) => n,
        None => return ToolCallResult::error("Missing required parameter: vm_name"),
    };
    let namespace = match params.get("namespace").and_then(|v| v.as_str()) {
        Some(ns) => ns,
        None => return ToolCallResult::error("Missing required parameter: namespace"),
    };

    run_virtctl(cfg, &["migrate", vm_name, "-n", namespace]).await
}

pub async fn virtctl_pause(params: &Value, cfg: &Config) -> ToolCallResult {
    let vm_name = match params.get("vm_name").and_then(|v| v.as_str()) {
        Some(n) => n,
        None => return ToolCallResult::error("Missing required parameter: vm_name"),
    };
    let namespace = match params.get("namespace").and_then(|v| v.as_str()) {
        Some(ns) => ns,
        None => return ToolCallResult::error("Missing required parameter: namespace"),
    };

    run_virtctl(cfg, &["pause", "vm", vm_name, "-n", namespace]).await
}

pub async fn virtctl_unpause(params: &Value, cfg: &Config) -> ToolCallResult {
    let vm_name = match params.get("vm_name").and_then(|v| v.as_str()) {
        Some(n) => n,
        None => return ToolCallResult::error("Missing required parameter: vm_name"),
    };
    let namespace = match params.get("namespace").and_then(|v| v.as_str()) {
        Some(ns) => ns,
        None => return ToolCallResult::error("Missing required parameter: namespace"),
    };

    run_virtctl(cfg, &["unpause", "vm", vm_name, "-n", namespace]).await
}

pub async fn virtctl_ssh(params: &Value, cfg: &Config) -> ToolCallResult {
    let vm_name = match params.get("vm_name").and_then(|v| v.as_str()) {
        Some(n) => n,
        None => return ToolCallResult::error("Missing required parameter: vm_name"),
    };
    let namespace = match params.get("namespace").and_then(|v| v.as_str()) {
        Some(ns) => ns,
        None => return ToolCallResult::error("Missing required parameter: namespace"),
    };
    let command = match params.get("command").and_then(|v| v.as_str()) {
        Some(c) => c,
        None => return ToolCallResult::error("Missing required parameter: command"),
    };
    let username = params.get("username").and_then(|v| v.as_str()).unwrap_or("fedora");

    let target = format!("{}@{}", username, vm_name);
    run_virtctl(cfg, &["ssh", &target, "-n", namespace, "--", command]).await
}

// ── Internal helpers ──────────────────────────────────────────────────────────

async fn run_virtctl(cfg: &Config, args: &[&str]) -> ToolCallResult {
    debug!("virtctl {}", args.join(" "));

    let mut cmd = tokio::process::Command::new(&cfg.virtctl_path);
    cmd.args(args);
    if let Some(ref kc) = cfg.kubeconfig {
        cmd.env("KUBECONFIG", kc);
    }

    match cmd.output().await {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            if out.status.success() {
                ToolCallResult::text(if stdout.is_empty() {
                    format!("virtctl command succeeded. stderr: {}", stderr)
                } else {
                    stdout
                })
            } else {
                ToolCallResult::error(format!(
                    "virtctl exited with {}\nstdout: {}\nstderr: {}",
                    out.status.code().unwrap_or(-1),
                    stdout,
                    stderr
                ))
            }
        }
        Err(e) => ToolCallResult::error(format!("Failed to run virtctl: {}", e)),
    }
}
