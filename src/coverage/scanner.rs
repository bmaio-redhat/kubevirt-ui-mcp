use std::collections::HashMap;
use std::path::{Path, PathBuf};

use regex::Regex;
use walkdir::WalkDir;

use crate::config::Config;

#[derive(Debug, Clone)]
pub struct TestFileInfo {
    pub file_path: PathBuf,
    pub relative_path: String,
    pub tier: String,
    pub feature: String,
    pub jira_ids: Vec<String>,
    pub test_names: Vec<String>,
    pub describe_titles: Vec<String>,
    pub tags: Vec<String>,
    pub step_drivers_used: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct MethodInfo {
    pub name: String,
    pub file_path: PathBuf,
    pub class_name: String,
    pub line_number: usize,
    pub is_async: bool,
    pub visibility: Visibility,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Visibility {
    Public,
    Protected,
    Private,
}

pub struct ProjectScanner {
    pub playwright_root: PathBuf,
    pub tests_dir: PathBuf,
    pub page_objects_dir: PathBuf,
    pub step_drivers_dir: PathBuf,
    pub docs_dir: PathBuf,

    test_files_cache: Option<Vec<TestFileInfo>>,
    po_methods_cache: Option<Vec<MethodInfo>>,
    sd_methods_cache: Option<Vec<MethodInfo>>,
}

impl ProjectScanner {
    pub fn new(cfg: &Config) -> Self {
        let playwright_root = cfg.playwright_root.clone();
        let tests_dir = playwright_root.join("tests");
        let page_objects_dir = playwright_root.join("src/page-objects");
        let step_drivers_dir = playwright_root.join("src/step-drivers");
        let docs_dir = playwright_root.join("docs");
        Self {
            playwright_root,
            tests_dir,
            page_objects_dir,
            step_drivers_dir,
            docs_dir,
            test_files_cache: None,
            po_methods_cache: None,
            sd_methods_cache: None,
        }
    }

    #[cfg(test)]
    pub fn for_test(root: std::path::PathBuf) -> Self {
        Self {
            tests_dir: root.join("tests"),
            page_objects_dir: root.join("src/page-objects"),
            step_drivers_dir: root.join("src/step-drivers"),
            docs_dir: root.join("docs"),
            playwright_root: root,
            test_files_cache: None,
            po_methods_cache: None,
            sd_methods_cache: None,
        }
    }

    pub fn invalidate_cache(&mut self) {
        self.test_files_cache = None;
        self.po_methods_cache = None;
        self.sd_methods_cache = None;
    }

    pub fn scan_test_files(&mut self) -> &[TestFileInfo] {
        if self.test_files_cache.is_none() {
            let files = walk_dir(&self.tests_dir, ".spec.ts");
            let playwright_root = self.playwright_root.clone();
            let parsed: Vec<TestFileInfo> = files
                .into_iter()
                .map(|f| parse_test_file(&f, &playwright_root))
                .collect();
            self.test_files_cache = Some(parsed);
        }
        self.test_files_cache.as_deref().unwrap_or(&[])
    }

    pub fn scan_page_object_methods(&mut self) -> &[MethodInfo] {
        if self.po_methods_cache.is_none() {
            let po_dir = self.page_objects_dir.clone();
            let methods = scan_methods_in_dir(&po_dir);
            self.po_methods_cache = Some(methods);
        }
        self.po_methods_cache.as_deref().unwrap_or(&[])
    }

    pub fn scan_step_driver_methods(&mut self) -> &[MethodInfo] {
        if self.sd_methods_cache.is_none() {
            let sd_dir = self.step_drivers_dir.clone();
            let methods = scan_methods_in_dir(&sd_dir);
            self.sd_methods_cache = Some(methods);
        }
        self.sd_methods_cache.as_deref().unwrap_or(&[])
    }

    pub fn find_method_references(&self, method_name: &str, search_dirs: &[&Path]) -> Vec<String> {
        let pattern = format!(r"\.{}\s*\(", regex::escape(method_name));
        let re = match Regex::new(&pattern) {
            Ok(r) => r,
            Err(_) => return vec![],
        };

        let mut refs = Vec::new();
        for dir in search_dirs {
            for file in walk_dir(dir, ".ts") {
                if let Ok(content) = std::fs::read_to_string(&file) {
                    if re.is_match(&content) {
                        let rel = file
                            .strip_prefix(&self.playwright_root)
                            .unwrap_or(&file)
                            .to_string_lossy()
                            .to_string();
                        refs.push(rel);
                    }
                }
            }
        }
        refs
    }

    pub fn scan_docs_for_feature(&self, feature: &str) -> Vec<String> {
        if !self.docs_dir.exists() {
            return vec![];
        }
        let feature_lower = feature.to_lowercase();
        let mut matches = Vec::new();
        for file in walk_dir(&self.docs_dir, ".md") {
            let rel = file
                .strip_prefix(&self.playwright_root)
                .unwrap_or(&file)
                .to_string_lossy()
                .to_string();
            let rel_lower = rel.to_lowercase();
            if rel_lower.contains(&feature_lower) {
                if let Ok(content) = std::fs::read_to_string(&file) {
                    if content.to_lowercase().contains(&feature_lower) {
                        matches.push(rel.clone());
                        continue;
                    }
                }
                matches.push(rel);
            }
        }
        matches
    }
}

fn walk_dir(dir: &Path, ext: &str) -> Vec<PathBuf> {
    if !dir.exists() {
        return vec![];
    }
    WalkDir::new(dir)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_type().is_file()
                && e.path()
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.ends_with(ext))
                    .unwrap_or(false)
                && !e.path().to_string_lossy().contains("node_modules")
        })
        .map(|e| e.path().to_path_buf())
        .collect()
}

