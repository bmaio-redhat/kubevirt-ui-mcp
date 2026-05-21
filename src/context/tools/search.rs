use std::fmt::Write as FmtWrite;

use serde_json::Value;

use crate::context::indexer::Index;
use crate::mcp::protocol::ToolCallResult;

/// Full-text search over method names and JSDoc across step drivers and page objects.
pub fn search_methods(index: &Index, params: &Value) -> ToolCallResult {
    let query = match params.get("query").and_then(|v| v.as_str()) {
        Some(s) => s.to_lowercase(),
        None => return ToolCallResult::error("Missing required parameter: query"),
    };
    let scope = params
        .get("scope")
        .and_then(|v| v.as_str())
        .unwrap_or("all");

    if query.len() < 2 {
        return ToolCallResult::error("Query must be at least 2 characters.");
    }

    // Tokenise query for multi-word matching
    let terms: Vec<&str> = query.split_whitespace().collect();

    #[derive(Clone)]
    struct Hit {
        class_name: String,
        relative_path: String,
        #[allow(dead_code)]
        method_name: String,
        signature: String,
        jsdoc: Option<String>,
        line: usize,
        score: usize,
    }

    let mut hits: Vec<Hit> = Vec::new();

    let scope_filter = |path: &str| -> bool {
        match scope {
            "step-drivers" => path.contains("step-driver"),
            "page-objects" => path.contains("page-object"),
            _ => path.contains("step-driver") || path.contains("page-object"),
        }
    };

    for cls in index.classes.values() {
        if !scope_filter(&cls.relative_path) {
            continue;
        }

        for method in &cls.methods {
            let name_lower = method.name.to_lowercase();
            let doc_lower = method.jsdoc.as_deref().unwrap_or("").to_lowercase();

            // Score based on match quality
            let mut score = 0usize;

            for term in &terms {
                if name_lower.contains(term) {
                    score += 10;
                    // Bonus for exact match or prefix
                    if name_lower == *term {
                        score += 20;
                    } else if name_lower.starts_with(term) {
                        score += 5;
                    }
                }
                if doc_lower.contains(term) {
                    score += 3;
                }
            }

            if score > 0 {
                hits.push(Hit {
                    class_name: cls.name.clone(),
                    relative_path: cls.relative_path.clone(),
                    method_name: method.name.clone(),
                    signature: method.signature.clone(),
                    jsdoc: method.jsdoc.clone(),
                    line: method.line,
                    score,
                });
            }
        }
    }

    if hits.is_empty() {
        return ToolCallResult::text(format!(
            "No methods found matching '{}' in scope '{}'. Try broader terms or use get_class_surface() for a specific class.",
            query, scope
        ));
    }

    // Sort by score descending, then by class name for stable output
    hits.sort_by(|a, b| b.score.cmp(&a.score).then(a.class_name.cmp(&b.class_name)));

    // Limit results
    let max_results = 30;
    let total = hits.len();
    hits.truncate(max_results);

    let mut out = String::new();
    let _ = writeln!(out, "// Search results for '{}' (scope: {})", query, scope);
    let _ = writeln!(out, "// Found {} matching methods, showing top {}\n", total, hits.len());

    // Group by class
    let mut current_class = String::new();
    for hit in &hits {
        if hit.class_name != current_class {
            if !current_class.is_empty() {
                out.push('\n');
            }
            let _ = writeln!(out, "// ── {} ({})", hit.class_name, hit.relative_path);
            current_class = hit.class_name.clone();
        }
        if let Some(ref doc) = hit.jsdoc {
            let _ = writeln!(out, "  /** {} */", doc);
        }
        let _ = writeln!(out, "  {}  // line {}", hit.signature, hit.line);
    }

    if total > max_results {
        let _ = writeln!(out, "\n// ... {} more results not shown. Use a more specific query.", total - max_results);
    }

    ToolCallResult::text(out)
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::indexer::{ClassInfo, Index, MethodSignature, SymbolExport};
    use serde_json::json;
    use std::path::PathBuf;

    fn make_method(name: &str, jsdoc: Option<&str>) -> MethodSignature {
        MethodSignature {
            name: name.to_string(),
            signature: format!("public async {}(): Promise<void>", name),
            jsdoc: jsdoc.map(|s| s.to_string()),
            line: 1,
            is_public: true,
            is_async: true,
            is_static: false,
        }
    }

    fn make_class(name: &str, dir: &str, methods: Vec<MethodSignature>) -> ClassInfo {
        ClassInfo {
            name: name.to_string(),
            file_path: PathBuf::from(format!("playwright/src/{}/{}.ts", dir, name.to_lowercase())),
            relative_path: format!("playwright/src/{}/{}.ts", dir, name.to_lowercase()),
            extends: None,
            methods,
            selectors: vec![],
        }
    }

    fn build_index(classes: Vec<ClassInfo>) -> Index {
        let mut idx = Index::default();
        for cls in classes {
            idx.exports.insert(
                cls.name.to_lowercase(),
                SymbolExport {
                    name: cls.name.clone(),
                    file_path: cls.file_path.clone(),
                    relative_path: cls.relative_path.clone(),
                },
            );
            idx.classes.insert(cls.name.to_lowercase(), cls);
        }
        idx
    }

    // ── Basic matching ───────────────────────────────────────────────────────

    #[test]
    fn finds_method_by_name() {
        let idx = build_index(vec![make_class(
            "VmStepDriver",
            "step-drivers",
            vec![make_method("takeSnapshot", None), make_method("deleteVm", None)],
        )]);
        let result = search_methods(&idx, &json!({"query": "snapshot"}));
        let text = &result.content[0].text;
        assert!(text.contains("takeSnapshot"), "snapshot method not found");
        assert!(!text.contains("deleteVm"), "unrelated method should not appear");
    }

    #[test]
    fn finds_method_by_jsdoc() {
        let idx = build_index(vec![make_class(
            "VmStepDriver",
            "step-drivers",
            vec![
                make_method("takeAction", Some("Creates a VirtualMachineSnapshot for the current VM")),
                make_method("doOther", None),
            ],
        )]);
        let result = search_methods(&idx, &json!({"query": "snapshot"}));
        let text = &result.content[0].text;
        assert!(text.contains("takeAction"), "jsdoc match should surface the method");
    }

    #[test]
    fn returns_no_results_message_when_not_found() {
        let idx = build_index(vec![make_class(
            "VmStepDriver",
            "step-drivers",
            vec![make_method("clickButton", None)],
        )]);
        let result = search_methods(&idx, &json!({"query": "snapshot"}));
        let text = &result.content[0].text;
        assert!(
            text.contains("No methods found"),
            "should report no results: {}",
            text
        );
    }

    // ── Scope filtering ──────────────────────────────────────────────────────

    #[test]
    fn scope_step_drivers_only() {
        let idx = build_index(vec![
            make_class(
                "VmStepDriver",
                "step-drivers",
                vec![make_method("migrateVm", None)],
            ),
            make_class(
                "VmPage",
                "page-objects",
                vec![make_method("migrateButton", None)],
            ),
        ]);
        let result = search_methods(
            &idx,
            &json!({"query": "migrat", "scope": "step-drivers"}),
        );
        let text = &result.content[0].text;
        assert!(text.contains("migrateVm"), "step driver method should be found");
        assert!(!text.contains("migrateButton"), "page object should be excluded with step-drivers scope");
    }

    #[test]
    fn scope_page_objects_only() {
        let idx = build_index(vec![
            make_class(
                "VmStepDriver",
                "step-drivers",
                vec![make_method("clickMigrateBtn", None)],
            ),
            make_class(
                "VmPage",
                "page-objects",
                vec![make_method("clickMigrateOption", None)],
            ),
        ]);
        let result = search_methods(
            &idx,
            &json!({"query": "migrate", "scope": "page-objects"}),
        );
        let text = &result.content[0].text;
        assert!(text.contains("clickMigrateOption"), "page object method expected");
        assert!(!text.contains("clickMigrateBtn"), "step driver should be excluded");
    }

    // ── Ranking ──────────────────────────────────────────────────────────────

    #[test]
    fn exact_name_match_ranks_higher_than_partial() {
        let idx = build_index(vec![make_class(
            "VmStepDriver",
            "step-drivers",
            vec![
                make_method("snapshot", None),        // exact match
                make_method("takeSnapshot", None),     // partial match
            ],
        )]);
        let result = search_methods(&idx, &json!({"query": "snapshot"}));
        let text = &result.content[0].text;
        // Both should appear; exact match comes first
        let exact_pos = text.find("snapshot(").unwrap_or(usize::MAX);
        let partial_pos = text.find("takeSnapshot").unwrap_or(usize::MAX);
        assert!(exact_pos < partial_pos, "exact match should rank before partial");
    }

    // ── Edge cases ───────────────────────────────────────────────────────────

    #[test]
    fn query_too_short_returns_error() {
        let idx = Index::default();
        let result = search_methods(&idx, &json!({"query": "a"}));
        assert_eq!(result.is_error, Some(true));
    }

    #[test]
    fn missing_query_returns_error() {
        let idx = Index::default();
        let result = search_methods(&idx, &json!({}));
        assert_eq!(result.is_error, Some(true));
    }

    #[test]
    fn empty_index_returns_no_results_not_error() {
        let idx = Index::default();
        let result = search_methods(&idx, &json!({"query": "snapshot"}));
        assert!(result.is_error.is_none(), "empty index should not error");
        assert!(result.content[0].text.contains("No methods found"));
    }
}
