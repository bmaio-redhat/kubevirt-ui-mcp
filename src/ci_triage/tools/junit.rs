use std::path::Path;

use quick_xml::events::Event;
use quick_xml::reader::Reader;
use regex::Regex;
use serde::Serialize;

/// A single test case result parsed from JUnit XML.
#[derive(Debug, Clone, Serialize)]
pub struct TestResult {
    pub classname: String,
    pub test_name: String,
    pub spec_path: Option<String>,
    pub jira_ids: Vec<String>,
    pub status: TestStatus,
    pub error_message: Option<String>,
    pub is_quarantined: bool,
    pub duration_secs: f64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TestStatus {
    Passed,
    Failed,
    Skipped,
    Error,
}

/// Parse a JUnit XML file and return all test results.
pub fn parse_file(path: &Path) -> Result<Vec<TestResult>, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Cannot read {}: {}", path.display(), e))?;
    parse_xml(&content)
}

/// Parse JUnit XML content string.
pub fn parse_xml(xml: &str) -> Result<Vec<TestResult>, String> {
    let jira_re = Regex::new(r"ID\((CNV-\d+)\)").unwrap();
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut results = Vec::new();
    let mut current: Option<PartialResult> = None;
    let mut capture_text = false;
    let mut text_buf = String::new();
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Empty(ref e)) => {
                // Self-closing tags: <testcase .../>, <failure .../>, <skipped .../>
                let tag = std::str::from_utf8(e.name().as_ref()).unwrap_or("").to_lowercase();
                match tag.as_str() {
                    "testcase" => {
                        // A self-closing testcase is a passing test — push immediately
                        let classname = attr_str(e, "classname");
                        let name = attr_str(e, "name");
                        let time: f64 = attr_str(e, "time").parse().unwrap_or(0.0);
                        let jira_ids: Vec<String> = jira_re
                            .captures_iter(&name)
                            .map(|c| c[1].to_string())
                            .collect();
                        let spec_path = classname_to_spec(&classname);
                        let is_quarantined = name.contains("Quarantined:");
                        results.push(TestResult {
                            classname,
                            test_name: name,
                            spec_path,
                            jira_ids,
                            status: TestStatus::Passed,
                            error_message: None,
                            is_quarantined,
                            duration_secs: time,
                        });
                    }
                    "failure" | "error" if current.is_some() => {
                        let msg = attr_str(e, "message");
                        if let Some(ref mut p) = current {
                            p.status = if tag == "failure" { TestStatus::Failed } else { TestStatus::Error };
                            if !msg.is_empty() { p.error_message = Some(msg); }
                        }
                    }
                    "skipped" if current.is_some() => {
                        let msg = attr_str(e, "message");
                        if let Some(ref mut p) = current {
                            p.status = TestStatus::Skipped;
                            if !msg.is_empty() { p.error_message = Some(msg); }
                        }
                    }
                    _ => {}
                }
            }

            Ok(Event::Start(ref e)) => {
                let tag = std::str::from_utf8(e.name().as_ref()).unwrap_or("").to_lowercase();
                match tag.as_str() {
                    "testcase" => {
                        let classname = attr_str(e, "classname");
                        let name = attr_str(e, "name");
                        let time: f64 = attr_str(e, "time").parse().unwrap_or(0.0);
                        let jira_ids: Vec<String> = jira_re
                            .captures_iter(&name)
                            .map(|c| c[1].to_string())
                            .collect();
                        let spec_path = classname_to_spec(&classname);
                        let is_quarantined = name.contains("Quarantined:");
                        current = Some(PartialResult {
                            classname,
                            test_name: name,
                            spec_path,
                            jira_ids,
                            status: TestStatus::Passed,
                            error_message: None,
                            is_quarantined,
                            duration_secs: time,
                        });
                        capture_text = false;
                        text_buf.clear();
                    }
                    "failure" | "error" if current.is_some() => {
                        let msg = attr_str(e, "message");
                        if let Some(ref mut p) = current {
                            p.status = if tag == "failure" { TestStatus::Failed } else { TestStatus::Error };
                            if !msg.is_empty() { p.error_message = Some(msg); }
                        }
                        capture_text = true;
                        text_buf.clear();
                    }
                    "skipped" if current.is_some() => {
                        let msg = attr_str(e, "message");
                        if let Some(ref mut p) = current {
                            p.status = TestStatus::Skipped;
                            if !msg.is_empty() { p.error_message = Some(msg); }
                        }
                    }
                    _ => {}
                }
            }

            Ok(Event::Text(e)) if capture_text => {
                if let Ok(t) = e.unescape() {
                    text_buf.push_str(&t);
                }
            }

            Ok(Event::End(ref e)) => {
                let tag = std::str::from_utf8(e.name().as_ref()).unwrap_or("").to_lowercase();
                match tag.as_str() {
                    "failure" | "error" if capture_text => {
                        capture_text = false;
                        if let Some(ref mut p) = current {
                            if !text_buf.trim().is_empty() {
                                // Prefer the body over the message attr if richer
                                let body = text_buf.trim().to_string();
                                if p.error_message.as_deref().map(|m| m.len()).unwrap_or(0)
                                    < body.len()
                                {
                                    p.error_message = Some(body);
                                }
                            }
                        }
                        text_buf.clear();
                    }
                    "testcase" => {
                        if let Some(p) = current.take() {
                            results.push(TestResult {
                                classname: p.classname,
                                test_name: p.test_name,
                                spec_path: p.spec_path,
                                jira_ids: p.jira_ids,
                                status: p.status,
                                error_message: p.error_message,
                                is_quarantined: p.is_quarantined,
                                duration_secs: p.duration_secs,
                            });
                        }
                    }
                    _ => {}
                }
            }

            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("XML parse error: {}", e)),
            _ => {}
        }
        buf.clear();
    }

    Ok(results)
}

