use std::path::Path;

use regex::Regex;
use serde_json::Value;

use crate::config::Config;
use crate::mcp::protocol::ToolCallResult;

#[derive(Debug, serde::Serialize)]
pub struct LintViolation {
    pub line: usize,
    pub rule: String,
    pub message: String,
    pub snippet: String,
}

pub fn lint_spec_file(params: &Value, cfg: &Config) -> ToolCallResult {
    let path_str = match params.get("path").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => return ToolCallResult::error("Missing required parameter: path"),
    };

    let path = {
        let p = Path::new(path_str);
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            cfg.project_root.join(path_str)
        }
    };

    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => return ToolCallResult::error(format!("Cannot read {}: {}", path.display(), e)),
    };

    let violations = run_lint_rules(&content, &path);

    if violations.is_empty() {
        return ToolCallResult::text(format!(
            "No violations found in {} — file passes all lint rules.",
            path.display()
        ));
    }

    let json = serde_json::to_string_pretty(&violations)
        .unwrap_or_else(|e| format!("Serialization error: {}", e));

    ToolCallResult::text(format!(
        "{} violation(s) in {}:\n\n{}",
        violations.len(),
        path.display(),
        json
    ))
}

fn run_lint_rules(content: &str, path: &Path) -> Vec<LintViolation> {
    let mut violations = Vec::new();
    let lines: Vec<&str> = content.lines().collect();

    let is_spec = path.to_string_lossy().ends_with(".spec.ts");

    // Precompile patterns
    let raw_page_re = Regex::new(r"\bpage\.(click|fill|goto|locator|getBy|waitFor)\b").unwrap();
    let hardcoded_timeout_re =
        Regex::new(r"timeout:\s*\d{4,}").unwrap();
    let jira_id_re = Regex::new(r#"test\.step\s*\("#).unwrap();
    let jira_annotation_re = Regex::new(r"ID\((CNV-\d+)\)").unwrap();
    let create_resource_re =
        Regex::new(r"\b(create|apply|post)\b.*\b(vm|datavolume|snapshot|template)\b").unwrap();
    let cleanup_track_re = Regex::new(r"cleanup\.track\(").unwrap();

    // Rule: raw page usage in spec (should go through page objects)
    if is_spec {
        for (i, line) in lines.iter().enumerate() {
            if raw_page_re.is_match(line) && !line.trim_start().starts_with("//") {
                violations.push(LintViolation {
                    line: i + 1,
                    rule: "no-raw-page".into(),
                    message: "Direct 'page' usage in spec — use a page object instead".into(),
                    snippet: line.trim().chars().take(120).collect(),
                });
            }
        }
    }

    // Rule: hardcoded timeouts
    for (i, line) in lines.iter().enumerate() {
        if hardcoded_timeout_re.is_match(line) && !line.contains("TestTimeouts") {
            violations.push(LintViolation {
                line: i + 1,
                rule: "use-test-timeouts".into(),
                message: "Hardcoded numeric timeout — use TestTimeouts constants instead".into(),
                snippet: line.trim().chars().take(120).collect(),
            });
        }
    }

    // Rule: test.step calls should have an ID(CNV-*) annotation somewhere nearby
    if is_spec {
        let full = content;
        let has_any_jira = jira_annotation_re.is_match(full);
        let has_any_test_step = jira_id_re.is_match(full);
        if has_any_test_step && !has_any_jira {
            violations.push(LintViolation {
                line: 0,
                rule: "require-jira-id".into(),
                message: "Spec contains test.step() calls but no ID(CNV-XXXXX) Jira annotations. Add ID() within test.step() calls for traceability.".into(),
                snippet: "(whole file)".into(),
            });
        }
    }

    // Rule: resource creation without cleanup.track
    if is_spec {
        let has_create = create_resource_re.is_match(content);
        let has_cleanup = cleanup_track_re.is_match(content);
        if has_create && !has_cleanup {
            violations.push(LintViolation {
                line: 0,
                rule: "require-cleanup-track".into(),
                message: "Spec appears to create KubeVirt resources but has no cleanup.track() calls. Resources may leak after test failure.".into(),
                snippet: "(whole file)".into(),
            });
        }
    }

    // Rule: no import from non-barrel paths (should use @/ aliases)
    let deep_import_re =
        Regex::new(r#"from\s+['"][^'"]*/(page-objects|clients|fixtures)/[^'"]+['"]\s*;"#).unwrap();
    for (i, line) in lines.iter().enumerate() {
        if deep_import_re.is_match(line) && !line.trim_start().starts_with("//") {
            violations.push(LintViolation {
                line: i + 1,
                rule: "use-barrel-imports".into(),
                message: "Deep import path detected — consider using barrel index exports".into(),
                snippet: line.trim().chars().take(120).collect(),
            });
        }
    }

    violations
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn spec_path(name: &str) -> std::path::PathBuf {
        std::path::PathBuf::from(format!("playwright/tests/tier1/{}", name))
    }

    #[test]
    fn detects_raw_page_usage_in_spec() {
        let content = "await page.click('[data-test=start]');";
        let v = run_lint_rules(content, &spec_path("vm.spec.ts"));
        assert!(v.iter().any(|v| v.rule == "no-raw-page"));
    }

    #[test]
    fn does_not_flag_page_object_files() {
        let content = "await this.page.click('[data-test=start]');";
        let v = run_lint_rules(content, Path::new("playwright/src/page-objects/vm-page.ts"));
        assert!(!v.iter().any(|v| v.rule == "no-raw-page"));
    }

    #[test]
    fn detects_hardcoded_timeout() {
        let content = "await element.waitFor({ timeout: 30000 });";
        let v = run_lint_rules(content, &spec_path("vm.spec.ts"));
        assert!(v.iter().any(|v| v.rule == "use-test-timeouts"));
    }

    #[test]
    fn allows_test_timeouts_constant() {
        let content = "await element.waitFor({ timeout: TestTimeouts.VM_RUNNING });";
        let v = run_lint_rules(content, &spec_path("vm.spec.ts"));
        assert!(!v.iter().any(|v| v.rule == "use-test-timeouts"));
    }

    #[test]
    fn detects_missing_jira_id_when_test_step_present() {
        let content = "await test.step('Create VM', async () => { /* no ID */ });";
        let v = run_lint_rules(content, &spec_path("vm.spec.ts"));
        assert!(v.iter().any(|v| v.rule == "require-jira-id"));
    }

    #[test]
    fn no_violation_when_jira_id_present() {
        let content = r#"await test.step('ID(CNV-12345) Create VM', async () => {});"#;
        let v = run_lint_rules(content, &spec_path("vm.spec.ts"));
        assert!(!v.iter().any(|v| v.rule == "require-jira-id"));
    }

    #[test]
    fn empty_file_has_no_violations() {
        let v = run_lint_rules("", &spec_path("empty.spec.ts"));
        assert!(v.is_empty());
    }

    #[test]
    fn line_numbers_are_correct() {
        let content = "line1\nline2\nawait page.click('x');\nline4";
        let v = run_lint_rules(content, &spec_path("vm.spec.ts"));
        let raw_page = v.iter().find(|v| v.rule == "no-raw-page");
        assert!(raw_page.is_some());
        assert_eq!(raw_page.unwrap().line, 3);
    }

    #[test]
    fn commented_raw_page_not_flagged() {
        let content = "// await page.click('x'); // this is commented out";
        let v = run_lint_rules(content, &spec_path("vm.spec.ts"));
        assert!(!v.iter().any(|v| v.rule == "no-raw-page"));
    }
}
