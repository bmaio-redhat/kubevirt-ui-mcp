use std::collections::HashMap;
use std::path::Path;

use serde_json::{json, Value};

use crate::coverage::scanner::{ProjectScanner, Visibility};

static FEATURE_ALIASES: &[(&str, &[&str])] = &[
    ("virtual-machines", &["virtualmachines", "vm-", "virtual-machine"]),
    ("vm-detail", &["virtual-machine-detail", "vm-tabs", "vm-overview", "vm-console", "vm-configuration"]),
    ("vm-actions", &["vm-lifecycle", "vm-resource", "vm-migration"]),
    ("bootable-volumes", &["bootable-volume", "bootable_volume"]),
    ("catalog", &["catalog"]),
    ("templates", &["template"]),
    ("instance-types", &["instancetype", "instance-type"]),
    ("overview", &["overview"]),
    ("networking", &["network", "nad", "udn", "nnc"]),
    ("migration-policies", &["migration-polic", "migrationpolic"]),
    ("checkups", &["checkup"]),
    ("quotas", &["quota", "aaq"]),
    ("settings", &["cluster-settings", "user-settings"]),
    ("storage-migration", &["storage_migration", "storage-migration"]),
];

fn feature_matches(text: &str, feature: &str) -> bool {
    let lower = text.to_lowercase();
    let feature_lower = feature.to_lowercase();

    if lower.contains(&feature_lower) {
        return true;
    }

    for (key, aliases) in FEATURE_ALIASES {
        if *key == feature_lower || aliases.iter().any(|a| *a == feature_lower.as_str()) {
            if aliases.iter().any(|a| lower.contains(a)) || lower.contains(key) {
                return true;
            }
        }
    }
    false
}

fn to_camel_case(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(f) => f.to_lowercase().to_string() + chars.as_str(),
    }
}

pub fn get_coverage_for_feature(scanner: &mut ProjectScanner, feature: &str) -> Value {
    let playwright_root = scanner.playwright_root.clone();
    let tests: Vec<_> = scanner
        .scan_test_files()
        .iter()
        .filter(|t| {
            feature_matches(&t.relative_path, feature)
                || t.describe_titles.iter().any(|d| feature_matches(d, feature))
                || t.feature.to_lowercase().contains(&feature.to_lowercase())
                || t.tags.iter().any(|tag| feature_matches(tag, feature))
        })
        .cloned()
        .collect();

    let used_sd_names: std::collections::HashSet<String> =
        tests.iter().flat_map(|t| t.step_drivers_used.clone()).collect();

    let sd_dir = scanner.step_drivers_dir.clone();
    let po_dir = scanner.page_objects_dir.clone();

    let related_step_drivers: Vec<String> = scanner
        .scan_step_driver_methods()
        .iter()
        .filter(|m| {
            let camel = to_camel_case(&m.class_name.replace("StepDriver", ""));
            used_sd_names.contains(&camel)
        })
        .map(|m| {
            m.file_path
                .strip_prefix(&playwright_root)
                .unwrap_or(&m.file_path)
                .to_string_lossy()
                .to_string()
        })
        .fold(Vec::new(), |mut acc, p| {
            if !acc.contains(&p) { acc.push(p); }
            acc
        });

    let related_page_objects: Vec<String> = scanner
        .scan_page_object_methods()
        .iter()
        .filter(|m| feature_matches(&m.class_name, feature) || feature_matches(&m.file_path.to_string_lossy(), feature))
        .map(|m| {
            m.file_path
                .strip_prefix(&playwright_root)
                .unwrap_or(&m.file_path)
                .to_string_lossy()
                .to_string()
        })
        .fold(Vec::new(), |mut acc, p| {
            if !acc.contains(&p) { acc.push(p); }
            acc
        });

    let related_docs = scanner.scan_docs_for_feature(feature);

    let all_jira_ids: Vec<String> = {
        let mut ids: Vec<String> = tests.iter().flat_map(|t| t.jira_ids.clone()).collect();
        ids.sort();
        ids.dedup();
        ids
    };

    let spec_files: Vec<Value> = tests
        .iter()
        .map(|t| json!({
            "path": t.relative_path,
            "tier": t.tier,
            "tests": t.test_names,
            "jiraIds": t.jira_ids,
            "tags": t.tags,
        }))
        .collect();

    let total_tests: usize = tests.iter().map(|t| t.test_names.len()).sum();

    json!({
        "feature": feature,
        "totalTests": total_tests,
        "specFiles": spec_files,
        "stepDrivers": related_step_drivers,
        "pageObjects": related_page_objects,
        "docs": related_docs,
        "jiraIds": all_jira_ids,
        "stepDriversUsed": used_sd_names.into_iter().collect::<Vec<_>>(),
    })
}

