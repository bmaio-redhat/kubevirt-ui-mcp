use serde_json::Value;
use tokio::time::{Duration, sleep};
use tracing::{info, warn};

use crate::memory::store::TicketRecord;

pub async fn fetch_ticket(
    client: &reqwest::Client,
    base_url: &str,
    key: &str,
) -> Option<TicketRecord> {
    // Small delay to avoid hammering the public API
    sleep(Duration::from_millis(50)).await;

    let url = format!("{}/rest/api/3/issue/{}", base_url, key);

    let resp = match client.get(&url).send().await {
        Ok(r) => r,
        Err(e) => {
            warn!("Jira request failed for {}: {}", key, e);
            return None;
        }
    };

    if !resp.status().is_success() {
        warn!("Jira returned {} for {}", resp.status(), key);
        return None;
    }

    let body: Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            warn!("Failed to parse Jira response for {}: {}", key, e);
            return None;
        }
    };

    let fields = body.get("fields")?;

    let summary = str_field(fields, "summary");
    let status = fields
        .get("status")
        .and_then(|v| v.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let issue_type = fields
        .get("issuetype")
        .and_then(|v| v.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let labels = string_array(fields, "labels");
    let components = named_array(fields, "components");
    let fix_versions = named_array(fields, "fixVersions");

    let description_excerpt = fields
        .get("description")
        .map(|d| extract_adf_text(d))
        .unwrap_or_default()
        .chars()
        .take(500)
        .collect();

    info!("Fetched {}: {}", key, summary);

    Some(TicketRecord {
        key: key.to_string(),
        summary,
        status,
        issue_type,
        labels,
        components,
        fix_versions,
        description_excerpt,
        commit_shas: vec![],
        fetched_at: chrono::Utc::now(),
    })
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn str_field(fields: &Value, key: &str) -> String {
    fields
        .get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

fn string_array(fields: &Value, key: &str) -> Vec<String> {
    fields
        .get(key)
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default()
}

fn named_array(fields: &Value, key: &str) -> Vec<String> {
    fields
        .get(key)
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.get("name"))
                .filter_map(|v| v.as_str())
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// Recursively extract plain text from Atlassian Document Format (ADF).
fn extract_adf_text(node: &Value) -> String {
    let mut out = String::new();

    if let Some(text) = node.get("text").and_then(|v| v.as_str()) {
        out.push_str(text);
        out.push(' ');
    }

    if let Some(content) = node.get("content").and_then(|v| v.as_array()) {
        for child in content {
            out.push_str(&extract_adf_text(child));
        }
        // Add a newline after block-level nodes
        let node_type = node.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if matches!(node_type, "paragraph" | "codeBlock" | "heading" | "listItem" | "bulletList" | "orderedList") {
            out.push('\n');
        }
    }

    out
}
