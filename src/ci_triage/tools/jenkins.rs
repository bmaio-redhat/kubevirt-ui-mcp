use serde_json::Value;
use std::sync::Arc;

use crate::config::Config;
use crate::mcp::protocol::ToolCallResult;
use crate::ci_triage::tools::junit::{TestResult, TestStatus, classname_to_spec, merge_quarantined};

/// Parse a Jenkins testReport/api/json response into the same TestResult schema.
pub async fn parse_jenkins(
    params: &Value,
    cfg: &Config,
    client: &Arc<reqwest::Client>,
) -> ToolCallResult {
    let source = match params.get("source").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return ToolCallResult::error("Missing required parameter: source"),
    };

    let json_val: Value = if source.starts_with("http://") || source.starts_with("https://") {
        // Fetch from Jenkins
        let url = normalize_jenkins_url(source);
        let mut req = client.get(&url);

        if let (Some(user), Some(token)) = (&cfg.jenkins_user, &cfg.jenkins_token) {
            req = req.basic_auth(user, Some(token));
        }

        match req.send().await {
            Ok(resp) => {
                let status = resp.status();
                match resp.json::<Value>().await {
                    Ok(v) => v,
                    Err(e) => {
                        return ToolCallResult::error(format!(
                            "Failed to parse Jenkins JSON response (HTTP {}): {}",
                            status, e
                        ))
                    }
                }
            }
            Err(e) => return ToolCallResult::error(format!("HTTP request failed: {}", e)),
        }
    } else {
        // Read from local file
        match std::fs::read_to_string(source) {
            Ok(content) => match serde_json::from_str::<Value>(&content) {
                Ok(v) => v,
                Err(e) => return ToolCallResult::error(format!("JSON parse error: {}", e)),
            },
            Err(e) => {
                return ToolCallResult::error(format!("Cannot read file {}: {}", source, e))
            }
        }
    };

    match parse_jenkins_json(&json_val) {
        Ok(results) => {
            let merged = merge_quarantined(&results);
            let failures: Vec<_> = merged
                .iter()
                .filter(|r| {
                    r.status == TestStatus::Failed
                        || r.status == TestStatus::Error
                        || (r.status == TestStatus::Skipped && r.is_quarantined)
                })
                .collect();

            let json_out = serde_json::to_string_pretty(&failures)
                .unwrap_or_else(|e| format!("Serialization error: {}", e));

            ToolCallResult::text(format!(
                "Jenkins report parsed: {} total tests, {} actionable failures (after merging quarantined).\n\n{}",
                results.len(),
                failures.len(),
                json_out
            ))
        }
        Err(e) => ToolCallResult::error(e),
    }
}

/// Append /testReport/api/json to a Jenkins build URL if not already present.
fn normalize_jenkins_url(url: &str) -> String {
    let trimmed = url.trim_end_matches('/');
    if trimmed.ends_with("/api/json") {
        trimmed.to_string()
    } else if trimmed.ends_with("/testReport") {
        format!("{}/api/json", trimmed)
    } else {
        format!("{}/testReport/api/json", trimmed)
    }
}

/// Parse Jenkins testReport/api/json structure into TestResult vec.
fn parse_jenkins_json(json: &Value) -> Result<Vec<TestResult>, String> {
    let suites = json
        .get("suites")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "Jenkins JSON missing 'suites' array".to_string())?;

    let mut results = Vec::new();

    for suite in suites {
        let cases = match suite.get("cases").and_then(|v| v.as_array()) {
            Some(c) => c,
            None => continue,
        };

        for case in cases {
            let classname = case
                .get("className")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let test_name = case
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let duration_secs = case
                .get("duration")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let status_str = case
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("PASSED");
            let error_details = case
                .get("errorDetails")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let status = match status_str {
                "FAILED" => TestStatus::Failed,
                "ERROR" => TestStatus::Error,
                "SKIPPED" => TestStatus::Skipped,
                _ => TestStatus::Passed,
            };

            let is_quarantined = test_name.contains("Quarantined:");
            let spec_path = classname_to_spec(&classname);

            let jira_re = regex::Regex::new(r"ID\((CNV-\d+)\)").unwrap();
            let jira_ids: Vec<String> = jira_re
                .captures_iter(&test_name)
                .map(|c| c[1].to_string())
                .collect();

            results.push(TestResult {
                classname,
                test_name,
                spec_path,
                jira_ids,
                status,
                error_message: error_details,
                is_quarantined,
                duration_secs,
            });
        }
    }

    Ok(results)
}
