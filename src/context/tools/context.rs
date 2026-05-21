use std::fmt::Write as FmtWrite;

use serde_json::Value;

use crate::context::indexer::Index;
use crate::mcp::protocol::ToolCallResult;

/// Returns minimal context for a task by matching keywords against class names and method names.
pub fn get_task_context(index: &Index, params: &Value) -> ToolCallResult {
    let task = match params.get("task").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return ToolCallResult::error("Missing required parameter: task"),
    };

    let task_lower = task.to_lowercase();
    let keywords = extract_keywords(&task_lower);

    let mut out = String::new();
    let _ = writeln!(out, "// Context for task: \"{}\"\n", task);

    // Determine intent
    let is_test_creation = task_lower.contains("test") || task_lower.contains("spec") || task_lower.contains("create test") || task_lower.contains("add test") || task_lower.contains("write test");
    let is_step_driver_ext = task_lower.contains("step driver") || task_lower.contains("stepdriver") || task_lower.contains("extend step");
    let is_page_object_ext = task_lower.contains("page object") || task_lower.contains("pageobject") || task_lower.contains("extend page") || task_lower.contains("add selector") || task_lower.contains("add method");

    // Find relevant classes
    let mut relevant_classes: Vec<&crate::context::indexer::ClassInfo> = Vec::new();

    for keyword in &keywords {
        let matches = index.find_class(keyword);
        for cls in matches {
            if !relevant_classes.iter().any(|c| c.name == cls.name) {
                relevant_classes.push(cls);
            }
        }
    }

    // Also search by method content
    let method_matches = search_methods_for_keywords(index, &keywords);
    for cls_name in &method_matches {
        if let Some(cls) = index.classes.get(&cls_name.to_lowercase()) {
            if !relevant_classes.iter().any(|c| c.name == cls.name) {
                relevant_classes.push(cls);
            }
        }
    }

    // Limit to most relevant
    relevant_classes.truncate(5);

    if relevant_classes.is_empty() {
        let _ = writeln!(out, "// No directly matching classes found. Showing general framework context.\n");
        // Fall back to general context
        return get_general_framework_context(index, is_test_creation, is_step_driver_ext, is_page_object_ext);
    }

    // For each relevant class, show filtered API surface
    for cls in &relevant_classes {
        let relevant_methods: Vec<_> = cls
            .methods
            .iter()
            .filter(|m| {
                keywords.iter().any(|kw| m.name.to_lowercase().contains(kw.as_str()))
                    || is_test_creation  // for test creation, show all methods
            })
            .take(30)
            .collect();

        if relevant_methods.is_empty() && !is_test_creation {
            // Show all public methods if no keyword match
            let all_methods: Vec<_> = cls.methods.iter().take(20).collect();
            let _ = writeln!(out, "// {} ({}) — {} public methods", cls.name, cls.relative_path, cls.methods.len());
            if let Some(ref parent) = cls.extends {
                let _ = writeln!(out, "// Extends: {}", parent);
            }
            out.push('\n');
            for m in &all_methods {
                if let Some(ref doc) = m.jsdoc {
                    let _ = writeln!(out, "  /** {} */", doc);
                }
                let _ = writeln!(out, "  {}  // line {}", m.signature, m.line);
            }
            if cls.methods.len() > 20 {
                let _ = writeln!(out, "  // ... {} more methods — use get_class_surface('{}') for full list", cls.methods.len() - 20, cls.name);
            }
        } else {
            let _ = writeln!(out, "// {} ({}) — showing {} relevant methods of {}", cls.name, cls.relative_path, relevant_methods.len(), cls.methods.len());
            if let Some(ref parent) = cls.extends {
                let _ = writeln!(out, "// Extends: {}", parent);
            }
            out.push('\n');
            for m in &relevant_methods {
                if let Some(ref doc) = m.jsdoc {
                    let _ = writeln!(out, "  /** {} */", doc);
                }
                let _ = writeln!(out, "  {}  // line {}", m.signature, m.line);
            }
            if cls.methods.len() > relevant_methods.len() {
                let _ = writeln!(out, "  // ... use get_class_surface('{}') for full list", cls.name);
            }
        }
        out.push('\n');
    }

    // Add import hints
    let _ = writeln!(out, "\n// Import guide for referenced classes:");
    for cls in &relevant_classes {
        let import_path = derive_import_path(&cls.relative_path);
        let _ = writeln!(out, "// import {{ {} }} from '{}';", cls.name, import_path);
    }

    // Add fixture reminder if creating a test
    if is_test_creation {
        out.push_str(FIXTURE_REMINDER);
    }

    ToolCallResult::text(out)
}

