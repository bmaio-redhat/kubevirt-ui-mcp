use serde_json::Value;

use crate::config::Config;
use crate::mcp::protocol::ToolCallResult;
use crate::ci_triage::tools::junit::{parse_file, TestResult, TestStatus};

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureClass {
    Infrastructure,
    ProductBug,
    TestBug,
    Flaky,
    Unknown,
}

#[derive(Debug, serde::Serialize)]
pub struct ClassifiedFailure {
    pub test_name: String,
    pub spec_path: Option<String>,
    pub jira_ids: Vec<String>,
    pub classification: FailureClass,
    pub reason: String,
    pub error_message: Option<String>,
}

/// Classify each failure in a JUnit report.
pub fn classify_failures(params: &Value, cfg: &Config) -> ToolCallResult {
    let path = params
        .get("path")
        .and_then(|v| v.as_str())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| cfg.junit_path());

    let results = match parse_file(&path) {
        Ok(r) => r,
        Err(e) => return ToolCallResult::error(e),
    };

    let failures: Vec<_> = results
        .iter()
        .filter(|r| r.status == TestStatus::Failed || r.status == TestStatus::Error)
        .collect();

    if failures.is_empty() {
        return ToolCallResult::text("No failures found to classify.");
    }

    let classified: Vec<ClassifiedFailure> = failures
        .iter()
        .map(|r| classify_one(r))
        .collect();

    let mut by_class: std::collections::BTreeMap<String, usize> =
        std::collections::BTreeMap::new();
    for c in &classified {
        let key = format!("{:?}", c.classification);
        *by_class.entry(key).or_insert(0) += 1;
    }

    let summary: Vec<String> = by_class
        .iter()
        .map(|(k, v)| format!("  {}: {}", k, v))
        .collect();

    let json = serde_json::to_string_pretty(&classified)
        .unwrap_or_else(|e| format!("Serialization error: {}", e));

    ToolCallResult::text(format!(
        "Classified {} failures:\n{}\n\nDetails:\n{}",
        classified.len(),
        summary.join("\n"),
        json
    ))
}

fn classify_one(r: &TestResult) -> ClassifiedFailure {
    let msg = r.error_message.as_deref().unwrap_or("").to_lowercase();

    let (classification, reason) = if is_infrastructure(&msg) {
        (FailureClass::Infrastructure, infer_infra_reason(&msg))
    } else if is_flaky(&msg) {
        (FailureClass::Flaky, "Intermittent/retry pattern or race condition detected".into())
    } else if is_test_bug(&msg) {
        (FailureClass::TestBug, infer_test_bug_reason(&msg))
    } else if is_product_bug(&msg) {
        (FailureClass::ProductBug, "Assertion failed on UI element or API response".into())
    } else {
        (FailureClass::Unknown, "No matching pattern — review manually".into())
    };

    ClassifiedFailure {
        test_name: r.test_name.clone(),
        spec_path: r.spec_path.clone(),
        jira_ids: r.jira_ids.clone(),
        classification,
        reason,
        error_message: r.error_message.clone(),
    }
}

fn is_infrastructure(msg: &str) -> bool {
    let patterns = [
        "timeout",
        "etimedout",
        "econnrefused",
        "econnreset",
        "network",
        "dns",
        "cluster",
        "kubeconfig",
        "401 unauthorized",
        "403 forbidden",
        "503 service unavailable",
        "node not ready",
        "pod not running",
        "failed to connect",
        "context deadline exceeded",
        "waiting for deployment",
        "globalsetup",
        "storage class",
    ];
    patterns.iter().any(|p| msg.contains(p))
}

fn is_flaky(msg: &str) -> bool {
    let patterns = [
        "retry",
        "intermittent",
        "race",
        "already exists",
        "conflict",
        "resource version",
        "flak",
        "eventual",
        "still running",
        "not yet",
    ];
    patterns.iter().any(|p| msg.contains(p))
}

fn is_test_bug(msg: &str) -> bool {
    let patterns = [
        "typeerror",
        "referenceerror",
        "cannot read propert",
        "is not a function",
        "is undefined",
        "locator",
        "no element",
        "element not found",
        "selector",
        "data-test",
        "import",
        "module not found",
        "compilation error",
        "syntaxerror",
    ];
    patterns.iter().any(|p| msg.contains(p))
}

fn is_product_bug(msg: &str) -> bool {
    let patterns = [
        "expect(",
        "tobevisible",
        "tohavetext",
        "tocontaintext",
        "expected",
        "received",
        "assertion",
        "assert",
        "status code",
        "response body",
    ];
    patterns.iter().any(|p| msg.contains(p))
}

