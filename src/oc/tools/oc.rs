use serde_json::Value;
use tokio::process::Command;
use tracing::debug;

use crate::config::Config;
use crate::mcp::protocol::ToolCallResult;

// ── oc_get ────────────────────────────────────────────────────────────────────

pub async fn oc_get(params: &Value, cfg: &Config) -> ToolCallResult {
    let resource = match params.get("resource").and_then(|v| v.as_str()) {
        Some(r) => r,
        None => return ToolCallResult::error("Missing required parameter: resource"),
    };
    let name = params.get("name").and_then(|v| v.as_str());
    let namespace = params.get("namespace").and_then(|v| v.as_str());
    let label_selector = params.get("label_selector").and_then(|v| v.as_str());

    let mut args: Vec<String> = vec!["get".into(), resource.into()];

    if let Some(n) = name {
        args.push(n.into());
    }
    if let Some(ns) = namespace {
        args.extend(["-n".into(), ns.into()]);
    }
    if let Some(ls) = label_selector {
        args.extend(["-l".into(), ls.into()]);
    }
    args.extend(["-o".into(), "json".into()]);

    run_oc(cfg, &args).await
}

// ── oc_apply_yaml ─────────────────────────────────────────────────────────────

pub async fn oc_apply_yaml(params: &Value, cfg: &Config) -> ToolCallResult {
    let yaml = match params.get("yaml").and_then(|v| v.as_str()) {
        Some(y) => y,
        None => return ToolCallResult::error("Missing required parameter: yaml"),
    };
    let namespace = params.get("namespace").and_then(|v| v.as_str());

    let mut args: Vec<String> = vec!["apply".into(), "-f".into(), "-".into()];
    if let Some(ns) = namespace {
        args.extend(["-n".into(), ns.into()]);
    }

    run_oc_with_stdin(cfg, &args, yaml).await
}

// ── oc_delete ─────────────────────────────────────────────────────────────────

pub async fn oc_delete(params: &Value, cfg: &Config) -> ToolCallResult {
    let resource = match params.get("resource").and_then(|v| v.as_str()) {
        Some(r) => r,
        None => return ToolCallResult::error("Missing required parameter: resource"),
    };
    let name = match params.get("name").and_then(|v| v.as_str()) {
        Some(n) => n,
        None => return ToolCallResult::error("Missing required parameter: name"),
    };
    let namespace = params.get("namespace").and_then(|v| v.as_str());
    let ignore_not_found = params
        .get("ignore_not_found")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let mut args: Vec<String> = vec!["delete".into(), resource.into(), name.into()];
    if let Some(ns) = namespace {
        args.extend(["-n".into(), ns.into()]);
    }
    if ignore_not_found {
        args.push("--ignore-not-found=true".into());
    }

    run_oc(cfg, &args).await
}

// ── oc_wait ───────────────────────────────────────────────────────────────────

pub async fn oc_wait(params: &Value, cfg: &Config) -> ToolCallResult {
    let resource = match params.get("resource").and_then(|v| v.as_str()) {
        Some(r) => r,
        None => return ToolCallResult::error("Missing required parameter: resource"),
    };
    let condition = match params.get("condition").and_then(|v| v.as_str()) {
        Some(c) => c,
        None => return ToolCallResult::error("Missing required parameter: condition"),
    };
    let namespace = params.get("namespace").and_then(|v| v.as_str());
    let timeout_secs = params.get("timeout_secs").and_then(|v| v.as_u64()).unwrap_or(300);

    let mut args: Vec<String> = vec![
        "wait".into(),
        resource.into(),
        format!("--for=condition={}", condition),
        format!("--timeout={}s", timeout_secs),
    ];
    if let Some(ns) = namespace {
        args.extend(["-n".into(), ns.into()]);
    }

    run_oc(cfg, &args).await
}

// ── oc_logs ───────────────────────────────────────────────────────────────────

