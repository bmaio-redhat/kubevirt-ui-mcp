use serde_json::{json, Value};

use crate::coverage::scanner::ProjectScanner;

fn gh(args: &[&str]) -> Result<String, String> {
    let output = std::process::Command::new("gh")
        .args(args)
        .output()
        .map_err(|e| format!("gh CLI not found: {}", e))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

fn resolve_repo(repo: Option<&str>, default_repo: &str) -> String {
    repo.map(|r| r.to_string()).unwrap_or_else(|| default_repo.to_string())
}

pub fn get_pr_details(repo: Option<&str>, pr_number: u64, default_repo: &str) -> Value {
    let r = resolve_repo(repo, default_repo);
    match gh(&["pr", "view", &pr_number.to_string(), "--repo", &r, "--json",
        "number,title,body,state,url,baseRefName,headRefName,author,additions,deletions,changedFiles,reviews,checksUrl,statusCheckRollup"]) {
        Ok(json_str) => serde_json::from_str(&json_str).unwrap_or_else(|_| json!({ "error": "Failed to parse PR details" })),
        Err(e) => json!({ "error": format!("gh pr view failed: {}", e) }),
    }
}

pub fn get_pr_comments(repo: Option<&str>, pr_number: u64, default_repo: &str) -> Value {
    let r = resolve_repo(repo, default_repo);

    let review_comments = match gh(&["api", &format!("repos/{}/pulls/{}/comments", r, pr_number)]) {
        Ok(s) => serde_json::from_str::<Value>(&s).unwrap_or(json!([])),
        Err(e) => json!({ "error": format!("Failed to get review comments: {}", e) }),
    };

    let issue_comments = match gh(&["api", &format!("repos/{}/issues/{}/comments", r, pr_number)]) {
        Ok(s) => serde_json::from_str::<Value>(&s).unwrap_or(json!([])),
        Err(e) => json!({ "error": format!("Failed to get issue comments: {}", e) }),
    };

    json!({
        "reviewComments": review_comments,
        "issueComments": issue_comments,
    })
}

pub fn list_open_prs(repo: Option<&str>, author: Option<&str>, label: Option<&str>, limit: u32, default_repo: &str) -> Value {
    let r = resolve_repo(repo, default_repo);
    let mut args = vec![
        "pr".to_string(), "list".to_string(),
        "--repo".to_string(), r.clone(),
        "--limit".to_string(), limit.to_string(),
        "--json".to_string(), "number,title,headRefName,state,url,author,additions,deletions,reviewDecision".to_string(),
    ];
    if let Some(a) = author {
        args.push("--author".to_string());
        args.push(a.to_string());
    }
    if let Some(l) = label {
        args.push("--label".to_string());
        args.push(l.to_string());
    }

    let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    match gh(&args_ref) {
        Ok(s) => serde_json::from_str::<Value>(&s).unwrap_or_else(|_| json!({ "error": "parse error" })),
        Err(e) => json!({ "error": format!("gh pr list failed: {}", e) }),
    }
}

pub fn search_prs(repo: Option<&str>, query: &str, limit: u32, default_repo: &str) -> Value {
    let r = resolve_repo(repo, default_repo);
    let search_query = format!("repo:{} {}", r, query);
    match gh(&["search", "prs", &search_query, "--limit", &limit.to_string(),
        "--json", "number,title,state,url,author,createdAt,closedAt"]) {
        Ok(s) => serde_json::from_str::<Value>(&s).unwrap_or_else(|_| json!({ "error": "parse error" })),
        Err(e) => json!({ "error": format!("gh search prs failed: {}", e) }),
    }
}

pub fn get_pr_files_coverage(repo: Option<&str>, pr_number: u64, scanner: &mut ProjectScanner, default_repo: &str) -> Value {
    let r = resolve_repo(repo, default_repo);

    let files_json = match gh(&["pr", "view", &pr_number.to_string(), "--repo", &r,
        "--json", "files", "--jq", ".files[].path"]) {
        Ok(s) => s,
        Err(e) => return json!({ "error": format!("Could not fetch PR files: {}", e) }),
    };

    let changed_files: Vec<&str> = files_json.lines().collect();

    let playwright_files: Vec<&str> = changed_files
        .iter()
        .filter(|f| f.contains("playwright/"))
        .copied()
        .collect();

    let other_files: Vec<&str> = changed_files
        .iter()
        .filter(|f| !f.contains("playwright/"))
        .copied()
        .collect();

    let changed_specs: Vec<&str> = playwright_files
        .iter()
        .filter(|f| f.ends_with(".spec.ts"))
        .copied()
        .collect();

    let changed_page_objects: Vec<&str> = playwright_files
        .iter()
        .filter(|f| f.contains("page-objects"))
        .copied()
        .collect();

    let changed_step_drivers: Vec<&str> = playwright_files
        .iter()
        .filter(|f| f.contains("step-drivers"))
        .copied()
        .collect();

    json!({
        "prNumber": pr_number,
        "summary": {
            "totalFiles": changed_files.len(),
            "playwrightFiles": playwright_files.len(),
            "otherFiles": other_files.len(),
        },
        "playwrightChanges": {
            "specs": changed_specs,
            "pageObjects": changed_page_objects,
            "stepDrivers": changed_step_drivers,
        },
        "nonPlaywrightFiles": other_files,
    })
}
