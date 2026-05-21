use std::path::Path;

use serde_json::Value;
use walkdir::WalkDir;

use crate::config::Config;
use crate::mcp::protocol::ToolCallResult;

#[derive(Debug, serde::Serialize)]
pub struct AllureFailure {
    pub name: String,
    pub suite: Option<String>,
    pub status: String,
    pub error_message: Option<String>,
    pub attachments: Vec<String>,
}

/// Scan an allure-results directory and return failed test details.
pub fn get_allure_failures(params: &Value, cfg: &Config) -> ToolCallResult {
    let allure_dir = params
        .get("allure_dir")
        .and_then(|v| v.as_str())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| cfg.allure_dir());

    if !allure_dir.exists() {
        return ToolCallResult::error(format!(
            "allure-results directory not found: {}",
            allure_dir.display()
        ));
    }

    let failures = scan_allure_dir(&allure_dir);

    if failures.is_empty() {
        return ToolCallResult::text(format!(
            "No failed tests found in {}",
            allure_dir.display()
        ));
    }

    let json = serde_json::to_string_pretty(&failures)
        .unwrap_or_else(|e| format!("Serialization error: {}", e));

    ToolCallResult::text(format!(
        "{} failed tests in {}:\n\n{}",
        failures.len(),
        allure_dir.display(),
        json
    ))
}

fn scan_allure_dir(dir: &Path) -> Vec<AllureFailure> {
    let mut failures = Vec::new();

    // Allure stores one JSON file per test result with names like UUID-result.json
    for entry in WalkDir::new(dir)
        .max_depth(2)
        .into_iter()
        .flatten()
        .filter(|e| {
            e.file_type().is_file()
                && e.file_name().to_string_lossy().ends_with("-result.json")
        })
    {
        let content = match std::fs::read_to_string(entry.path()) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let json: Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let status = json
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        if status != "failed" && status != "broken" {
            continue;
        }

        let name = json
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("(unnamed)")
            .to_string();

        let suite = json
            .get("labels")
            .and_then(|v| v.as_array())
            .and_then(|labels| {
                labels
                    .iter()
                    .find(|l| l.get("name").and_then(|n| n.as_str()) == Some("suite"))
                    .and_then(|l| l.get("value").and_then(|v| v.as_str()))
            })
            .map(|s| s.to_string());

        let error_message = json
            .get("statusDetails")
            .and_then(|v| v.get("message"))
            .and_then(|v| v.as_str())
            .map(|s| s.chars().take(500).collect::<String>());

        // Collect attachment file paths
        let attachments: Vec<String> = json
            .get("attachments")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|a| a.get("source").and_then(|s| s.as_str()))
                    .map(|src| dir.join(src).to_string_lossy().to_string())
                    .collect()
            })
            .unwrap_or_default();

        failures.push(AllureFailure { name, suite, status, error_message, attachments });
    }

    failures.sort_by(|a, b| a.name.cmp(&b.name));
    failures
}