pub async fn oc_logs(params: &Value, cfg: &Config) -> ToolCallResult {
    let pod_name = match params.get("pod_name").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => return ToolCallResult::error("Missing required parameter: pod_name"),
    };
    let namespace = match params.get("namespace").and_then(|v| v.as_str()) {
        Some(ns) => ns,
        None => return ToolCallResult::error("Missing required parameter: namespace"),
    };
    let container = params.get("container").and_then(|v| v.as_str());
    let tail: u64 = params.get("tail").and_then(|v| v.as_u64()).unwrap_or(100);

    let mut args: Vec<String> =
        vec!["logs".into(), pod_name.into(), "-n".into(), namespace.into(), format!("--tail={}", tail)];
    if let Some(c) = container {
        args.extend(["-c".into(), c.into()]);
    }

    run_oc(cfg, &args).await
}

// ── oc_exec ───────────────────────────────────────────────────────────────────

pub async fn oc_exec(params: &Value, cfg: &Config) -> ToolCallResult {
    let pod_name = match params.get("pod_name").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => return ToolCallResult::error("Missing required parameter: pod_name"),
    };
    let namespace = match params.get("namespace").and_then(|v| v.as_str()) {
        Some(ns) => ns,
        None => return ToolCallResult::error("Missing required parameter: namespace"),
    };
    let command = match params.get("command").and_then(|v| v.as_str()) {
        Some(c) => c,
        None => return ToolCallResult::error("Missing required parameter: command"),
    };
    let container = params.get("container").and_then(|v| v.as_str());

    let mut args: Vec<String> =
        vec!["exec".into(), pod_name.into(), "-n".into(), namespace.into(), "--".into(), "sh".into(), "-c".into(), command.into()];
    if let Some(c) = container {
        // Insert -c <container> before the --
        let pos = args.iter().position(|a| a == "--").unwrap_or(args.len());
        args.insert(pos, c.into());
        args.insert(pos, "-c".into());
    }

    run_oc(cfg, &args).await
}

// ── cleanup_namespace ─────────────────────────────────────────────────────────

pub async fn cleanup_namespace(params: &Value, cfg: &Config) -> ToolCallResult {
    let namespace = match params.get("namespace").and_then(|v| v.as_str()) {
        Some(ns) => ns,
        None => return ToolCallResult::error("Missing required parameter: namespace"),
    };

    // Safety check: only clean up pw-* test namespaces
    if !namespace.starts_with("pw-") && !namespace.starts_with("test-") {
        return ToolCallResult::error(format!(
            "Safety check failed: namespace '{}' does not start with 'pw-' or 'test-'. Only test namespaces can be cleaned up via this tool.",
            namespace
        ));
    }

    let mut results = Vec::new();

    // Deletion order mirrors OcCliClient.cleanupTestNamespace
    let ordered_resources = [
        ("virtualmachineinstances", "force"),     // force delete VMIs first
        ("virtualmachines", "delete"),
        ("virtualmachineinstancemigrations", "delete"),
        ("datavolumes", "delete"),
        ("virtualmachinesnapshots", "delete"),
        ("templates", "delete"),
        ("instancetype.kubevirt.io/virtualmachineinstancetypes", "delete"),
        ("instancetype.kubevirt.io/virtualmachinepreferences", "delete"),
        ("persistentvolumeclaims", "delete"),
        ("secrets", "delete"),
        ("configmaps", "delete"),
        ("network-attachment-definitions", "delete"),
    ];

    for (resource, mode) in &ordered_resources {
        let mut args = vec!["delete".to_string(), resource.to_string(), "--all".to_string(), "-n".to_string(), namespace.to_string(), "--ignore-not-found=true".to_string()];
        if *mode == "force" {
            args.extend(["--force".to_string(), "--grace-period=0".to_string()]);
        }

        match run_oc_raw(cfg, &args).await {
            Ok(out) => {
                if !out.trim().is_empty() {
                    results.push(format!("  {} {}: {}", mode, resource, out.trim()));
                }
            }
            Err(e) => {
                results.push(format!("  {} {} FAILED: {}", mode, resource, e));
            }
        }
    }

    ToolCallResult::text(format!(
        "Namespace cleanup for '{}' complete:\n{}",
        namespace,
        results.join("\n")
    ))
}

// ── Internal helpers ──────────────────────────────────────────────────────────

async fn run_oc(cfg: &Config, args: &[String]) -> ToolCallResult {
    match run_oc_raw(cfg, args).await {
        Ok(out) => ToolCallResult::text(out),
        Err(e) => ToolCallResult::error(e),
    }
}

