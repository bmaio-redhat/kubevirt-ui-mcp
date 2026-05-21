use regex::Regex;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct SpecFile {
    pub rel_path: String,
    pub suites: Vec<Suite>,
}

#[derive(Debug, Clone)]
pub struct Suite {
    pub name: String,
    pub tags: Vec<String>,
    pub skipped: bool,
    pub tests: Vec<TestCase>,
    pub nested: Vec<Suite>,
}

#[derive(Debug, Clone)]
pub struct TestCase {
    pub name: String,
    pub jira_id: Option<String>,
    pub tags: Vec<String>,
    pub skipped: bool,
}

pub fn parse_spec(source: &str, rel_path: &str) -> SpecFile {
    let consts = extract_string_consts(source);
    let suites = parse_suites(source, &consts);
    SpecFile { rel_path: rel_path.to_string(), suites }
}

fn extract_string_consts(source: &str) -> HashMap<String, String> {
    let re = Regex::new(r#"const\s+([A-Z_][A-Z0-9_]*)\s*=\s*['"`]([^'"`]+)['"`]"#).unwrap();
    re.captures_iter(source).map(|c| (c[1].to_string(), c[2].to_string())).collect()
}

fn parse_suites(source: &str, consts: &HashMap<String, String>) -> Vec<Suite> {
    let re_desc = Regex::new(
        r#"test\.describe(?:\.(skip|only))?\s*\(\s*(?:['"`]([^'"`]+)['"`]|([A-Z_][A-Z0-9_]*))"#,
    )
    .unwrap();
    let re_test_inline =
        Regex::new(r#"^\s*test(?:\.(skip|only))?\s*\(\s*['"`]([^'"`]+)['"`]"#).unwrap();
    let re_test_open = Regex::new(r#"^\s*test(?:\.(skip|only))?\s*\(\s*$"#).unwrap();
    let re_jira = Regex::new(r#"ID\((CNV-\d+)\)"#).unwrap();
    let re_tag = Regex::new(r#"['"](@[\w\-]+)['"]"#).unwrap();

    struct DepthLine<'a> {
        text: &'a str,
        depth: i32,
    }

    let mut depth: i32 = 0;
    let lines: Vec<DepthLine> = source
        .lines()
        .map(|line| {
            let open = line.chars().filter(|&c| c == '{').count() as i32;
            let close = line.chars().filter(|&c| c == '}').count() as i32;
            let d = depth;
            depth += open - close;
            DepthLine { text: line, depth: d }
        })
        .collect();

    let mut stack: Vec<(i32, Suite)> = Vec::new();
    let mut top_suites: Vec<Suite> = Vec::new();

    for (idx, line) in lines.iter().enumerate() {
        let text = line.text;
        let depth = line.depth;

        // Pop completed suites
        while let Some((open_depth, _)) = stack.last() {
            if depth <= *open_depth {
                let (_, suite) = stack.pop().unwrap();
                if let Some((_, parent)) = stack.last_mut() {
                    parent.nested.push(suite);
                } else {
                    top_suites.push(suite);
                }
            } else {
                break;
            }
        }

        // describe
        if let Some(cap) = re_desc.captures(text) {
            let modifier = cap.get(1).map(|m| m.as_str()).unwrap_or("");
            let raw = cap.get(2).or_else(|| cap.get(3)).map(|m| m.as_str()).unwrap_or("");
            let name = consts.get(raw).cloned().unwrap_or_else(|| raw.to_string());
            let tags = extract_tags(text, &re_tag);
            stack.push((depth, Suite { name, tags, skipped: modifier == "skip", tests: vec![], nested: vec![] }));
            continue;
        }

        // test — may be inline or multi-line
        let joined_opt: String;
        let test_text = if re_test_open.is_match(text) {
            joined_opt = lines.iter().skip(idx).take(6).map(|l| l.text.trim()).collect::<Vec<_>>().join(" ");
            joined_opt.as_str()
        } else {
            text
        };

        if let Some(cap) = re_test_inline.captures(test_text) {
            let modifier = cap.get(1).map(|m| m.as_str()).unwrap_or("");
            let raw_name = cap.get(2).map(|m| m.as_str()).unwrap_or("").to_string();
            let jira_id = re_jira.captures(&raw_name).map(|c| c[1].to_string());
            let name = re_jira.replace(&raw_name, "").trim().to_string();

            // Tags from this line + next few lines
            let lookahead: String = lines.iter().skip(idx).take(6).map(|l| l.text).collect::<Vec<_>>().join("\n");
            let tags = extract_tags(&lookahead, &re_tag);

            let tc = TestCase { name, jira_id, tags, skipped: modifier == "skip" };
            if let Some((_, suite)) = stack.last_mut() {
                suite.tests.push(tc);
            }
        }
    }

    // Drain stack
    while let Some((_, suite)) = stack.pop() {
        if let Some((_, parent)) = stack.last_mut() {
            parent.nested.push(suite);
        } else {
            top_suites.push(suite);
        }
    }

    top_suites
}

fn extract_tags(text: &str, re_tag: &Regex) -> Vec<String> {
    re_tag.captures_iter(text).map(|c| c[1].to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── parse_spec ────────────────────────────────────────────────────────────

    #[test]
    fn inline_test_with_jira_and_tag() {
        let src = r#"
test.describe('My suite', { tag: ['@tier1'] }, () => {
  test('ID(CNV-1234) do the thing', { tag: ['@nonpriv'] }, async ({ page }) => {});
});
"#;
        let spec = parse_spec(src, "tier1/foo/foo.spec.ts");
        assert_eq!(spec.suites.len(), 1);
        let suite = &spec.suites[0];
        assert_eq!(suite.name, "My suite");
        assert_eq!(suite.tests.len(), 1);
        let tc = &suite.tests[0];
        assert_eq!(tc.name, "do the thing");
        assert_eq!(tc.jira_id.as_deref(), Some("CNV-1234"));
        assert!(tc.tags.contains(&"@nonpriv".to_string()));
    }

    #[test]
    fn multiline_test_declaration() {
        let src = r#"
test.describe('Suite', {}, () => {
  test(
    'ID(CNV-9999) multiline test name',
    { tag: ['@nonpriv'] },
    async ({ page }) => {},
  );
});
"#;
        let spec = parse_spec(src, "tier1/foo/foo.spec.ts");
        let tc = &spec.suites[0].tests[0];
        assert_eq!(tc.name, "multiline test name");
        assert_eq!(tc.jira_id.as_deref(), Some("CNV-9999"));
        assert!(tc.tags.contains(&"@nonpriv".to_string()));
    }

    #[test]
    fn skipped_describe_and_test() {
        let src = r#"
test.describe.skip('Skipped suite', {}, () => {
  test.skip('skipped test', async ({}) => {});
});
"#;
        let spec = parse_spec(src, "tier2/foo.spec.ts");
        let suite = &spec.suites[0];
        assert!(suite.skipped, "describe should be skipped");
        assert!(suite.tests[0].skipped, "test should be skipped");
    }

    #[test]
    fn const_suite_name_resolved() {
        let src = r#"
const SUITE = 'Resolved Name';
test.describe(SUITE, { tag: ['@tier1'] }, () => {
  test('a test', async ({}) => {});
});
"#;
        let spec = parse_spec(src, "tier1/foo.spec.ts");
        assert_eq!(spec.suites[0].name, "Resolved Name");
    }

    #[test]
    fn nested_describe() {
        let src = r#"
test.describe('Outer', {}, () => {
  test.describe('Inner', {}, () => {
    test('nested test', async ({}) => {});
  });
});
"#;
        let spec = parse_spec(src, "tier1/foo.spec.ts");
        assert_eq!(spec.suites.len(), 1);
        let outer = &spec.suites[0];
        assert_eq!(outer.name, "Outer");
        assert_eq!(outer.nested.len(), 1);
        assert_eq!(outer.nested[0].name, "Inner");
        assert_eq!(outer.nested[0].tests[0].name, "nested test");
    }

    #[test]
    fn no_jira_id_is_none() {
        let src = r#"
test.describe('Suite', {}, () => {
  test('plain test name', async ({}) => {});
});
"#;
        let spec = parse_spec(src, "tier1/foo.spec.ts");
        assert!(spec.suites[0].tests[0].jira_id.is_none());
    }

    // ── feature_md_candidates (via find_std_for_spec indirectly) ─────────────

    #[test]
    fn jira_id_stripped_from_name() {
        let src = r#"
test.describe('S', {}, () => {
  test('ID(CNV-100) name remains', async ({}) => {});
});
"#;
        let spec = parse_spec(src, "foo.spec.ts");
        assert_eq!(spec.suites[0].tests[0].name, "name remains");
    }
}

pub fn find_spec_files(root: &str) -> Vec<String> {
    use walkdir::WalkDir;
    let mut paths = Vec::new();
    let root_path = Path::new(root);
    for entry in WalkDir::new(root).follow_links(true).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_file() {
            if path.file_name().map(|n| n.to_string_lossy().ends_with(".spec.ts")).unwrap_or(false) {
                if let Ok(rel) = path.strip_prefix(root_path) {
                    paths.push(rel.to_string_lossy().to_string());
                }
            }
        }
    }
    paths.sort();
    paths
}