/// Attempt to map a JUnit classname → playwright/tests/... relative path.
/// Jenkins encodes the path as e.g. "tier1 › checkups › checkups.spec.ts"
/// or as a dot-separated classname.
pub fn classname_to_spec(classname: &str) -> Option<String> {
    if classname.is_empty() {
        return None;
    }

    // Arrow-separated (Jenkins Playwright reporter style)
    if classname.contains('›') {
        let parts: Vec<&str> = classname.split('›').map(|s| s.trim()).collect();
        if let Some(last) = parts.last() {
            if last.ends_with(".spec.ts") {
                let dir_parts = &parts[..parts.len() - 1];
                let path = format!(
                    "playwright/tests/{}/{}",
                    dir_parts.join("/"),
                    last
                );
                return Some(path);
            }
        }
    }

    // Already looks like a path
    if classname.contains(".spec.ts") {
        return Some(classname.to_string());
    }

    None
}

/// Merge FAILED tests with quarantined SKIPPED entries that correspond to the
/// same test (CI Safe Mode wraps failures as SKIPPED "Quarantined: <original_name>").
pub fn merge_quarantined(results: &[TestResult]) -> Vec<TestResult> {
    let mut merged: Vec<TestResult> = Vec::new();

    // Index quarantined skips by their "inner" name (after stripping the prefix)
    let quarantined: std::collections::HashMap<String, &TestResult> = results
        .iter()
        .filter(|r| r.is_quarantined && r.status == TestStatus::Skipped)
        .map(|r| {
            let inner = r
                .test_name
                .trim_start_matches("Quarantined:")
                .trim()
                .to_string();
            (inner, r)
        })
        .collect();

    for result in results {
        if result.is_quarantined && result.status == TestStatus::Skipped {
            // Will be surfaced via the FAILED counterpart; skip standalone
            continue;
        }

        let mut out = result.clone();

        // If this failed test has a quarantined twin, recover the hidden message
        if out.status == TestStatus::Failed || out.status == TestStatus::Error {
            if let Some(q) = quarantined.get(&out.test_name) {
                if out.error_message.is_none() {
                    out.error_message = q.error_message.clone();
                }
                out.is_quarantined = true;
            }
        }

        merged.push(out);
    }

    // Also add quarantined skips that have no FAILED counterpart (pure CI-safe skips)
    for result in results {
        if result.is_quarantined && result.status == TestStatus::Skipped {
            let inner = result
                .test_name
                .trim_start_matches("Quarantined:")
                .trim()
                .to_string();
            let already_included = merged.iter().any(|r| r.test_name == inner);
            if !already_included {
                merged.push(result.clone());
            }
        }
    }

    merged
}