fn get_general_framework_context(index: &Index, is_test: bool, is_sd: bool, is_po: bool) -> ToolCallResult {
    let mut out = String::new();

    if is_sd {
        // Show step driver base class
        if let Some(cls) = index.find_class("BasePageStepDriver").first().copied() {
            let _ = writeln!(out, "// BasePageStepDriver — extend this for UI step drivers");
            let _ = writeln!(out, "// File: {}\n", cls.relative_path);
            for m in cls.methods.iter().take(15) {
                let _ = writeln!(out, "  {}", m.signature);
            }
        }
    } else if is_po {
        if let Some(cls) = index.find_class("BasePage").first().copied() {
            let _ = writeln!(out, "// BasePage — extend this for page objects");
            let _ = writeln!(out, "// File: {}\n", cls.relative_path);
            for m in cls.methods.iter().take(15) {
                let _ = writeln!(out, "  {}", m.signature);
            }
        }
    } else if is_test {
        out.push_str(FIXTURE_REMINDER);
    }

    ToolCallResult::text(out)
}

/// Returns the compressed fixture interface by reading the fixture file directly.
/// The scenario-test-fixture uses TypeScript types (not classes), so we extract
/// the relevant sections with targeted text extraction rather than AST class parsing.
pub fn get_fixture_api(index: &Index, _params: &Value) -> ToolCallResult {
    // Find the fixture file through the export index
    let fixture_path = index
        .exports
        .values()
        .find(|e| e.relative_path.contains("scenario-test-fixture"))
        .map(|e| e.file_path.clone())
        .or_else(|| {
            // Fallback: look up via relative path hint
            index
                .indexed_files
                .iter()
                .find(|p| p.to_string_lossy().contains("scenario-test-fixture"))
                .cloned()
        });

    let path = match fixture_path {
        Some(p) => p,
        None => {
            return ToolCallResult::error(
                "Could not locate scenario-test-fixture.ts. Ensure KUBEVIRT_PROJECT_ROOT is set correctly.",
            );
        }
    };

    let source = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => return ToolCallResult::error(format!("Could not read fixture file: {}", e)),
    };

    let mut out = String::new();
    let _ = writeln!(out, "// ── scenario-test-fixture — Public API ──────────────────────────────────");
    let _ = writeln!(out, "// Import: import {{ test, expect }} from '@/fixtures/scenario-test-fixture';");
    out.push('\n');

    // Extract the `steps: { ... }` block
    if let Some(steps_block) = extract_steps_block(&source) {
        let _ = writeln!(out, "// test('...', async ({{ steps, cleanup, utils, sharedResources }}) => {{");
        out.push('\n');
        let _ = writeln!(out, "// steps — Available step driver properties:");
        out.push_str(&steps_block);
    }

    out.push('\n');
    out.push_str(FIXTURE_USAGE_NOTE);

    ToolCallResult::text(out)
}

/// Extracts the `steps: { ... }` block from the fixture source.
pub(crate) fn extract_steps_block(source: &str) -> Option<String> {
    // Find `steps: {` pattern
    let steps_start = source.find("steps: {")?;
    let block_start = steps_start + "steps: {".len();

    // Count braces to find the matching closing brace
    let mut depth = 1usize;
    let mut end = block_start;
    for ch in source[block_start..].chars() {
        end += ch.len_utf8();
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            _ => {}
        }
    }

    let block_content = &source[block_start..end - 1]; // exclude trailing }

    // Format each line, stripping TypeScript type annotations to just show property: DriverName
    let formatted: Vec<String> = block_content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let trimmed = l.trim();
            // Keep comment lines
            if trimmed.starts_with("//") || trimmed.starts_with("*") || trimmed.starts_with("/**") || trimmed.starts_with("*/") {
                format!("//  {}", trimmed)
            } else {
                format!("//  {}", trimmed)
            }
        })
        .collect();

    Some(formatted.join("\n") + "\n")
}

/// Project-specific well-known import aliases. Maps common usage names to
/// `(actual_export_name, import_path)` pairs.
static KNOWN_ALIASES: &[(&str, &str, &str)] = &[
    // fixture name variants
    ("scenariotest", "test", "@/fixtures/scenario-test-fixture"),
    ("scenario_test", "test", "@/fixtures/scenario-test-fixture"),
    ("apitest",       "test", "@/fixtures/api-test-fixture"),
    ("api_test",      "test", "@/fixtures/api-test-fixture"),
    ("fleettest",     "test", "@/fixtures/fleet-virtualization-acm-fixture"),
    // common re-exports
    ("expect",        "expect", "@/fixtures/scenario-test-fixture"),
    ("cleanupmanager","CleanupManager", "@/fixtures/cleanup-fixture"),
    ("sharedresource","sharedResources", "@/fixtures/shared-resource-fixture"),
];

