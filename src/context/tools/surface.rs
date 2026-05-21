use std::fmt::Write as FmtWrite;

use serde_json::Value;

use crate::context::indexer::Index;
use crate::mcp::protocol::ToolCallResult;

/// Returns the public API surface of a class — method signatures + JSDoc only.
pub fn get_class_surface(index: &Index, params: &Value) -> ToolCallResult {
    let class_name = match params.get("class_name").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return ToolCallResult::error("Missing required parameter: class_name"),
    };
    let filter = params.get("filter").and_then(|v| v.as_str()).map(|s| s.to_lowercase());

    let classes = index.find_class(class_name);
    if classes.is_empty() {
        // Try to give a helpful message
        let available: Vec<&str> = index
            .classes
            .values()
            .filter(|c| {
                c.relative_path.contains("step-driver") || c.relative_path.contains("page-object")
            })
            .map(|c| c.name.as_str())
            .take(20)
            .collect();
        return ToolCallResult::error(format!(
            "No class found matching '{}'. Available step drivers and page objects include: {}",
            class_name,
            available.join(", ")
        ));
    }

    let mut out = String::new();

    for cls in &classes {
        let methods: Vec<_> = cls
            .methods
            .iter()
            .filter(|m| {
                if let Some(ref f) = filter {
                    let name_match = m.name.to_lowercase().contains(f.as_str());
                    let doc_match = m
                        .jsdoc
                        .as_deref()
                        .map(|d| d.to_lowercase().contains(f.as_str()))
                        .unwrap_or(false);
                    name_match || doc_match
                } else {
                    true
                }
            })
            .collect();

        let _ = writeln!(
            out,
            "// ── {} ────────────────────────────────────",
            cls.name
        );
        let _ = writeln!(out, "// File: {}", cls.relative_path);
        if let Some(ref parent) = cls.extends {
            let _ = writeln!(out, "// Extends: {}", parent);
        }
        let _ = writeln!(out, "// {} public methods", methods.len());
        out.push('\n');

        for m in &methods {
            if let Some(ref doc) = m.jsdoc {
                let _ = writeln!(out, "  /** {} */", doc);
            }
            let _ = writeln!(out, "  {}  // line {}", m.signature, m.line);
        }

        if classes.len() > 1 {
            out.push('\n');
        }
    }

    if out.is_empty() {
        return ToolCallResult::error(format!(
            "Class '{}' was found but has no public methods matching the filter.",
            class_name
        ));
    }

    ToolCallResult::text(out)
}