fn parse_test_file(file_path: &Path, playwright_root: &Path) -> TestFileInfo {
    let content = std::fs::read_to_string(file_path).unwrap_or_default();
    let relative_path = file_path
        .strip_prefix(playwright_root)
        .unwrap_or(file_path)
        .to_string_lossy()
        .to_string();

    let tier_re = Regex::new(r"tests/(gating|tier1|tier2|fleet-virtualization-acm)/").unwrap();
    let tier = tier_re
        .captures(&relative_path)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .unwrap_or_else(|| "unknown".into());

    let parts: Vec<&str> = relative_path.split('/').collect();
    let feature = if parts.len() > 3 {
        parts[2..parts.len() - 1].join("/")
    } else {
        file_path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.trim_end_matches(".spec").to_string())
            .unwrap_or_default()
    };

    let jira_re = Regex::new(r"ID\((CNV-\d+)\)").unwrap();
    let jira_ids: Vec<String> = jira_re
        .captures_iter(&content)
        .map(|c| c[1].to_string())
        .collect();

    let test_name_re = Regex::new(r#"test\(\s*['"`]([^'"`]+)['"`]"#).unwrap();
    let test_names: Vec<String> = test_name_re
        .captures_iter(&content)
        .map(|c| c[1].to_string())
        .collect();

    let describe_re = Regex::new(r#"test\.describe(?:\.serial)?\(\s*['"`]([^'"`]+)['"`]"#).unwrap();
    let describe_titles: Vec<String> = describe_re
        .captures_iter(&content)
        .map(|c| c[1].to_string())
        .collect();

    let tag_re = Regex::new(r"tag:\s*\[([^\]]+)\]").unwrap();
    let tags: Vec<String> = tag_re
        .captures_iter(&content)
        .flat_map(|c| {
            c[1].split(',')
                .map(|t| t.trim().trim_matches(|c| c == '\'' || c == '"').to_string())
                .filter(|t| !t.is_empty())
                .collect::<Vec<_>>()
        })
        .collect();

    let steps_re = Regex::new(r"steps\.(\w+)\.").unwrap();
    let mut step_drivers_used: Vec<String> = steps_re
        .captures_iter(&content)
        .map(|c| c[1].to_string())
        .collect();
    step_drivers_used.sort();
    step_drivers_used.dedup();

    TestFileInfo {
        file_path: file_path.to_path_buf(),
        relative_path,
        tier,
        feature,
        jira_ids,
        test_names,
        describe_titles,
        tags,
        step_drivers_used,
    }
}

fn scan_methods_in_dir(dir: &Path) -> Vec<MethodInfo> {
    walk_dir(dir, ".ts")
        .iter()
        .flat_map(|f| parse_class_methods(f))
        .collect()
}

fn parse_class_methods(file_path: &Path) -> Vec<MethodInfo> {
    let content = match std::fs::read_to_string(file_path) {
        Ok(c) => c,
        Err(_) => return vec![],
    };

    let class_re = Regex::new(r"(?:export\s+)?(?:default\s+)?class\s+(\w+)").unwrap();
    let class_name = class_re
        .captures(&content)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .unwrap_or_else(|| {
            file_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Unknown")
                .to_string()
        });

    let method_re =
        Regex::new(r"^\s*(public\s+|protected\s+|private\s+)?(async\s+)?(\w+)\s*\(").unwrap();
    let arrow_re = Regex::new(
        r"^\s*(public\s+|protected\s+|private\s+)?(?:readonly\s+)?(\w+)\s*=\s*(async\s+)?\(",
    )
    .unwrap();
    let getter_re =
        Regex::new(r"^\s*(public\s+|protected\s+|private\s+)?get\s+(\w+)\s*\(\)").unwrap();

    let skip_names = [
        "constructor",
        "if",
        "for",
        "while",
        "switch",
        "catch",
        "return",
        "throw",
        "await",
        "try",
    ];

    let mut methods = Vec::new();

    for (i, line) in content.lines().enumerate() {
        let line_num = i + 1;

        if let Some(cap) = method_re.captures(line) {
            let vis_str = cap.get(1).map(|m| m.as_str().trim()).unwrap_or("public");
            let vis = parse_visibility(vis_str);
            let name = cap[3].to_string();
            if skip_names.contains(&name.as_str()) {
                continue;
            }
            methods.push(MethodInfo {
                name,
                file_path: file_path.to_path_buf(),
                class_name: class_name.clone(),
                line_number: line_num,
                is_async: cap.get(2).is_some(),
                visibility: vis,
            });
            continue;
        }

        if let Some(cap) = arrow_re.captures(line) {
            let vis_str = cap.get(1).map(|m| m.as_str().trim()).unwrap_or("public");
            let vis = parse_visibility(vis_str);
            methods.push(MethodInfo {
                name: cap[2].to_string(),
                file_path: file_path.to_path_buf(),
                class_name: class_name.clone(),
                line_number: line_num,
                is_async: cap.get(3).is_some(),
                visibility: vis,
            });
            continue;
        }

        if let Some(cap) = getter_re.captures(line) {
            let vis_str = cap.get(1).map(|m| m.as_str().trim()).unwrap_or("public");
            let vis = parse_visibility(vis_str);
            methods.push(MethodInfo {
                name: cap[2].to_string(),
                file_path: file_path.to_path_buf(),
                class_name: class_name.clone(),
                line_number: line_num,
                is_async: false,
                visibility: vis,
            });
        }
    }

    methods
}

fn parse_visibility(s: &str) -> Visibility {
    if s.contains("private") {
        Visibility::Private
    } else if s.contains("protected") {
        Visibility::Protected
    } else {
        Visibility::Public
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn write_file(dir: &TempDir, rel: &str, content: &str) -> PathBuf {
        let path = dir.path().join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        path
    }

    // ── parse_test_file ──────────────────────────────────────────────────────

    #[test]
    fn parses_tier_from_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_file(
            &dir,
            "tests/tier1/checkups/checkups.spec.ts",
            "test.describe('Checkups', () => { test('ID(CNV-12345) should work', async () => {}); });",
        );
        let info = parse_test_file(&path, dir.path());
        assert_eq!(info.tier, "tier1");
        assert_eq!(info.jira_ids, vec!["CNV-12345"]);
    }

    #[test]
    fn parses_multiple_jira_ids() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_file(
            &dir,
            "tests/tier2/vm/vm-actions.spec.ts",
            "// ID(CNV-11111) and ID(CNV-22222) test('should do something', async () => {});",
        );
        let info = parse_test_file(&path, dir.path());
        assert_eq!(info.jira_ids, vec!["CNV-11111", "CNV-22222"]);
        assert_eq!(info.tier, "tier2");
    }

    #[test]
    fn parses_describe_titles() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_file(
            &dir,
            "tests/gating/overview/overview.spec.ts",
            r#"test.describe('Virtualization Overview', () => {
  test.describe.serial('nested', () => {});
});"#,
        );
        let info = parse_test_file(&path, dir.path());
        assert!(info.describe_titles.iter().any(|t| t == "Virtualization Overview"));
        assert!(info.describe_titles.iter().any(|t| t == "nested"));
    }

    #[test]
    fn parses_tags() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_file(
            &dir,
            "tests/tier1/vm/vm.spec.ts",
            r#"test('my test', { tag: ['@tier1', '@nonpriv'] }, async () => {});"#,
        );
        let info = parse_test_file(&path, dir.path());
        assert!(info.tags.contains(&"@tier1".to_string()));
        assert!(info.tags.contains(&"@nonpriv".to_string()));
    }

    #[test]
    fn unknown_tier_for_non_standard_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_file(&dir, "some-other/path.spec.ts", "");
        let info = parse_test_file(&path, dir.path());
        assert_eq!(info.tier, "unknown");
    }

    // ── parse_class_methods ───────────────────────────────────────────────────

    #[test]
    fn extracts_public_method() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_file(
            &dir,
            "src/page-objects/vm-page.ts",
            r#"export default class VirtualMachinePage {
  async clickCreateVm() {}
  private internalHelper() {}
  protected sharedHelper() {}
}"#,
        );
        let methods = parse_class_methods(&path);
        let names: Vec<&str> = methods.iter().map(|m| m.name.as_str()).collect();
        assert!(names.contains(&"clickCreateVm"));
        assert!(names.contains(&"internalHelper"));
        assert!(names.contains(&"sharedHelper"));
        let click = methods.iter().find(|m| m.name == "clickCreateVm").unwrap();
        assert_eq!(click.visibility, Visibility::Public);
        assert!(click.is_async);
        let internal = methods.iter().find(|m| m.name == "internalHelper").unwrap();
        assert_eq!(internal.visibility, Visibility::Private);
    }

    #[test]
    fn skips_constructor_and_control_flow() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_file(
            &dir,
            "src/step-drivers/sd.ts",
            r#"class MyDriver {
  constructor(page) { if (true) {} for (;;) {} }
  doAction() {}
}"#,
        );
        let methods = parse_class_methods(&path);
        let names: Vec<&str> = methods.iter().map(|m| m.name.as_str()).collect();
        assert!(!names.contains(&"constructor"));
        assert!(!names.contains(&"if"));
        assert!(!names.contains(&"for"));
        assert!(names.contains(&"doAction"));
    }

    #[test]
    fn extracts_arrow_function_property() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_file(
            &dir,
            "src/page-objects/x-page.ts",
            r#"class XPage {
  clickButton = async () => { await this.page.click('button'); }
}"#,
        );
        let methods = parse_class_methods(&path);
        let names: Vec<&str> = methods.iter().map(|m| m.name.as_str()).collect();
        assert!(names.contains(&"clickButton"));
    }

    // ── visibility helper ────────────────────────────────────────────────────

    #[test]
    fn parse_visibility_variants() {
        assert_eq!(parse_visibility("private"), Visibility::Private);
        assert_eq!(parse_visibility("protected"), Visibility::Protected);
        assert_eq!(parse_visibility("public"), Visibility::Public);
        assert_eq!(parse_visibility(""), Visibility::Public);
    }
}