/// Returns import paths for requested symbols.
pub fn get_import_guide(index: &Index, params: &Value) -> ToolCallResult {
    let symbols = match params.get("symbols").and_then(|v| v.as_array()) {
        Some(arr) => arr
            .iter()
            .filter_map(|v| v.as_str())
            .collect::<Vec<_>>(),
        None => return ToolCallResult::error("Missing required parameter: symbols (array of strings)"),
    };

    let mut out = String::new();
    let mut found_count = 0;

    for symbol in &symbols {
        let sym_lower = symbol.to_lowercase();

        // Check well-known aliases first
        if let Some((actual, path)) = KNOWN_ALIASES
            .iter()
            .find(|(alias, _, _)| *alias == sym_lower.as_str())
            .map(|(_, actual, path)| (*actual, *path))
        {
            let _ = writeln!(out, "import {{ {} }} from '{}';  // import as: {} as {}", actual, path, actual, symbol);
            found_count += 1;
            continue;
        }

        // Check exports map
        if let Some(exp) = index.exports.get(&sym_lower) {
            let import_path = derive_import_path(&exp.relative_path);
            let _ = writeln!(out, "import {{ {} }} from '{}';", exp.name, import_path);
            found_count += 1;
            continue;
        }

        // Try class map (partial match)
        let classes = index.find_class(symbol);
        if let Some(cls) = classes.first() {
            let import_path = derive_import_path(&cls.relative_path);
            let _ = writeln!(out, "import {{ {} }} from '{}';", cls.name, import_path);
            found_count += 1;
            continue;
        }

        let _ = writeln!(out, "// '{}' not found in index — check spelling or use search_methods('{}')", symbol, symbol);
    }

    if found_count == 0 {
        return ToolCallResult::error(format!(
            "None of the requested symbols were found: {:?}. The index covers playwright/src/. Check the class name.",
            symbols
        ));
    }

    // Add note about @/ alias
    out.push_str("\n// Note: the playwright tsconfig maps '@/*' to 'playwright/src/*'\n");
    out.push_str("// You can also use relative paths from the test file.\n");

    ToolCallResult::text(out)
}

// ── Helpers ──────────────────────────────────────────────────────────────────

pub(crate) fn extract_keywords(task: &str) -> Vec<String> {
    // Extract meaningful words (skip stop words)
    let stop_words = [
        "a", "an", "the", "to", "for", "in", "of", "and", "or", "with", "into",
        "add", "create", "write", "extend", "test", "new", "some", "that", "this",
        "how", "want", "need", "make", "help", "me", "my", "use", "using", "when",
        "from", "on", "is", "be", "it", "i",
    ];

    task.split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase())
        .filter(|w| w.len() > 2 && !stop_words.contains(&w.as_str()))
        .collect()
}

fn search_methods_for_keywords(index: &Index, keywords: &[String]) -> Vec<String> {
    let mut class_names = Vec::new();
    for cls in index.classes.values() {
        for method in &cls.methods {
            for kw in keywords {
                if method.name.to_lowercase().contains(kw.as_str())
                    || method.jsdoc.as_deref().map(|d| d.to_lowercase().contains(kw.as_str())).unwrap_or(false)
                {
                    if !class_names.contains(&cls.name) {
                        class_names.push(cls.name.clone());
                    }
                    break;
                }
            }
        }
    }
    class_names
}

/// Convert a file's relative path to an importable module path.
/// e.g. `playwright/src/step-drivers/vm-step-driver.ts` -> `@/step-drivers/vm-step-driver`
pub(crate) fn derive_import_path(relative_path: &str) -> String {
    let path = relative_path
        .trim_start_matches("playwright/")
        .trim_start_matches("src/")
        .trim_end_matches(".ts")
        .trim_end_matches(".tsx");

    format!("@/{}", path)
}

const FIXTURE_REMINDER: &str = r#"
// ── Test fixture reminder ──────────────────────────────────────────────────
// Use `scenarioTest` from @/fixtures/scenario-test-fixture:
//
//   import { scenarioTest as test } from '@/fixtures/scenario-test-fixture';
//
//   test('test name', { tag: ['@tier1'] }, async ({ steps, page }) => {
//     // steps.vmActions.  — VirtualMachineActionsStepDriver
//     // steps.virtualMachines.  — VirtualMachinesStepDriver
//     // steps.catalog.  — CatalogStepDriver
//     // Use get_fixture_api() to see all available steps properties
//   });
// ───────────────────────────────────────────────────────────────────────────
"#;