pub fn get_untested_step_driver_methods(scanner: &mut ProjectScanner) -> Value {
    let playwright_root = scanner.playwright_root.clone();
    let tests_dir = scanner.tests_dir.clone();
    let sd_dir = scanner.step_drivers_dir.clone();

    let sd_methods: Vec<_> = scanner
        .scan_step_driver_methods()
        .iter()
        .filter(|m| m.visibility == Visibility::Public && !m.name.starts_with('_'))
        .cloned()
        .collect();

    let search_dirs = [tests_dir.as_path(), sd_dir.as_path()];
    let total = sd_methods.len();
    let mut untested = Vec::new();

    for method in &sd_methods {
        let refs = scanner.find_method_references(&method.name, &search_dirs);
        let self_file = method
            .file_path
            .strip_prefix(&playwright_root)
            .unwrap_or(&method.file_path)
            .to_string_lossy()
            .to_string();
        let external_refs: Vec<_> = refs.iter().filter(|r| **r != self_file).collect();
        if external_refs.is_empty() {
            untested.push(json!({
                "method": method.name,
                "className": method.class_name,
                "file": self_file,
                "line": method.line_number,
            }));
        }
    }

    let coverage_percent = if total > 0 {
        ((total - untested.len()) * 100) / total
    } else {
        100
    };

    json!({
        "totalPublicMethods": total,
        "untestedCount": untested.len(),
        "coveragePercent": coverage_percent,
        "untestedMethods": untested,
    })
}

pub fn get_orphan_page_object_methods(scanner: &mut ProjectScanner) -> Value {
    let playwright_root = scanner.playwright_root.clone();
    let sd_dir = scanner.step_drivers_dir.clone();
    let tests_dir = scanner.tests_dir.clone();

    let po_methods: Vec<_> = scanner
        .scan_page_object_methods()
        .iter()
        .filter(|m| m.visibility == Visibility::Public && !m.name.starts_with('_'))
        .cloned()
        .collect();

    let search_dirs = [sd_dir.as_path(), tests_dir.as_path()];
    let total = po_methods.len();
    let mut orphans = Vec::new();

    for method in &po_methods {
        let refs = scanner.find_method_references(&method.name, &search_dirs);
        if refs.is_empty() {
            let self_file = method
                .file_path
                .strip_prefix(&playwright_root)
                .unwrap_or(&method.file_path)
                .to_string_lossy()
                .to_string();
            orphans.push(json!({
                "method": method.name,
                "className": method.class_name,
                "file": self_file,
                "line": method.line_number,
            }));
        }
    }

    let coverage_percent = if total > 0 {
        ((total - orphans.len()) * 100) / total
    } else {
        100
    };

    json!({
        "totalPublicMethods": total,
        "orphanCount": orphans.len(),
        "coveragePercent": coverage_percent,
        "orphanMethods": orphans,
    })
}

