use serde_json::{json, Value};
use tokio::process::Command;

use crate::config::Config;

pub async fn run_tests(params: &Value, cfg: &Config) -> Value {
    let file = params.get("file").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let grep = params.get("grep").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let grep_invert = params.get("grep_invert").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let workers = params.get("workers").and_then(|v| v.as_u64());
    let retries = params.get("retries").and_then(|v| v.as_u64());
    let headed = params.get("headed").and_then(|v| v.as_bool()).unwrap_or(false);
    let debug = params.get("debug").and_then(|v| v.as_bool()).unwrap_or(false);
    let timeout_ms = params.get("timeout").and_then(|v| v.as_u64());
    let shard = params.get("shard").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let skip_cleanup = params.get("skip_cleanup").and_then(|v| v.as_bool()).unwrap_or(false);
    let dry_run = params.get("dry_run").and_then(|v| v.as_bool()).unwrap_or(false);

    let project_root = cfg.project_root.display().to_string();

    let mut args = vec!["test-playwright".to_string()];

    if !file.is_empty() {
        args.push(file.clone());
    }

    if !grep.is_empty() {
        args.push("--grep".to_string());
        args.push(grep.clone());
    }

    if !grep_invert.is_empty() {
        args.push("--grep-invert".to_string());
        args.push(grep_invert.clone());
    }

    if let Some(w) = workers {
        args.push("--workers".to_string());
        args.push(w.to_string());
    }

    if let Some(r) = retries {
        args.push("--retries".to_string());
        args.push(r.to_string());
    }

    if headed || debug {
        args.push("--headed".to_string());
    }

    if let Some(t) = timeout_ms {
        args.push("--timeout".to_string());
        args.push(t.to_string());
    }

    if !shard.is_empty() {
        args.push("--shard".to_string());
        args.push(shard.clone());
    }

    let mut env_vars = std::collections::HashMap::new();
    if skip_cleanup {
        env_vars.insert("SKIP_TEST_CLEANUP", "true".to_string());
    }
    if debug {
        env_vars.insert("DEBUG", "1".to_string());
    }

    let cmd_display = format!("cd {} && yarn {}", project_root, args.join(" "));

    if dry_run {
        return json!({ "command": cmd_display, "dryRun": true });
    }

    let mut cmd = Command::new("yarn");
    cmd.args(&args).current_dir(&cfg.project_root);
    for (k, v) in &env_vars {
        cmd.env(k, v);
    }

    match cmd.output().await {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            json!({
                "command": cmd_display,
                "exitCode": output.status.code().unwrap_or(-1),
                "success": output.status.success(),
                "stdout": stdout.chars().take(10000).collect::<String>(),
                "stderr": stderr.chars().take(5000).collect::<String>(),
            })
        }
        Err(e) => json!({
            "error": format!("Failed to run tests: {}", e),
            "command": cmd_display,
        }),
    }
}

pub fn get_test_results(params: &Value, cfg: &Config) -> Value {
    let source = params.get("source").and_then(|v| v.as_str());
    let junit_path = cfg.junit_path();
    let allure_dir = cfg.allure_dir();

    if source == Some("junit") || (source.is_none() && junit_path.exists()) {
        return parse_junit_results(&junit_path);
    }

    if source == Some("allure") || (source.is_none() && allure_dir.exists()) {
        return parse_allure_results(&allure_dir);
    }

    json!({
        "error": "No test results found.",
        "searched": [junit_path.display().to_string(), allure_dir.display().to_string()],
        "hint": "Run tests first with run_tests, then call this tool.",
    })
}

fn parse_junit_results(path: &std::path::Path) -> Value {
    let xml = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => return json!({ "error": format!("JUnit file not found: {}", e) }),
    };

    let tests = regex_capture_u64(&xml, r#"tests="(\d+)""#);
    let failures = regex_capture_u64(&xml, r#"failures="(\d+)""#);
    let errors = regex_capture_u64(&xml, r#"errors="(\d+)""#);
    let skipped = regex_capture_u64(&xml, r#"skipped="(\d+)""#);
    let time = regex_capture_f64(&xml, r#"time="([\d.]+)""#);

    json!({
        "source": "junit",
        "file": path.display().to_string(),
        "totals": {
            "tests": tests,
            "failures": failures,
            "errors": errors,
            "skipped": skipped,
            "time": time,
        }
    })
}

fn parse_allure_results(dir: &std::path::Path) -> Value {
    let mut passed = 0u64;
    let mut failed = 0u64;
    let mut broken = 0u64;
    let mut skipped = 0u64;
    let mut failures: Vec<Value> = Vec::new();

    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.file_name().and_then(|n| n.to_str()).map(|n| n.ends_with("-result.json")).unwrap_or(false) {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Ok(data) = serde_json::from_str::<Value>(&content) {
                        let status = data.get("status").and_then(|v| v.as_str()).unwrap_or("unknown");
                        match status {
                            "passed" => passed += 1,
                            "failed" => {
                                failed += 1;
                                let msg = data.get("statusDetails").and_then(|s| s.get("message")).and_then(|v| v.as_str()).unwrap_or("").chars().take(200).collect::<String>();
                                failures.push(json!({ "name": data.get("name"), "status": "failed", "message": msg }));
                            }
                            "broken" => {
                                broken += 1;
                                let msg = data.get("statusDetails").and_then(|s| s.get("message")).and_then(|v| v.as_str()).unwrap_or("").chars().take(200).collect::<String>();
                                failures.push(json!({ "name": data.get("name"), "status": "broken", "message": msg }));
                            }
                            "skipped" => skipped += 1,
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    json!({
        "source": "allure",
        "directory": dir.display().to_string(),
        "totals": { "passed": passed, "failed": failed, "broken": broken, "skipped": skipped },
        "failures": failures.into_iter().take(20).collect::<Vec<_>>(),
    })
}

fn regex_capture_u64(text: &str, pattern: &str) -> u64 {
    regex::Regex::new(pattern)
        .ok()
        .and_then(|re| re.captures(text))
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse().ok())
        .unwrap_or(0)
}

fn regex_capture_f64(text: &str, pattern: &str) -> f64 {
    regex::Regex::new(pattern)
        .ok()
        .and_then(|re| re.captures(text))
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse().ok())
        .unwrap_or(0.0)
}