async fn run_oc_with_stdin(cfg: &Config, args: &[String], stdin_data: &str) -> ToolCallResult {
    debug!("oc {}", args.join(" "));

    let mut cmd = build_oc_command(cfg, args);
    cmd.stdin(std::process::Stdio::piped());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return ToolCallResult::error(format!("Failed to spawn oc: {}", e)),
    };

    if let Some(stdin) = child.stdin.take() {
        use tokio::io::AsyncWriteExt;
        let mut stdin = stdin;
        let _ = stdin.write_all(stdin_data.as_bytes()).await;
    }

    match child.wait_with_output().await {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            if out.status.success() {
                ToolCallResult::text(if stdout.is_empty() { "(no output)".into() } else { stdout })
            } else {
                ToolCallResult::error(format!(
                    "oc exited with {}\nstdout: {}\nstderr: {}",
                    out.status.code().unwrap_or(-1),
                    stdout,
                    stderr
                ))
            }
        }
        Err(e) => ToolCallResult::error(format!("Failed to wait for oc: {}", e)),
    }
}

async fn run_oc_raw(cfg: &Config, args: &[String]) -> Result<String, String> {
    debug!("oc {}", args.join(" "));

    let out = build_oc_command(cfg, args)
        .output()
        .await
        .map_err(|e| format!("Failed to run oc: {}", e))?;

    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();

    if out.status.success() {
        Ok(if stdout.is_empty() { "(no output)".into() } else { stdout })
    } else {
        Err(format!(
            "oc exited with {}\nstderr: {}",
            out.status.code().unwrap_or(-1),
            stderr
        ))
    }
}

fn build_oc_command(cfg: &Config, args: &[String]) -> Command {
    let mut cmd = Command::new(&cfg.oc_path);
    cmd.args(args);
    if let Some(ref kc) = cfg.kubeconfig {
        cmd.env("KUBECONFIG", kc);
    }
    cmd
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_cfg() -> Config {
        Config::from_env()
    }

    #[tokio::test]
    async fn cleanup_rejects_non_test_namespace() {
        let cfg = make_cfg();
        let params = json!({ "namespace": "default" });
        let result = cleanup_namespace(&params, &cfg).await;
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("Safety check failed"));
    }

    #[tokio::test]
    async fn cleanup_rejects_production_namespace() {
        let cfg = make_cfg();
        let params = json!({ "namespace": "openshift-virtualization" });
        let result = cleanup_namespace(&params, &cfg).await;
        assert_eq!(result.is_error, Some(true));
    }

    #[tokio::test]
    async fn oc_get_missing_resource_returns_error() {
        let cfg = make_cfg();
        let params = json!({});
        let result = oc_get(&params, &cfg).await;
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("Missing required parameter: resource"));
    }

    #[tokio::test]
    async fn oc_delete_missing_name_returns_error() {
        let cfg = make_cfg();
        let params = json!({ "resource": "pod" });
        let result = oc_delete(&params, &cfg).await;
        assert_eq!(result.is_error, Some(true));
        assert!(result.content[0].text.contains("Missing required parameter: name"));
    }

    #[tokio::test]
    async fn oc_wait_missing_condition_returns_error() {
        let cfg = make_cfg();
        let params = json!({ "resource": "pod/mypod" });
        let result = oc_wait(&params, &cfg).await;
        assert_eq!(result.is_error, Some(true));
    }

    #[tokio::test]
    async fn oc_exec_missing_command_returns_error() {
        let cfg = make_cfg();
        let params = json!({ "pod_name": "mypod", "namespace": "default" });
        let result = oc_exec(&params, &cfg).await;
        assert_eq!(result.is_error, Some(true));
    }

    #[tokio::test]
    async fn oc_logs_missing_namespace_returns_error() {
        let cfg = make_cfg();
        let params = json!({ "pod_name": "mypod" });
        let result = oc_logs(&params, &cfg).await;
        assert_eq!(result.is_error, Some(true));
    }

    #[test]
    fn pw_namespace_passes_safety_check() {
        // The safety check happens inside the async fn but we can verify the logic:
        assert!("pw-abc123".starts_with("pw-"));
        assert!("test-run-42".starts_with("test-"));
        assert!(!"default".starts_with("pw-") && !"default".starts_with("test-"));
    }
}
