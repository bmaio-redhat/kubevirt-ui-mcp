use std::collections::BTreeSet;

use regex::Regex;
use walkdir::WalkDir;

use crate::config::Config;
use crate::mcp::protocol::ToolCallResult;

/// Check that every ID(CNV-*) in spec files has a matching STD doc entry.
pub fn validate_std_coverage(cfg: &Config) -> ToolCallResult {
    let tests_dir = cfg.playwright_root().join("tests");
    let docs_dir = cfg.playwright_root().join("docs");

    let jira_re = Regex::new(r"ID\((CNV-\d+)\)").unwrap();

    // Collect all Jira IDs found in spec files
    let mut spec_ids: BTreeSet<String> = BTreeSet::new();
    for entry in WalkDir::new(&tests_dir)
        .max_depth(6)
        .into_iter()
        .flatten()
        .filter(|e| e.file_type().is_file() && e.file_name().to_string_lossy().ends_with(".spec.ts"))
    {
        let content = match std::fs::read_to_string(entry.path()) {
            Ok(c) => c,
            Err(_) => continue,
        };
        for cap in jira_re.captures_iter(&content) {
            spec_ids.insert(cap[1].to_string());
        }
    }

    // Collect all Jira IDs found in STD docs
    let mut doc_ids: BTreeSet<String> = BTreeSet::new();
    if docs_dir.exists() {
        for entry in WalkDir::new(&docs_dir)
            .max_depth(6)
            .into_iter()
            .flatten()
            .filter(|e| {
                e.file_type().is_file() && e.file_name().to_string_lossy().ends_with(".md")
            })
        {
            let content = match std::fs::read_to_string(entry.path()) {
                Ok(c) => c,
                Err(_) => continue,
            };
            // STDs reference IDs as CNV-XXXXX without the ID() wrapper
            let doc_jira_re = Regex::new(r"\bCNV-\d+\b").unwrap();
            for cap in doc_jira_re.find_iter(&content) {
                doc_ids.insert(cap.as_str().to_string());
            }
        }
    }

    let missing: Vec<&String> = spec_ids.iter().filter(|id| !doc_ids.contains(*id)).collect();
    let extra: Vec<&String> = doc_ids.iter().filter(|id| !spec_ids.contains(*id)).collect();

    let mut out = format!(
        "## STD Coverage Report\n\n\
        - Jira IDs in specs: {}\n\
        - Jira IDs in docs: {}\n\
        - Missing from docs: {}\n\
        - In docs but not in specs: {}\n\n",
        spec_ids.len(),
        doc_ids.len(),
        missing.len(),
        extra.len()
    );

    if !missing.is_empty() {
        out.push_str("### IDs in specs with no STD doc entry\n");
        for id in &missing {
            out.push_str(&format!("  - {}\n", id));
        }
        out.push('\n');
    }

    if !extra.is_empty() {
        out.push_str("### IDs in docs with no spec (docs may be stale)\n");
        for id in extra.iter().take(20) {
            out.push_str(&format!("  - {}\n", id));
        }
        if extra.len() > 20 {
            out.push_str(&format!("  ... and {} more\n", extra.len() - 20));
        }
    }

    if missing.is_empty() && extra.is_empty() {
        out.push_str("All spec Jira IDs are covered in STD docs.");
    }

    ToolCallResult::text(out)
}

/// Check UI tier1 specs for console-proxy writes without mirrored API specs.
pub fn check_api_ui_parity(cfg: &Config) -> ToolCallResult {
    let tier1_dir = cfg.playwright_root().join("tests/tier1");
    let api_dir = cfg.playwright_root().join("tests/api");

    if !tier1_dir.exists() {
        return ToolCallResult::error(format!(
            "playwright/tests/tier1/ not found at {}",
            tier1_dir.display()
        ));
    }

    // Patterns that indicate a console-proxy write call
    let proxy_write_re =
        Regex::new(r"\b(post|put|patch|delete)\s*\(|apiClient\.(create|update|patch|delete|put|post)")
            .unwrap();

    // Collect API spec file names (without extension) for cross-reference
    let api_specs: BTreeSet<String> = if api_dir.exists() {
        WalkDir::new(&api_dir)
            .max_depth(4)
            .into_iter()
            .flatten()
            .filter(|e| {
                e.file_type().is_file() && e.file_name().to_string_lossy().ends_with(".spec.ts")
            })
            .map(|e| {
                e.path()
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default()
            })
            .collect()
    } else {
        BTreeSet::new()
    };

    let mut missing_parity: Vec<String> = Vec::new();

    for entry in WalkDir::new(&tier1_dir)
        .max_depth(5)
        .into_iter()
        .flatten()
        .filter(|e| e.file_type().is_file() && e.file_name().to_string_lossy().ends_with(".spec.ts"))
    {
        let content = match std::fs::read_to_string(entry.path()) {
            Ok(c) => c,
            Err(_) => continue,
        };

        if !proxy_write_re.is_match(&content) {
            continue;
        }

        // Check if there's a matching API spec
        let stem = entry
            .path()
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();

        // Remove .spec suffix for matching
        let base = stem.trim_end_matches(".spec").to_string();

        let has_mirror = api_specs.iter().any(|api| {
            let api_base = api.trim_end_matches(".spec");
            api_base.contains(&base) || base.contains(api_base)
        });

        if !has_mirror {
            let rel = entry
                .path()
                .strip_prefix(cfg.playwright_root())
                .ok()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();
            missing_parity.push(rel);
        }
    }

    if missing_parity.is_empty() {
        ToolCallResult::text(
            "All tier1 specs with console-proxy writes appear to have a mirrored API spec.",
        )
    } else {
        ToolCallResult::text(format!(
            "## API/UI Parity Gaps\n\n\
            {} tier1 spec(s) make console-proxy writes but have no mirrored API spec:\n\n{}",
            missing_parity.len(),
            missing_parity.iter().map(|s| format!("  - {}", s)).collect::<Vec<_>>().join("\n")
        ))
    }
}
