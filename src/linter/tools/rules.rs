use regex::Regex;
use walkdir::WalkDir;

use crate::config::Config;
use crate::mcp::protocol::ToolCallResult;

/// Parse setup-rules.ts and return the rule list.
pub fn get_setup_rules(cfg: &Config) -> ToolCallResult {
    let path = cfg.playwright_root().join("project-dependencies/rule-engine/setup-rules.ts");

    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            return ToolCallResult::error(format!("Cannot read {}: {}", path.display(), e))
        }
    };

    let rules = extract_rules(&content, "setup");
    if rules.is_empty() {
        return ToolCallResult::text(format!(
            "No setup rules found in {}. File may have an unexpected format.",
            path.display()
        ));
    }

    ToolCallResult::text(format!(
        "## Setup rules ({})\n\nSource: {}\n\n{}",
        rules.len(),
        path.display(),
        rules.join("\n")
    ))
}

/// Parse teardown modules and return all teardown rules.
pub fn get_teardown_rules(cfg: &Config) -> ToolCallResult {
    let teardown_dir = cfg.playwright_root().join("project-dependencies");

    let mut all_rules = Vec::new();

    for entry in WalkDir::new(&teardown_dir)
        .max_depth(3)
        .into_iter()
        .flatten()
        .filter(|e| {
            e.file_type().is_file()
                && e.file_name().to_string_lossy().contains("teardown")
                && e.file_name().to_string_lossy().ends_with(".ts")
        })
    {
        let content = match std::fs::read_to_string(entry.path()) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let rules = extract_rules(&content, "teardown");
        if !rules.is_empty() {
            all_rules.push(format!(
                "### {}\n{}",
                entry.path().file_name().unwrap_or_default().to_string_lossy(),
                rules.join("\n")
            ));
        }
    }

    if all_rules.is_empty() {
        ToolCallResult::text(format!(
            "No teardown rules found under {}.",
            teardown_dir.display()
        ))
    } else {
        ToolCallResult::text(format!(
            "## Teardown rules\n\n{}",
            all_rules.join("\n\n")
        ))
    }
}

fn extract_rules(content: &str, _rule_type: &str) -> Vec<String> {
    let mut rules = Vec::new();

    // Match { id: 'something', ... description: 'text' } style objects
    let id_re = Regex::new(r#"id:\s*['"]([^'"]+)['"]"#).unwrap();
    let desc_re = Regex::new(r#"description:\s*['"]([^'"]+)['"]"#).unwrap();
    let phase_re = Regex::new(r#"phase:\s*['"]?([A-Z_]+)['"]?"#).unwrap();

    // Simple block extraction: split on { and find rule-like objects
    let block_re = Regex::new(r#"\{[^{}]*id:\s*['"][^'"]+['"][^{}]*\}"#).unwrap();
    for m in block_re.find_iter(content) {
        let block = m.as_str();
        let id = id_re.captures(block).map(|c| c[1].to_string()).unwrap_or_else(|| "(no id)".into());
        let desc = desc_re.captures(block).map(|c| c[1].to_string()).unwrap_or_else(|| "(no description)".into());
        let phase = phase_re.captures(block).map(|c| c[1].to_string()).unwrap_or_else(|| "(no phase)".into());
        rules.push(format!("  - [{}] {} — {}", phase, id, desc));
    }

    // Fallback: just extract id values if block approach finds nothing
    if rules.is_empty() {
        for cap in id_re.captures_iter(content) {
            rules.push(format!("  - {}", &cap[1]));
        }
    }

    rules
}
