use crate::spec::tools::parser::{SpecFile, Suite, TestCase};
use crate::spec::tools::std_docs::StdDoc;

/// Render a STD doc as the primary content, with spec metadata appended as a reference table.
pub fn std_with_spec_metadata(std_doc: &StdDoc, spec: Option<&SpecFile>) -> String {
    let mut out = std_doc.content.clone();

    if let Some(spec) = spec {
        out.push_str("\n\n---\n\n");
        out.push_str("## Spec Metadata\n\n");
        out.push_str(&format!("_Source: `{}`_\n\n", spec.rel_path));

        let all_tests = collect_all_tests(&spec.suites);
        if all_tests.is_empty() {
            out.push_str("_No tests found in spec._\n");
            return out;
        }

        out.push_str("| Test | Jira | Tags | Status |\n");
        out.push_str("|------|------|------|--------|\n");
        for (suite_name, tc) in &all_tests {
            let name = if tc.skipped { format!("~~{}~~", tc.name) } else { tc.name.clone() };
            let jira = tc
                .jira_id
                .as_deref()
                .map(|id| format!("[{}](https://issues.redhat.com/browse/{})", id, id))
                .unwrap_or_else(|| "—".to_string());
            let tags = if tc.tags.is_empty() {
                "—".to_string()
            } else {
                tc.tags.iter().map(|t| format!("`{}`", t)).collect::<Vec<_>>().join(" ")
            };
            let status = if tc.skipped { "⏭ skip" } else { "✅ active" };
            out.push_str(&format!(
                "| **{}** / {} | {} | {} | {} |\n",
                suite_name, name, jira, tags, status
            ));
        }
    }

    out
}

/// Render spec metadata only (fallback when no STD doc exists).
pub fn spec_metadata_only(spec: &SpecFile) -> String {
    let mut out = String::new();
    out.push_str(&format!("## `{}`\n\n", spec.rel_path));
    out.push_str("> _No STD document found for this spec. Showing spec metadata only._\n\n");

    for suite in &spec.suites {
        render_suite_table(&mut out, suite, 3);
    }

    out
}

fn render_suite_table(out: &mut String, suite: &Suite, level: usize) {
    let hashes = "#".repeat(level.min(6));
    let skip = if suite.skipped { " ~~(skipped)~~" } else { "" };
    let tags = if suite.tags.is_empty() {
        String::new()
    } else {
        format!("  `{}`", suite.tags.join("` `"))
    };
    out.push_str(&format!("{} {}{}{}\n\n", hashes, suite.name, skip, tags));

    if !suite.tests.is_empty() {
        out.push_str("| Test | Jira | Tags | Status |\n");
        out.push_str("|------|------|------|--------|\n");
        for tc in &suite.tests {
            let name = if tc.skipped { format!("~~{}~~", tc.name) } else { tc.name.clone() };
            let jira = tc
                .jira_id
                .as_deref()
                .map(|id| format!("[{}](https://issues.redhat.com/browse/{})", id, id))
                .unwrap_or_else(|| "—".to_string());
            let tags = if tc.tags.is_empty() {
                "—".to_string()
            } else {
                tc.tags.iter().map(|t| format!("`{}`", t)).collect::<Vec<_>>().join(" ")
            };
            let status = if tc.skipped { "⏭ skip" } else { "✅ active" };
            out.push_str(&format!("| {} | {} | {} | {} |\n", name, jira, tags, status));
        }
        out.push('\n');
    }

    for nested in &suite.nested {
        render_suite_table(out, nested, level + 1);
    }
}

/// Flatten all tests with their suite name for the metadata table.
fn collect_all_tests<'a>(suites: &'a [Suite]) -> Vec<(String, &'a TestCase)> {
    let mut result = Vec::new();
    for suite in suites {
        for tc in &suite.tests {
            result.push((suite.name.clone(), tc));
        }
        let nested = collect_all_tests(&suite.nested);
        result.extend(nested);
    }
    result
}

/// Render search results — STD content when available, spec row otherwise.
pub fn search_results_to_markdown(
    query: &str,
    results: &[(&SpecFile, &Suite, &TestCase)],
    docs_root: Option<&str>,
) -> String {
    use crate::spec::tools::std_docs::find_std_for_spec;
    let mut out = String::new();
    out.push_str(&format!("# Search results for `{}`\n\n", query));

    if results.is_empty() {
        out.push_str("_No matching tests found._\n");
        return out;
    }

    out.push_str(&format!("Found **{}** matching test(s).\n\n", results.len()));

    for (spec, suite, tc) in results {
        let name = if tc.skipped { format!("~~{}~~", tc.name) } else { tc.name.clone() };
        let jira = tc
            .jira_id
            .as_deref()
            .map(|id| format!("[{}](https://issues.redhat.com/browse/{})", id, id))
            .unwrap_or_default();

        out.push_str(&format!("### {}\n\n", name));
        out.push_str(&format!("**File:** `{}`  \n**Suite:** {}  \n", spec.rel_path, suite.name));
        if !jira.is_empty() {
            out.push_str(&format!("**Jira:** {}  \n", jira));
        }
        if !tc.tags.is_empty() {
            let tags = tc.tags.iter().map(|t| format!("`{}`", t)).collect::<Vec<_>>().join(" ");
            out.push_str(&format!("**Tags:** {}  \n", tags));
        }
        out.push('\n');

        // Include STD excerpt if available
        if let Some(docs_root) = docs_root {
            let stds = find_std_for_spec(docs_root, &spec.rel_path);
            if let Some(std_doc) = stds.first() {
                // Include just the STD test section that mentions the test name or jira id
                let excerpt = extract_std_excerpt(&std_doc.content, tc);
                if !excerpt.is_empty() {
                    out.push_str(&format!(
                        "<details><summary>STD: {}</summary>\n\n{}\n\n</details>\n\n",
                        std_doc.rel_path, excerpt
                    ));
                }
            }
        }

        out.push_str("---\n\n");
    }

    out
}

/// Extract the relevant section from an STD doc for a given test case.
fn extract_std_excerpt(std_content: &str, tc: &TestCase) -> String {
    let search_terms: Vec<String> = {
        let mut terms = vec![tc.name.to_lowercase()];
        if let Some(ref id) = tc.jira_id {
            terms.push(id.to_lowercase());
        }
        terms
    };

    // Find lines around any heading or row that mentions these terms
    let lines: Vec<&str> = std_content.lines().collect();
    let mut sections: Vec<String> = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let line_lower = lines[i].to_lowercase();
        if search_terms.iter().any(|t| line_lower.contains(t.as_str())) {
            // Find the nearest heading above this line
            let section_start = find_section_start(&lines, i);
            let section_end = find_section_end(&lines, section_start + 1);
            let section: String =
                lines[section_start..section_end.min(lines.len())].join("\n");
            if !sections.contains(&section) {
                sections.push(section);
            }
            i = section_end;
        } else {
            i += 1;
        }
    }

    sections.join("\n\n")
}

fn find_section_start(lines: &[&str], from: usize) -> usize {
    for i in (0..=from).rev() {
        if lines[i].starts_with('#') {
            return i;
        }
    }
    0
}

fn find_section_end(lines: &[&str], from: usize) -> usize {
    for i in from..lines.len() {
        if lines[i].starts_with('#') && i > from {
            return i;
        }
    }
    lines.len()
}