pub fn get_tier_distribution(scanner: &mut ProjectScanner) -> Value {
    let mut tiers: HashMap<String, serde_json::Map<String, Value>> = HashMap::new();

    for test in scanner.scan_test_files().iter() {
        let entry = tiers.entry(test.tier.clone()).or_insert_with(|| {
            let mut m = serde_json::Map::new();
            m.insert("fileCount".into(), json!(0));
            m.insert("testCount".into(), json!(0));
            m.insert("jiraIds".into(), json!([]));
            m.insert("files".into(), json!([]));
            m
        });

        *entry.get_mut("fileCount").unwrap() =
            json!(entry["fileCount"].as_u64().unwrap_or(0) + 1);
        *entry.get_mut("testCount").unwrap() =
            json!(entry["testCount"].as_u64().unwrap_or(0) + test.test_names.len() as u64);

        let jira_arr = entry["jiraIds"].as_array_mut().unwrap();
        for id in &test.jira_ids {
            if !jira_arr.iter().any(|v| v.as_str() == Some(id)) {
                jira_arr.push(json!(id));
            }
        }

        let files_arr = entry["files"].as_array_mut().unwrap();
        files_arr.push(json!(test.relative_path));
    }

    let total_tests: u64 = tiers.values().map(|t| t["testCount"].as_u64().unwrap_or(0)).sum();
    let total_files: u64 = tiers.values().map(|t| t["fileCount"].as_u64().unwrap_or(0)).sum();

    json!({
        "totalFiles": total_files,
        "totalTests": total_tests,
        "tiers": tiers,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn write_file(dir: &TempDir, rel: &str, content: &str) {
        let path = dir.path().join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::File::create(&path).unwrap().write_all(content.as_bytes()).unwrap();
    }

    fn make_scanner(dir: &TempDir) -> ProjectScanner {
        ProjectScanner::for_test(dir.path().to_path_buf())
    }

    // ── feature_matches ──────────────────────────────────────────────────────

    #[test]
    fn feature_matches_direct_substring() {
        assert!(feature_matches("tests/tier1/checkups/checkups.spec.ts", "checkups"));
        assert!(!feature_matches("tests/tier1/vm/vm.spec.ts", "checkups"));
    }

    #[test]
    fn feature_matches_via_alias() {
        // "networking" aliases include "nad"
        assert!(feature_matches("tests/tier1/networking/nad.spec.ts", "nad"));
        // "virtual-machines" aliases include "vm-"
        assert!(feature_matches("tests/tier1/vm-list.spec.ts", "virtual-machines"));
    }

    // ── get_tier_distribution ────────────────────────────────────────────────

    #[test]
    fn tier_distribution_counts_files_and_tests() {
        let dir = tempfile::tempdir().unwrap();
        write_file(&dir, "tests/tier1/checkups/checkups.spec.ts",
            "test('ID(CNV-1) foo', async () => {}); test('ID(CNV-2) bar', async () => {});");
        write_file(&dir, "tests/gating/overview/overview.spec.ts",
            "test('ID(CNV-3) baz', async () => {});");
        let mut scanner = make_scanner(&dir);
        let dist = get_tier_distribution(&mut scanner);
        assert_eq!(dist["totalFiles"], 2);
        assert_eq!(dist["totalTests"], 3);
        let tiers = dist["tiers"].as_object().unwrap();
        assert!(tiers.contains_key("tier1"));
        assert!(tiers.contains_key("gating"));
        assert_eq!(tiers["tier1"]["fileCount"], 1);
        assert_eq!(tiers["tier1"]["testCount"], 2);
    }

    // ── find_tests_by_jira ────────────────────────────────────────────────────

    #[test]
    fn find_tests_by_jira_found() {
        let dir = tempfile::tempdir().unwrap();
        write_file(&dir, "tests/tier1/vm/vm.spec.ts",
            "test('ID(CNV-99999) should start VM', async () => {});");
        let mut scanner = make_scanner(&dir);
        let result = find_tests_by_jira(&mut scanner, "CNV-99999");
        assert_eq!(result["found"], true);
        assert_eq!(result["ticketId"], "CNV-99999");
        let tests = result["tests"].as_array().unwrap();
        assert_eq!(tests.len(), 1);
    }

    #[test]
    fn find_tests_by_jira_not_found() {
        let dir = tempfile::tempdir().unwrap();
        write_file(&dir, "tests/tier1/vm/vm.spec.ts", "test('no jira', async () => {});");
        let mut scanner = make_scanner(&dir);
        let result = find_tests_by_jira(&mut scanner, "CNV-00000");
        assert_eq!(result["found"], false);
    }

    #[test]
    fn find_tests_by_jira_case_insensitive() {
        let dir = tempfile::tempdir().unwrap();
        write_file(&dir, "tests/tier1/vm/vm.spec.ts",
            "test('ID(CNV-12345) works', async () => {});");
        let mut scanner = make_scanner(&dir);
        let result = find_tests_by_jira(&mut scanner, "cnv-12345");
        assert_eq!(result["found"], true);
    }

    // ── get_coverage_for_feature ─────────────────────────────────────────────

    #[test]
    fn coverage_for_feature_returns_matching_spec() {
        let dir = tempfile::tempdir().unwrap();
        write_file(&dir, "tests/tier1/checkups/checkups.spec.ts",
            r#"test.describe('Checkups', () => { test('ID(CNV-55555) should create', async () => {}); });"#);
        write_file(&dir, "tests/tier1/vm/vm.spec.ts",
            "test('should start vm', async () => {});");
        let mut scanner = make_scanner(&dir);
        let result = get_coverage_for_feature(&mut scanner, "checkups");
        let specs = result["specFiles"].as_array().unwrap();
        assert_eq!(specs.len(), 1);
        assert!(specs[0]["path"].as_str().unwrap().contains("checkups"));
        assert!(result["jiraIds"].as_array().unwrap().iter().any(|v| v == "CNV-55555"));
    }
}

pub fn find_tests_by_jira(scanner: &mut ProjectScanner, ticket_id: &str) -> Value {
    let normalized = ticket_id.to_uppercase().replace(' ', "");

    let matches: Vec<_> = scanner
        .scan_test_files()
        .iter()
        .filter(|t| t.jira_ids.iter().any(|id| id.to_uppercase() == normalized))
        .map(|t| json!({
            "file": t.relative_path,
            "tier": t.tier,
            "feature": t.feature,
            "tests": t.test_names,
            "jiraIds": t.jira_ids,
            "tags": t.tags,
        }))
        .collect();

    if matches.is_empty() {
        json!({
            "ticketId": normalized,
            "found": false,
            "message": format!("No tests found for {}", normalized),
            "tests": [],
        })
    } else {
        json!({
            "ticketId": normalized,
            "found": true,
            "tests": matches,
        })
    }
}