const FIXTURE_USAGE_NOTE: &str = r#"// ── Usage ────────────────────────────────────────────────────────────────
// Import: import { scenarioTest as test } from '@/fixtures/scenario-test-fixture';
// The `steps` fixture property holds all step driver instances.
// Use `get_task_context('vm snapshot test')` for task-specific context.
// ─────────────────────────────────────────────────────────────────────────
"#;

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::indexer::{ClassInfo, Index, Indexer, MethodSignature, SymbolExport};
    use serde_json::json;
    use std::path::PathBuf;

    fn make_index_with_class(name: &str, methods: Vec<&str>, relative: &str) -> Index {
        let mut idx = Index::default();
        let ms: Vec<MethodSignature> = methods
            .iter()
            .enumerate()
            .map(|(i, &m)| MethodSignature {
                name: m.to_string(),
                signature: format!("public async {}(): Promise<void>", m),
                jsdoc: None,
                line: i + 10,
                is_public: true,
                is_async: true,
                is_static: false,
            })
            .collect();
        let cls = ClassInfo {
            name: name.to_string(),
            file_path: PathBuf::from(format!("playwright/src/{}", relative)),
            relative_path: format!("playwright/src/{}", relative),
            extends: None,
            methods: ms,
            selectors: vec![],
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

    fn indexer() -> Indexer {
        Indexer::new(PathBuf::from("/fake/playwright"))
    }

    // ── extract_keywords ─────────────────────────────────────────────────────

    #[test]
    fn keywords_filters_stop_words() {
        let kws = extract_keywords("add a vm migration test to tier1");
        assert!(!kws.contains(&"a".to_string()), "stop word 'a' should be filtered");
        assert!(!kws.contains(&"to".to_string()), "stop word 'to' should be filtered");
        assert!(kws.iter().any(|k| k == "migration" || k == "vm" || k == "tier1"));
    }

    #[test]
    fn keywords_lowercases() {
        let kws = extract_keywords("Snapshot Migration");
        assert!(kws.contains(&"snapshot".to_string()));
        assert!(kws.contains(&"migration".to_string()));
    }

    #[test]
    fn keywords_strips_punctuation() {
        let kws = extract_keywords("vm-snapshot, creation");
        // punctuation stripped from edges
        assert!(kws.iter().any(|k| k.contains("snapshot") || k.contains("vm")));
    }

    #[test]
    fn keywords_empty_input() {
        let kws = extract_keywords("");
        assert!(kws.is_empty());
    }

    // ── derive_import_path ───────────────────────────────────────────────────

    #[test]
    fn import_path_strips_playwright_src_prefix() {
        assert_eq!(
            derive_import_path("playwright/src/step-drivers/vm-step-driver.ts"),
            "@/step-drivers/vm-step-driver"
        );
    }

    #[test]
    fn import_path_strips_ts_extension() {
        assert_eq!(
            derive_import_path("playwright/src/page-objects/catalog-page.ts"),
            "@/page-objects/catalog-page"
        );
    }

    #[test]
    fn import_path_without_playwright_prefix() {
        assert_eq!(
            derive_import_path("src/fixtures/scenario-test-fixture.ts"),
            "@/fixtures/scenario-test-fixture"
        );
    }

    // ── extract_steps_block ──────────────────────────────────────────────────

    #[test]
    fn extracts_simple_steps_block() {
        let src = r#"
type TestFixtures = {
  steps: {
    virtualMachines: VirtualMachinesStepDriver;
    catalog: CatalogStepDriver;
  };
  cleanup: CleanupFixture;
};
"#;
        let block = extract_steps_block(src);
        assert!(block.is_some(), "should find steps block");
        let text = block.unwrap();
        assert!(text.contains("VirtualMachinesStepDriver"), "missing step driver type");
        assert!(text.contains("CatalogStepDriver"), "missing catalog driver");
        assert!(!text.contains("CleanupFixture"), "should not include content after closing brace");
    }

    #[test]
    fn steps_block_missing_returns_none() {
        let src = "type Foo = { bar: string; };";
        assert!(extract_steps_block(src).is_none());
    }

    #[test]
    fn steps_block_handles_nested_braces() {
        let src = r#"
type T = {
  steps: {
    vm: { nested: SomeType };
    catalog: CatalogStepDriver;
  };
};
"#;
        let block = extract_steps_block(src);
        assert!(block.is_some());
        let text = block.unwrap();
        assert!(text.contains("CatalogStepDriver"));
    }

    // ── get_import_guide ─────────────────────────────────────────────────────

    #[test]
    fn import_guide_finds_class_in_index() {
        let idx = make_index_with_class(
            "VirtualMachinesStepDriver",
            vec![],
            "step-drivers/virtual-machines-step-driver.ts",
        );
        let result = get_import_guide(&idx, &json!({"symbols": ["VirtualMachinesStepDriver"]}));
        let text = &result.content[0].text;
        assert!(text.contains("VirtualMachinesStepDriver"), "class name missing");
        assert!(text.contains("@/step-drivers"), "import path missing");
    }

    #[test]
    fn import_guide_known_alias_scenariotest() {
        let idx = Index::default();
        let result = get_import_guide(&idx, &json!({"symbols": ["scenarioTest"]}));
        let text = &result.content[0].text;
        assert!(text.contains("scenario-test-fixture"), "fixture path missing");
        assert!(result.is_error.is_none(), "should not be an error");
    }

    #[test]
    fn import_guide_known_alias_apitest() {
        let idx = Index::default();
        let result = get_import_guide(&idx, &json!({"symbols": ["apiTest"]}));
        let text = &result.content[0].text;
        assert!(text.contains("api-test-fixture"), "api fixture path missing");
    }

    #[test]
    fn import_guide_missing_symbols_returns_error() {
        let idx = Index::default();
        let result = get_import_guide(&idx, &json!({"symbols": ["NonExistentClass"]}));
        assert_eq!(result.is_error, Some(true));
    }

    #[test]
    fn import_guide_missing_param_returns_error() {
        let idx = Index::default();
        let result = get_import_guide(&idx, &json!({}));
        assert_eq!(result.is_error, Some(true));
    }

    #[test]
    fn import_guide_partial_match_via_class_lookup() {
        let idx = make_index_with_class(
            "CatalogStepDriver",
            vec![],
            "step-drivers/catalog-step-driver.ts",
        );
        let result = get_import_guide(&idx, &json!({"symbols": ["Catalog"]}));
        let text = &result.content[0].text;
        assert!(text.contains("CatalogStepDriver"), "partial match should work");
    }

    // ── get_task_context ─────────────────────────────────────────────────────

    #[test]
    fn task_context_finds_relevant_class() {
        let idx = make_index_with_class(
            "MigrationPoliciesStepDriver",
            vec!["createMigrationPolicy", "deleteMigrationPolicy"],
            "step-drivers/migration-policies-step-driver.ts",
        );
        let result = get_task_context(&idx, &json!({"task": "add migration policy test"}));
        let text = &result.content[0].text;
        assert!(text.contains("MigrationPoliciesStepDriver"), "relevant class should appear");
    }

    #[test]
    fn task_context_missing_param_returns_error() {
        let idx = Index::default();
        let result = get_task_context(&idx, &json!({}));
        assert_eq!(result.is_error, Some(true));
    }

    #[test]
    fn task_context_includes_fixture_reminder_for_test_creation() {
        let idx = make_index_with_class("SomePage", vec![], "page-objects/some-page.ts");
        let result = get_task_context(&idx, &json!({"task": "create a new test for snapshots"}));
        let text = &result.content[0].text;
        assert!(
            text.contains("scenarioTest") || text.contains("fixture"),
            "fixture reminder should appear for test creation tasks"
        );
    }

    // ── get_fixture_api (real file from indexer) ─────────────────────────────

    #[test]
    fn fixture_api_with_inline_source() {
        let src = r#"
type TestFixtures = {
  steps: {
    login: LoginStepDriver;
    virtualMachines: VirtualMachinesStepDriver;
  };
  cleanup: CleanupFixture;
};
export const test = {} as any;
"#;
        // Parse the source through the indexer so indexed_files is populated
        let idx = indexer().parse_source(src, "fixtures/scenario-test-fixture");
        // get_fixture_api needs the file on disk — it reads the actual file for step block extraction.
        // Instead, test extract_steps_block directly here (which get_fixture_api delegates to).
        let block = extract_steps_block(src);
        assert!(block.is_some());
        assert!(block.unwrap().contains("VirtualMachinesStepDriver"));

        // Verify export was registered under "test"
        assert!(idx.exports.contains_key("test"), "test export should be indexed");
    }
}
