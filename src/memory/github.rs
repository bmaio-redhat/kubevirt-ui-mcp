use std::collections::HashMap;

use regex::Regex;
use serde::Deserialize;
use tracing::{info, warn};

#[derive(Debug, Deserialize)]
struct CommitResponse {
    sha: String,
    commit: CommitData,
}

#[derive(Debug, Deserialize)]
struct CommitData {
    message: String,
}

/// Returns a map of CNV ticket key → list of commit SHAs that reference it.
pub async fn fetch_ticket_commits(
    client: &reqwest::Client,
    repo: &str,
    pages: u32,
) -> HashMap<String, Vec<String>> {
    let cnv_re = Regex::new(r"CNV-(\d+)").expect("valid regex");
    let mut result: HashMap<String, Vec<String>> = HashMap::new();

    for page in 1..=pages {
        let url = format!(
            "https://api.github.com/repos/{}/commits?per_page=100&page={}",
            repo, page
        );

        let resp = match client.get(&url).send().await {
            Ok(r) => r,
            Err(e) => {
                warn!("GitHub request failed (page {}): {}", page, e);
                break;
            }
        };

        if resp.status() == 404 {
            warn!("GitHub repo not found: {}", repo);
            break;
        }

        // Stop if we've gone past the last page
        let has_next = resp
            .headers()
            .get("link")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.contains(r#"rel="next""#))
            .unwrap_or(false);

        let commits: Vec<CommitResponse> = match resp.json().await {
            Ok(v) => v,
            Err(e) => {
                warn!("Failed to parse commits page {}: {}", page, e);
                break;
            }
        };

        info!("GitHub page {}: {} commits", page, commits.len());

        for commit in commits {
            for cap in cnv_re.captures_iter(&commit.commit.message) {
                let key = format!("CNV-{}", &cap[1]);
                result.entry(key).or_default().push(commit.sha.clone());
            }
        }

        if !has_next {
            break;
        }
    }

    info!(
        "Found {} unique CNV ticket references across commit history",
        result.len()
    );
    result
}