// ── Internal helpers ──────────────────────────────────────────────────────────

struct PartialResult {
    classname: String,
    test_name: String,
    spec_path: Option<String>,
    jira_ids: Vec<String>,
    status: TestStatus,
    error_message: Option<String>,
    is_quarantined: bool,
    duration_secs: f64,
}

fn attr_str(e: &quick_xml::events::BytesStart, name: &str) -> String {
    e.attributes()
        .flatten()
        .find(|a| a.key.as_ref() == name.as_bytes())
        .and_then(|a| a.unescape_value().ok())
        .map(|v| v.into_owned())
        .unwrap_or_default()
}

// ── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── parse_xml ─────────────────────────────────────────────────────────────

    const BASIC_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuites>
  <testsuite name="E2E Tests" tests="3" failures="1" skipped="1">
    <testcase classname="tier1 › checkups › checkups.spec.ts" name="should create checkup ID(CNV-12345)" time="5.2">
    </testcase>
    <testcase classname="tier1 › vm › vm-actions.spec.ts" name="should start VM ID(CNV-99999)" time="12.0">
      <failure message="Expected element to be visible">Locator not found: data-test=start-vm</failure>
    </testcase>
    <testcase classname="tier1 › snapshots › snapshots.spec.ts" name="Quarantined: should restore snapshot ID(CNV-55555)" time="0.0">
      <skipped message="Quarantined by CI Safe Mode"/>
    </testcase>
  </testsuite>