fn infer_infra_reason(msg: &str) -> String {
    if msg.contains("timeout") {
        "Test timed out — likely cluster slowness or resource not reaching expected state".into()
    } else if msg.contains("econnrefused") || msg.contains("econnreset") {
        "Network connection refused/reset — API server may be unreachable".into()
    } else if msg.contains("401") || msg.contains("403") {
        "Authentication/authorization failure — check kubeconfig and RBAC".into()
    } else if msg.contains("globalsetup") || msg.contains("kubeconfig") {
        "Global setup failure — cluster not reachable or login failed".into()
    } else {
        "Infrastructure issue detected — review cluster health".into()
    }
}

fn infer_test_bug_reason(msg: &str) -> String {
    if msg.contains("locator") || msg.contains("selector") || msg.contains("data-test") {
        "Locator/selector not found — page object may need updating for new UI".into()
    } else if msg.contains("typeerror") || msg.contains("is not a function") {
        "TypeScript/runtime type error — likely wrong import or API mismatch".into()
    } else if msg.contains("module not found") || msg.contains("import") {
        "Import resolution failure — check barrel file or path changes".into()
    } else {
        "Test code error detected — review test implementation".into()
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infrastructure_timeout_classified_correctly() {
        let r = TestResult {
            classname: "tier1".into(),
            test_name: "slow test".into(),
            spec_path: None,
            jira_ids: vec![],
            status: TestStatus::Failed,
            error_message: Some("Test timeout exceeded waiting for element".into()),
            is_quarantined: false,
            duration_secs: 480.0,
        };
        let c = classify_one(&r);
        assert_eq!(c.classification, FailureClass::Infrastructure);
    }

    #[test]
    fn locator_classified_as_test_bug() {
        let r = TestResult {
            classname: "tier1".into(),
            test_name: "click test".into(),
            spec_path: None,
            jira_ids: vec![],
            status: TestStatus::Failed,
            error_message: Some("locator('[data-test=submit]') not found in page".into()),
            is_quarantined: false,
            duration_secs: 2.0,
        };
        let c = classify_one(&r);
        assert_eq!(c.classification, FailureClass::TestBug);
    }

    #[test]
    fn assertion_classified_as_product_bug() {
        let r = TestResult {
            classname: "tier1".into(),
            test_name: "visibility test".into(),
            spec_path: None,
            jira_ids: vec![],
            status: TestStatus::Failed,
            error_message: Some("expect(element).toBeVisible() — Expected element to be visible but it was hidden".into()),
            is_quarantined: false,
            duration_secs: 3.0,
        };
        let c = classify_one(&r);
        assert_eq!(c.classification, FailureClass::ProductBug);
    }

    #[test]
    fn connection_refused_classified_as_infrastructure() {
        let r = TestResult {
            classname: "tier1".into(),
            test_name: "api test".into(),
            spec_path: None,
            jira_ids: vec![],
            status: TestStatus::Error,
            error_message: Some("ECONNREFUSED 127.0.0.1:6443".into()),
            is_quarantined: false,
            duration_secs: 0.1,
        };
        let c = classify_one(&r);
        assert_eq!(c.classification, FailureClass::Infrastructure);
    }

    #[test]
    fn no_message_classified_as_unknown() {
        let r = TestResult {
            classname: "tier1".into(),
            test_name: "mystery test".into(),
            spec_path: None,
            jira_ids: vec![],
            status: TestStatus::Failed,
            error_message: None,
            is_quarantined: false,
            duration_secs: 1.0,
        };
        let c = classify_one(&r);
        assert_eq!(c.classification, FailureClass::Unknown);
    }

    #[test]
    fn flaky_pattern_detected() {
        let r = TestResult {
            classname: "tier1".into(),
            test_name: "flaky test".into(),
            spec_path: None,
            jira_ids: vec![],
            status: TestStatus::Failed,
            error_message: Some("resource version conflict on retry attempt".into()),
            is_quarantined: false,
            duration_secs: 5.0,
        };
        let c = classify_one(&r);
        assert_eq!(c.classification, FailureClass::Flaky);
    }

    #[test]
    fn classify_preserves_jira_ids() {
        let r = TestResult {
            classname: "tier1".into(),
            test_name: "test ID(CNV-999)".into(),
            spec_path: Some("playwright/tests/tier1/x.spec.ts".into()),
            jira_ids: vec!["CNV-999".into()],
            status: TestStatus::Failed,
            error_message: None,
            is_quarantined: false,
            duration_secs: 1.0,
        };
        let c = classify_one(&r);
        assert_eq!(c.jira_ids, vec!["CNV-999"]);
        assert_eq!(c.spec_path.as_deref(), Some("playwright/tests/tier1/x.spec.ts"));
    }
}