/// Returns all selectors defined in a page object class.
pub fn get_selector_map(index: &Index, params: &Value) -> ToolCallResult {
    let class_name = match params.get("class_name").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return ToolCallResult::error("Missing required parameter: class_name"),
    };

    let classes = index.find_class(class_name);
    if classes.is_empty() {
        return ToolCallResult::error(format!("No class found matching '{}'", class_name));
    }

    let mut out = String::new();

    for cls in &classes {
        if cls.selectors.is_empty() {
            let _ = writeln!(out, "// {} — no selectors found", cls.name);
            continue;
        }

        let _ = writeln!(out, "// Selectors in {} ({})", cls.name, cls.relative_path);
        let _ = writeln!(out);

        // Group by type
        let mut by_type: std::collections::HashMap<&str, Vec<&crate::context::indexer::SelectorInfo>> =
            std::collections::HashMap::new();
        for s in &cls.selectors {
            by_type.entry(s.kind.as_str()).or_default().push(s);
        }

        for (kind, sels) in &by_type {
            let _ = writeln!(out, "  // {} selectors:", kind);
            for s in sels {
                let _ = writeln!(out, "  [{}=\"{}\"]  // line {}", kind, s.value, s.line);
            }
            out.push('\n');
        }
    }

    ToolCallResult::text(out)
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::indexer::{ClassInfo, Index, MethodSignature, SelectorInfo, SymbolExport};
    use serde_json::json;
    use std::path::PathBuf;

    fn make_method(name: &str, jsdoc: Option<&str>) -> MethodSignature {
        MethodSignature {
            name: name.to_string(),
            signature: format!("public async {}(): Promise<void>", name),
            jsdoc: jsdoc.map(|s| s.to_string()),
            line: 10,
            is_public: true,
            is_async: true,
            is_static: false,
        }
    }

    fn make_selector(value: &str, kind: &str) -> SelectorInfo {
        SelectorInfo { value: value.to_string(), kind: kind.to_string(), line: 5 }
    }

    fn index_with(name: &str, methods: Vec<MethodSignature>, selectors: Vec<SelectorInfo>) -> Index {
        let mut idx = Index::default();
        let cls = ClassInfo {
            name: name.to_string(),
            file_path: PathBuf::from("playwright/src/page-objects/test-page.ts"),
            relative_path: "playwright/src/page-objects/test-page.ts".to_string(),
            extends: Some("BasePage".to_string()),
            methods,
            selectors,
        };
        idx.exports.insert(
            name.to_lowercase(),
            SymbolExport {
                name: name.to_string(),
                file_path: cls.file_path.clone(),
                relative_path: cls.relative_path.clone(),
            },
        );
        idx.classes.insert(name.to_lowercase(), cls);
        idx
    }

    // ── get_class_surface ────────────────────────────────────────────────────

    #[test]
    fn surface_contains_class_name() {
        let idx = index_with("VmPage", vec![make_method("navigate", None)], vec![]);
        let result = get_class_surface(&idx, &json!({"class_name": "VmPage"}));
        assert!(result.content[0].text.contains("VmPage"));
    }

    #[test]
    fn surface_contains_method_signature() {
        let idx = index_with(
            "VmPage",
            vec![make_method("clickCreate", None)],
            vec![],
        );
        let result = get_class_surface(&idx, &json!({"class_name": "VmPage"}));
        let text = &result.content[0].text;
        assert!(text.contains("clickCreate"), "method name missing");
        assert!(text.contains("Promise<void>"), "return type missing");
    }

    #[test]
    fn surface_contains_extends_info() {
        let idx = index_with("VmPage", vec![], vec![]);
        let result = get_class_surface(&idx, &json!({"class_name": "VmPage"}));
        assert!(result.content[0].text.contains("BasePage"), "extends clause missing");
    }

    #[test]
    fn surface_shows_jsdoc() {
        let idx = index_with(
            "VmPage",
            vec![make_method("navigate", Some("Navigates to the VM list."))],
            vec![],
        );
        let result = get_class_surface(&idx, &json!({"class_name": "VmPage"}));
        assert!(result.content[0].text.contains("Navigates to the VM list."));
    }

    #[test]
    fn surface_filter_narrows_methods() {
        let idx = index_with(
            "VmPage",
            vec![
                make_method("takeSnapshot", None),
                make_method("deleteVm", None),
                make_method("restoreSnapshot", None),
            ],
            vec![],
        );
        let result = get_class_surface(
            &idx,
            &json!({"class_name": "VmPage", "filter": "snapshot"}),
        );
        let text = &result.content[0].text;
        assert!(text.contains("takeSnapshot"), "snapshot method should pass filter");
        assert!(text.contains("restoreSnapshot"), "restore snapshot should pass filter");
        assert!(!text.contains("deleteVm"), "unrelated method should be filtered out");
    }

    #[test]
    fn surface_partial_class_name_match() {
        let idx = index_with("VirtualMachinesPage", vec![make_method("navigate", None)], vec![]);
        let result = get_class_surface(&idx, &json!({"class_name": "VirtualMachines"}));
        assert!(result.content[0].text.contains("VirtualMachinesPage"));
        assert!(result.is_error.is_none());
    }

    #[test]
    fn surface_unknown_class_returns_error() {
        let idx = index_with("VmPage", vec![], vec![]);
        let result = get_class_surface(&idx, &json!({"class_name": "NonExistentClass"}));
        assert_eq!(result.is_error, Some(true));
    }

    #[test]
    fn surface_missing_class_name_returns_error() {
        let idx = Index::default();
        let result = get_class_surface(&idx, &json!({}));
        assert_eq!(result.is_error, Some(true));
    }

    #[test]
    fn surface_includes_file_path() {
        let idx = index_with("VmPage", vec![], vec![]);
        let result = get_class_surface(&idx, &json!({"class_name": "VmPage"}));
        assert!(result.content[0].text.contains("page-objects/test-page.ts"));
    }

    // ── get_selector_map ─────────────────────────────────────────────────────

    #[test]
    fn selector_map_lists_data_test() {
        let idx = index_with(
            "VmPage",
            vec![],
            vec![make_selector("create-vm-btn", "data-test")],
        );
        let result = get_selector_map(&idx, &json!({"class_name": "VmPage"}));
        let text = &result.content[0].text;
        assert!(text.contains("create-vm-btn"));
        assert!(text.contains("data-test"));
    }

    #[test]
    fn selector_map_groups_by_type() {
        let idx = index_with(
            "VmPage",
            vec![],
            vec![
                make_selector("create-btn", "data-test"),
                make_selector("submit-id", "data-test-id"),
                make_selector("button", "role"),
            ],
        );
        let result = get_selector_map(&idx, &json!({"class_name": "VmPage"}));
        let text = &result.content[0].text;
        // All three types should appear
        assert!(text.contains("data-test"), "data-test group missing");
        assert!(text.contains("data-test-id"), "data-test-id group missing");
        assert!(text.contains("role"), "role group missing");
    }

    #[test]
    fn selector_map_unknown_class_returns_error() {
        let idx = Index::default();
        let result = get_selector_map(&idx, &json!({"class_name": "NoSuchPage"}));
        assert_eq!(result.is_error, Some(true));
    }

    #[test]
    fn selector_map_empty_selectors_noted() {
        let idx = index_with("VmPage", vec![], vec![]);
        let result = get_selector_map(&idx, &json!({"class_name": "VmPage"}));
        let text = &result.content[0].text;
        assert!(text.contains("no selectors"), "should indicate no selectors found");
    }

    #[test]
    fn selector_map_missing_class_name_returns_error() {
        let idx = Index::default();
        let result = get_selector_map(&idx, &json!({}));
        assert_eq!(result.is_error, Some(true));
    }
}