</testsuites>"#;

    #[test]
    fn parses_passed_test() {
        let results = parse_xml(BASIC_XML).unwrap();
        let passed = results.iter().find(|r| r.test_name.contains("checkup")).unwrap();
        assert_eq!(passed.status, TestStatus::Passed);
        assert_eq!(passed.jira_ids, vec!["CNV-12345"]);
        assert!(passed.spec_path.as_deref().unwrap().contains("checkups.spec.ts"));
    }

    #[test]
    fn parses_failed_test_with_message() {
        let results = parse_xml(BASIC_XML).unwrap();
        let failed = results.iter().find(|r| r.test_name.contains("start VM")).unwrap();
        assert_eq!(failed.status, TestStatus::Failed);
        assert!(failed.error_message.as_deref().unwrap().contains("Locator not found"));
        assert_eq!(failed.jira_ids, vec!["CNV-99999"]);
    }

    #[test]
    fn parses_quarantined_skip() {
        let results = parse_xml(BASIC_XML).unwrap();
        let skipped = results.iter().find(|r| r.is_quarantined).unwrap();
        assert_eq!(skipped.status, TestStatus::Skipped);
        assert!(skipped.is_quarantined);
        assert_eq!(skipped.jira_ids, vec!["CNV-55555"]);
    }

    #[test]
    fn extracts_multiple_jira_ids() {
        let xml = r#"<testsuites><testsuite>
          <testcase classname="tier1" name="test ID(CNV-111) and ID(CNV-222)" time="1.0"/>
        </testsuite></testsuites>"#;
        let results = parse_xml(xml).unwrap();
        assert_eq!(results[0].jira_ids, vec!["CNV-111", "CNV-222"]);
    }

    #[test]
    fn parses_empty_testsuites() {
        let xml = r#"<testsuites></testsuites>"#;
        let results = parse_xml(xml).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn returns_error_on_invalid_xml() {
        let result = parse_xml("<not valid xml");
        assert!(result.is_err());
    }

    // ── classname_to_spec ─────────────────────────────────────────────────────

    #[test]
    fn maps_arrow_classname_to_spec_path() {
        let path = classname_to_spec("tier1 › checkups › checkups.spec.ts");
        assert_eq!(path.as_deref(), Some("playwright/tests/tier1/checkups/checkups.spec.ts"));
    }

    #[test]
    fn maps_nested_arrow_classname() {
        let path = classname_to_spec("tier1 › vm › tabs › vm-tabs.spec.ts");
        assert_eq!(path.as_deref(), Some("playwright/tests/tier1/vm/tabs/vm-tabs.spec.ts"));
    }

    #[test]
    fn returns_none_for_empty_classname() {
        assert!(classname_to_spec("").is_none());
    }

    #[test]
    fn passthrough_for_existing_spec_path() {
        let path = classname_to_spec("playwright/tests/tier1/vm.spec.ts");
        assert_eq!(path.as_deref(), Some("playwright/tests/tier1/vm.spec.ts"));
    }

    // ── merge_quarantined ─────────────────────────────────────────────────────

    #[test]
    fn merge_recovers_quarantined_message() {
        let results = vec![
            TestResult {
                classname: "tier1".into(),
                test_name: "my failing test".into(),
                spec_path: None,
                jira_ids: vec![],
                status: TestStatus::Failed,
                error_message: None, // CI Safe Mode hid the message
                is_quarantined: false,
                duration_secs: 1.0,
            },
            TestResult {
                classname: "tier1".into(),
                test_name: "Quarantined: my failing test".into(),
                spec_path: None,
                jira_ids: vec![],
                status: TestStatus::Skipped,
                error_message: Some("Real error: element not visible".into()),
                is_quarantined: true,
                duration_secs: 0.0,
            },
        ];

        let merged = merge_quarantined(&results);
        // Should have one result (the quarantined skip is absorbed)
        let actionable: Vec<_> = merged
            .iter()
            .filter(|r| r.test_name == "my failing test")
            .collect();
        assert_eq!(actionable.len(), 1);
        assert!(actionable[0].error_message.as_deref().unwrap().contains("Real error"));
        assert!(actionable[0].is_quarantined);
    }

    #[test]
    fn merge_keeps_non_quarantined_intact() {
        let results = vec![
            TestResult {
                classname: "tier1".into(),
                test_name: "normal passing test".into(),
                spec_path: None,
                jira_ids: vec![],
                status: TestStatus::Passed,
                error_message: None,
                is_quarantined: false,
                duration_secs: 2.0,
            },
        ];
        let merged = merge_quarantined(&results);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].status, TestStatus::Passed);
    }

    #[test]
    fn merge_is_idempotent_with_no_quarantined() {
        let xml = r#"<testsuites><testsuite>
          <testcase classname="tier1" name="test A" time="1.0"/>
          <testcase classname="tier1" name="test B" time="1.0">
            <failure message="err">stack</failure>
          </testcase>
        </testsuite></testsuites>"#;
        let results = parse_xml(xml).unwrap();
        let merged = merge_quarantined(&results);
        assert_eq!(merged.len(), results.len());
    }

    // ── duration ─────────────────────────────────────────────────────────────

    #[test]
    fn parses_duration_correctly() {
        let xml = r#"<testsuites><testsuite>
          <testcase classname="x" name="slow test" time="123.456"/>
        </testsuite></testsuites>"#;
        let results = parse_xml(xml).unwrap();
        assert!((results[0].duration_secs - 123.456).abs() < 0.001);
    }
}

// ── MCP tool handler ──────────────────────────────────────────────────────────

use crate::config::Config;
use crate::mcp::protocol::ToolCallResult;

pub fn parse_junit(params: &serde_json::Value, cfg: &Config) -> ToolCallResult {
    let path = params
        .get("path")
        .and_then(|v| v.as_str())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| cfg.junit_path());

    match parse_file(&path) {
        Ok(results) => {
            let json = serde_json::to_string_pretty(&results)
                .unwrap_or_else(|e| format!("Serialization error: {}", e));
            ToolCallResult::text(format!(
                "Parsed {} test results from {}:\n\n{}",
                results.len(),
                path.display(),
                json
            ))
        }
        Err(e) => ToolCallResult::error(e),
    }
}

pub fn merge_quarantined_tool(params: &serde_json::Value, cfg: &Config) -> ToolCallResult {
    let path = params
        .get("path")
        .and_then(|v| v.as_str())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| cfg.junit_path());

    let results = match parse_file(&path) {
        Ok(r) => r,
        Err(e) => return ToolCallResult::error(e),
    };

    let merged = merge_quarantined(&results);
    let failures: Vec<_> = merged
        .iter()
        .filter(|r| {
            r.status == TestStatus::Failed
                || r.status == TestStatus::Error
                || (r.status == TestStatus::Skipped && r.is_quarantined)
        })
        .collect();

    let json = serde_json::to_string_pretty(&failures)
        .unwrap_or_else(|e| format!("Serialization error: {}", e));

    ToolCallResult::text(format!(
        "After merging quarantined skips: {} actionable failures from {} total results.\n\n{}",
        failures.len(),
        merged.len(),
        json
    ))
}

pub fn get_failure_summary(params: &serde_json::Value, cfg: &Config) -> ToolCallResult {
    let path = params
        .get("path")
        .and_then(|v| v.as_str())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| cfg.junit_path());

    let results = match parse_file(&path) {
        Ok(r) => r,
        Err(e) => return ToolCallResult::error(e),
    };

    let total = results.len();
    let passed = results.iter().filter(|r| r.status == TestStatus::Passed).count();
    let failed = results
        .iter()
        .filter(|r| r.status == TestStatus::Failed || r.status == TestStatus::Error)
        .count();
    let skipped = results.iter().filter(|r| r.status == TestStatus::Skipped).count();
    let quarantined = results.iter().filter(|r| r.is_quarantined).count();

    // Per-tier breakdown from classname/spec_path prefix
    let mut tier_counts: std::collections::BTreeMap<String, (usize, usize)> =
        std::collections::BTreeMap::new();
    for r in &results {
        let tier = r
            .spec_path
            .as_deref()
            .and_then(|p| p.strip_prefix("playwright/tests/"))
            .and_then(|p| p.split('/').next())
            .or_else(|| {
                r.classname
                    .split(['›', '/'])
                    .next()
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
            })
            .unwrap_or("unknown")
            .to_string();

        let entry = tier_counts.entry(tier).or_insert((0, 0));
        entry.0 += 1;
        if r.status == TestStatus::Failed || r.status == TestStatus::Error {
            entry.1 += 1;
        }
    }

    // Top failure signatures
    let mut error_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    for r in &results {
        if r.status == TestStatus::Failed || r.status == TestStatus::Error {
            let sig = r
                .error_message
                .as_deref()
                .map(|m| {
                    m.lines()
                        .next()
                        .unwrap_or("")
                        .chars()
                        .take(120)
                        .collect::<String>()
                })
                .unwrap_or_else(|| "(no message)".into());
            *error_counts.entry(sig).or_insert(0) += 1;
        }
    }
    let mut top_errors: Vec<_> = error_counts.into_iter().collect();
    top_errors.sort_by(|a, b| b.1.cmp(&a.1));

    let mut out = format!(
        "## Test Run Summary — {}\n\n\
        - Total: {}\n\
        - Passed: {}\n\
        - Failed: {}\n\
        - Skipped: {}\n\
        - Quarantined (CI Safe Mode): {}\n\n\
        ### Per-tier breakdown\n",
        path.display(),
        total,
        passed,
        failed,
        skipped,
        quarantined,
    );

    for (tier, (total_t, failed_t)) in &tier_counts {
        out.push_str(&format!("- {}: {} total, {} failed\n", tier, total_t, failed_t));
    }

    out.push_str("\n### Top failure signatures\n");
    for (sig, count) in top_errors.iter().take(5) {
        out.push_str(&format!("- ({}) {}\n", count, sig));
    }

    ToolCallResult::text(out)
}

pub fn get_reproduce_command(params: &serde_json::Value, cfg: &Config) -> ToolCallResult {
    let spec_path = match params.get("spec_path").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => return ToolCallResult::error("Missing required parameter: spec_path"),
    };

    let grep = if let Some(jira_id) = params.get("jira_id").and_then(|v| v.as_str()) {
        jira_id.to_string()
    } else if let Some(test_name) = params.get("test_name").and_then(|v| v.as_str()) {
        test_name.to_string()
    } else {
        String::new()
    };

    let project_root = cfg.project_root.display().to_string();

    let mut cmd = format!(
        "cd {} && CI_SAFE_MODE=0 yarn test-playwright --project=\"E2E Tests\" {}",
        project_root, spec_path
    );

    if !grep.is_empty() {
        cmd.push_str(&format!(" --grep \"{}\"", grep));
    }

    ToolCallResult::text(format!(
        "Reproduce command:\n\n```bash\n{}\n```\n\nOr with debug (headed, verbose):\n\n```bash\n{} --headed --timeout=0\n```",
        cmd, cmd
    ))
}
