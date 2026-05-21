#![allow(dead_code)]
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tree_sitter::{Language, Node, Parser};
use walkdir::WalkDir;

// ── Data structures ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct MethodSignature {
    pub name: String,
    /// Reconstructed signature line, e.g. `public async createVm(name: string): Promise<void>`
    pub signature: String,
    /// JSDoc comment block preceding the method (stripped of `/** */` delimiters)
    pub jsdoc: Option<String>,
    pub line: usize,
    pub is_public: bool,
    pub is_async: bool,
    pub is_static: bool,
}

#[derive(Debug, Clone)]
pub struct SelectorInfo {
    pub value: String,
    pub kind: String, // "data-test", "data-test-id", "role"
    pub line: usize,
}

#[derive(Debug, Clone)]
pub struct ClassInfo {
    pub name: String,
    pub file_path: PathBuf,
    /// Path relative to playwright/ root
    pub relative_path: String,
    pub extends: Option<String>,
    pub methods: Vec<MethodSignature>,
    pub selectors: Vec<SelectorInfo>,
}

/// Symbol → file mapping for import resolution.
#[derive(Debug, Clone)]
pub struct SymbolExport {
    /// Class/function/type name
    pub name: String,
    pub file_path: PathBuf,
    pub relative_path: String,
}

/// The full in-memory project index.
#[derive(Debug, Default)]
pub struct Index {
    /// class name (lowercase) → ClassInfo
    pub classes: HashMap<String, ClassInfo>,
    /// symbol name (lowercase) → SymbolExport
    pub exports: HashMap<String, SymbolExport>,
    /// List of all indexed file paths
    pub indexed_files: Vec<PathBuf>,
}

impl Index {
    /// Find a class by exact or partial name (case-insensitive).
    pub fn find_class(&self, name: &str) -> Vec<&ClassInfo> {
        let needle = name.to_lowercase();
        let mut exact: Vec<&ClassInfo> = self
            .classes
            .get(&needle)
            .map(|c| vec![c])
            .unwrap_or_default();

        if exact.is_empty() {
            exact = self
                .classes
                .values()
                .filter(|c| c.name.to_lowercase().contains(&needle))
                .collect();
            // Sort by how early the match appears, then by name length (prefer exact prefix)
            exact.sort_by_key(|c| {
                let pos = c.name.to_lowercase().find(&needle).unwrap_or(usize::MAX);
                (pos, c.name.len())
            });
        }
        exact
    }

    /// Find all classes whose file path contains `dir_fragment`.
    pub fn classes_in_dir(&self, dir_fragment: &str) -> Vec<&ClassInfo> {
        self.classes
            .values()
            .filter(|c| c.relative_path.contains(dir_fragment))
            .collect()
    }
}

// ── Indexer ──────────────────────────────────────────────────────────────────

pub struct Indexer {
    language: Language,
    playwright_root: PathBuf,
}

impl Indexer {
    pub fn new(playwright_root: PathBuf) -> Self {
        let language: Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
        Self { language, playwright_root }
    }

    /// Build a full index from the playwright source directories.
    pub fn build(&self) -> Index {
        let mut index = Index::default();

        let scan_dirs = [
            self.playwright_root.join("src/page-objects"),
            self.playwright_root.join("src/step-drivers"),
            self.playwright_root.join("src/fixtures"),
            self.playwright_root.join("src/clients"),
            self.playwright_root.join("src/utils"),
            self.playwright_root.join("src/data-models"),
        ];

        for dir in &scan_dirs {
            if dir.exists() {
                self.index_dir(dir, &mut index);
            }
        }

        index
    }

    fn index_dir(&self, dir: &Path, index: &mut Index) {
        for entry in WalkDir::new(dir)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.is_file()
                && path.extension().and_then(|e| e.to_str()) == Some("ts")
                && !path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.ends_with(".d.ts") || n == "index.ts")
                    .unwrap_or(false)
            {
                self.index_file(path, index);
            }
        }
    }

    /// Parse a TypeScript source string directly into an Index — used in tests.
    #[cfg(test)]
    pub fn parse_source(&self, source: &str, label: &str) -> Index {
        let mut index = Index::default();
        let fake_path = std::path::PathBuf::from(format!("playwright/src/{}.ts", label));
        self.parse_source_into(source, &fake_path, label, &mut index);
        index
    }

    #[cfg(test)]
    fn parse_source_into(&self, source: &str, path: &Path, relative: &str, index: &mut Index) {
        let mut parser = tree_sitter::Parser::new();
        if parser.set_language(&self.language).is_err() {
            return;
        }
        let tree = match parser.parse(source, None) {
            Some(t) => t,
            None => return,
        };
        let source_bytes = source.as_bytes();
        let root = tree.root_node();
        let classes = extract_classes(root, source_bytes, path, relative);
        for cls in classes {
            let key = cls.name.to_lowercase();
            index.exports.insert(key.clone(), SymbolExport {
                name: cls.name.clone(),
                file_path: path.to_path_buf(),
                relative_path: relative.to_string(),
            });
            index.classes.insert(key, cls);
        }
        let top = extract_top_level_exports(root, source_bytes, path, relative);
        for exp in top {
            index.exports.entry(exp.name.to_lowercase()).or_insert(exp);
        }
        index.indexed_files.push(path.to_path_buf());
    }

    fn index_file(&self, path: &Path, index: &mut Index) {
        let source = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => return,
        };

        let relative = path
            .strip_prefix(self.playwright_root.parent().unwrap_or(&self.playwright_root))
            .unwrap_or(path)
            .to_string_lossy()
            .into_owned();

        index.indexed_files.push(path.to_path_buf());

        let mut parser = Parser::new();
        if parser.set_language(&self.language).is_err() {
            return;
        }

        let tree = match parser.parse(&source, None) {
            Some(t) => t,
            None => return,
        };

        let source_bytes = source.as_bytes();
        let root = tree.root_node();

        // Extract all classes from this file
        let classes = extract_classes(root, source_bytes, path, &relative);
        for cls in classes {
            let key = cls.name.to_lowercase();
            // Also register each exported symbol
            index.exports.insert(
                key.clone(),
                SymbolExport {
                    name: cls.name.clone(),
                    file_path: path.to_path_buf(),
                    relative_path: relative.clone(),
                },
            );
            index.classes.insert(key, cls);
        }

        // Also extract non-class exports (exported functions, types, consts)
        let top_exports = extract_top_level_exports(root, source_bytes, path, &relative);
        for exp in top_exports {
            index.exports.entry(exp.name.to_lowercase()).or_insert(exp);
        }
    }
}

// ── AST walking helpers ──────────────────────────────────────────────────────

fn extract_classes(
    root: Node,
    source: &[u8],
    file_path: &Path,
    relative_path: &str,
) -> Vec<ClassInfo> {
    let mut classes = Vec::new();
    walk_for_classes(root, source, file_path, relative_path, &mut classes);
    classes
}

fn walk_for_classes(
    node: Node,
    source: &[u8],
    file_path: &Path,
    relative_path: &str,
    out: &mut Vec<ClassInfo>,
) {
    let kind = node.kind();

    if matches!(kind, "class_declaration" | "abstract_class_declaration") {
        if let Some(cls) = parse_class(node, source, file_path, relative_path) {
            out.push(cls);
        }
        // Don't descend into class bodies — inner classes are very rare in this codebase
        return;
    }

    // Recurse into children, but skip function/method bodies (too deep, no classes there)
    if matches!(
        kind,
        "function_body" | "statement_block" | "arrow_function"
    ) {
        return;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_for_classes(child, source, file_path, relative_path, out);
    }
}

fn parse_class(
    node: Node,
    source: &[u8],
    file_path: &Path,
    relative_path: &str,
) -> Option<ClassInfo> {
    let name = node
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(source).ok())
        .map(|s| s.to_string())?;

    let extends = node
        .child_by_field_name("body")
        .and_then(|_| {
            // Look for class_heritage -> extends_clause -> type_identifier
            find_extends(node, source)
        });

    let body = node.child_by_field_name("body")?;
    let (methods, selectors) = parse_class_body(body, source);

    Some(ClassInfo {
        name,
        file_path: file_path.to_path_buf(),
        relative_path: relative_path.to_string(),
        extends,
        methods,
        selectors,
    })
}

fn find_extends(class_node: Node, source: &[u8]) -> Option<String> {
    let mut cursor = class_node.walk();
    for child in class_node.children(&mut cursor) {
        if child.kind() == "class_heritage" {
            let mut hcursor = child.walk();
            for hchild in child.children(&mut hcursor) {
                if hchild.kind() == "extends_clause" {
                    let mut ecursor = hchild.walk();
                    for echild in hchild.children(&mut ecursor) {
                        if matches!(echild.kind(), "type_identifier" | "identifier") {
                            return echild.utf8_text(source).ok().map(|s| s.to_string());
                        }
                    }
                }
            }
        }
    }
    None
}

fn parse_class_body(body: Node, source: &[u8]) -> (Vec<MethodSignature>, Vec<SelectorInfo>) {
    let mut methods = Vec::new();
    let mut selectors = Vec::new();
    let mut prev_comment: Option<String> = None;

    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        let kind = child.kind();

        match kind {
            "comment" => {
                let text = child.utf8_text(source).unwrap_or("").trim().to_string();
                if text.starts_with("/**") || text.starts_with("//") {
                    prev_comment = Some(clean_jsdoc(&text));
                } else {
                    prev_comment = None;
                }
                continue;
            }
            "method_definition" => {
                if let Some(m) = parse_method(child, source, prev_comment.take()) {
                    methods.push(m);
                }
            }
            "public_field_definition" => {
                // Arrow function properties: `myMethod = async (...) => { ... }`
                if let Some(m) = parse_field_method(child, source, prev_comment.take()) {
                    methods.push(m);
                }
                // Also check for selector strings in field definitions
                collect_selectors_from_node(child, source, &mut selectors);
            }
            "{" | "}" => {}
            _ => {
                collect_selectors_from_node(child, source, &mut selectors);
            }
        }

        if kind != "comment" {
            prev_comment = None;
        }
    }

    // Also scan the whole body text for selectors in method bodies
    scan_body_for_selectors(body, source, &mut selectors);

    (methods, selectors)
}

fn parse_method(node: Node, source: &[u8], jsdoc: Option<String>) -> Option<MethodSignature> {
    let mut accessibility = "public".to_string();
    let mut is_async = false;
    let mut is_static = false;

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "accessibility_modifier" => {
                accessibility = child.utf8_text(source).unwrap_or("public").to_string();
            }
            "async" => is_async = true,
            "static" => is_static = true,
            _ => {}
        }
    }

    // Skip private and protected methods — not useful for agent context
    if accessibility == "private" || accessibility == "protected" {
        return None;
    }

    let name_node = node.child_by_field_name("name")?;
    let name = name_node.utf8_text(source).ok()?.to_string();

    // Skip constructor and internal helpers
    if name == "constructor" || name.starts_with('_') {
        return None;
    }

    let params = node
        .child_by_field_name("parameters")
        .and_then(|n| n.utf8_text(source).ok())
        .unwrap_or("()")
        .to_string();

    let return_type = node
        .child_by_field_name("return_type")
        .and_then(|n| n.utf8_text(source).ok())
        .map(|s| s.to_string());

    let signature = build_signature(&accessibility, is_static, is_async, &name, &params, &return_type);

    Some(MethodSignature {
        name,
        signature,
        jsdoc,
        line: node.start_position().row + 1,
        is_public: true,
        is_async,
        is_static,
    })
}

fn parse_field_method(node: Node, source: &[u8], jsdoc: Option<String>) -> Option<MethodSignature> {
    let mut accessibility = "public".to_string();
    let mut is_async = false;
    let mut is_static = false;

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "accessibility_modifier" => {
                accessibility = child.utf8_text(source).unwrap_or("public").to_string();
            }
            "static" => is_static = true,
            _ => {}
        }
    }

    if accessibility == "private" || accessibility == "protected" {
        return None;
    }

    let name_node = node.child_by_field_name("name")?;
    let name = name_node.utf8_text(source).ok()?.to_string();

    if name.starts_with('_') {
        return None;
    }

    // Check if the value is an arrow function
    let value_node = node.child_by_field_name("value")?;
    if !matches!(value_node.kind(), "arrow_function") {
        return None;
    }

    // Check for async inside arrow function
    let mut vcursor = value_node.walk();
    for vchild in value_node.children(&mut vcursor) {
        if vchild.kind() == "async" {
            is_async = true;
            break;
        }
    }

    let params = value_node
        .child_by_field_name("parameters")
        .and_then(|n| n.utf8_text(source).ok())
        .unwrap_or("()")
        .to_string();

    let return_type = value_node
        .child_by_field_name("return_type")
        .and_then(|n| n.utf8_text(source).ok())
        .map(|s| s.to_string());

    let signature = build_signature(&accessibility, is_static, is_async, &name, &params, &return_type);

    Some(MethodSignature {
        name,
        signature,
        jsdoc,
        line: node.start_position().row + 1,
        is_public: true,
        is_async,
        is_static,
    })
}

fn build_signature(
    accessibility: &str,
    is_static: bool,
    is_async: bool,
    name: &str,
    params: &str,
    return_type: &Option<String>,
) -> String {
    let mut parts = Vec::new();
    parts.push(accessibility.to_string());
    if is_static {
        parts.push("static".to_string());
    }
    if is_async {
        parts.push("async".to_string());
    }
    parts.push(name.to_string());
    let mut sig = parts.join(" ");
    sig.push_str(params);
    if let Some(rt) = return_type {
        sig.push_str(rt);
    }
    sig
}

fn clean_jsdoc(text: &str) -> String {
    // Strip /** ... */ and leading * from each line
    let stripped = text
        .trim_start_matches("/**")
        .trim_end_matches("*/")
        .trim()
        .to_string();

    stripped
        .lines()
        .map(|l| l.trim().trim_start_matches('*').trim())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

// ── Selector extraction ──────────────────────────────────────────────────────

fn collect_selectors_from_node(node: Node, source: &[u8], selectors: &mut Vec<SelectorInfo>) {
    // Look for string literals that look like selectors
    if matches!(node.kind(), "string" | "template_string") {
        if let Ok(text) = node.utf8_text(source) {
            check_selector_string(text, node.start_position().row + 1, selectors);
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if !matches!(child.kind(), "statement_block") {
            collect_selectors_from_node(child, source, selectors);
        }
    }
}

fn scan_body_for_selectors(body: Node, source: &[u8], selectors: &mut Vec<SelectorInfo>) {
    // Use the raw source text approach for selector scanning — faster than deep AST walk
    let start = body.start_byte();
    let end = body.end_byte();
    if end > source.len() {
        return;
    }
    let body_text = match std::str::from_utf8(&source[start..end]) {
        Ok(t) => t,
        Err(_) => return,
    };
    let start_line = body.start_position().row;

    for (line_idx, line) in body_text.lines().enumerate() {
        let line_num = start_line + line_idx + 1;
        extract_selectors_from_line(line, line_num, selectors);
    }
}

fn extract_selectors_from_line(line: &str, line_num: usize, selectors: &mut Vec<SelectorInfo>) {
    // data-test="value" or data-test='value'
    for cap in find_attr_values(line, "data-test=") {
        selectors.push(SelectorInfo { value: cap, kind: "data-test".into(), line: line_num });
    }
    for cap in find_attr_values(line, "data-test-id=") {
        selectors.push(SelectorInfo { value: cap, kind: "data-test-id".into(), line: line_num });
    }
    // getByRole('rolename') or getByRole("rolename")
    for cap in find_by_role(line) {
        selectors.push(SelectorInfo { value: cap, kind: "role".into(), line: line_num });
    }
}

fn find_attr_values(text: &str, attr: &str) -> Vec<String> {
    let mut results = Vec::new();
    let mut search = text;
    while let Some(pos) = search.find(attr) {
        let after = &search[pos + attr.len()..];
        if let Some(val) = extract_quoted(after) {
            results.push(val);
        }
        search = &search[pos + attr.len()..];
    }
    results
}

fn find_by_role(text: &str) -> Vec<String> {
    let mut results = Vec::new();
    let mut search = text;
    while let Some(pos) = search.find("getByRole(") {
        let after = &search[pos + "getByRole(".len()..];
        if let Some(val) = extract_quoted(after) {
            results.push(val);
        }
        search = &search[pos + 1..];
    }
    results
}

fn extract_quoted(s: &str) -> Option<String> {
    let s = s.trim_start();
    let quote_char = s.chars().next()?;
    if quote_char != '"' && quote_char != '\'' && quote_char != '`' {
        return None;
    }
    let inner = &s[1..];
    let end = inner.find(quote_char)?;
    Some(inner[..end].to_string())
}

fn check_selector_string(text: &str, line: usize, selectors: &mut Vec<SelectorInfo>) {
    let inner = text.trim_matches(|c| c == '"' || c == '\'' || c == '`');
    if inner.contains("data-test=") {
        for v in find_attr_values(inner, "data-test=") {
            selectors.push(SelectorInfo { value: v, kind: "data-test".into(), line });
        }
    }
}

// ── Top-level export extraction (non-class) ──────────────────────────────────

fn extract_top_level_exports(
    root: Node,
    source: &[u8],
    file_path: &Path,
    relative_path: &str,
) -> Vec<SymbolExport> {
    let mut exports = Vec::new();
    let mut cursor = root.walk();

    for child in root.children(&mut cursor) {
        match child.kind() {
            "export_statement" => {
                let mut ecursor = child.walk();
                for echild in child.children(&mut ecursor) {
                    match echild.kind() {
                        "function_declaration" | "lexical_declaration" | "variable_declaration" => {
                            if let Some(name) = get_declaration_name(echild, source) {
                                exports.push(SymbolExport {
                                    name,
                                    file_path: file_path.to_path_buf(),
                                    relative_path: relative_path.to_string(),
                                });
                            }
                        }
                        "type_alias_declaration" | "interface_declaration" | "enum_declaration" => {
                            if let Some(n) = echild.child_by_field_name("name") {
                                if let Ok(name) = n.utf8_text(source) {
                                    exports.push(SymbolExport {
                                        name: name.to_string(),
                                        file_path: file_path.to_path_buf(),
                                        relative_path: relative_path.to_string(),
                                    });
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    exports
}

fn get_declaration_name(node: Node, source: &[u8]) -> Option<String> {
    // For function declarations: field "name"
    if let Some(n) = node.child_by_field_name("name") {
        return n.utf8_text(source).ok().map(|s| s.to_string());
    }
    // For lexical/variable declarations: look for variable_declarator -> name
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "variable_declarator" {
            if let Some(n) = child.child_by_field_name("name") {
                return n.utf8_text(source).ok().map(|s| s.to_string());
            }
        }
    }
    None
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_indexer() -> Indexer {
        Indexer::new(PathBuf::from("/fake/playwright"))
    }

    // ── Class extraction ─────────────────────────────────────────────────────

    #[test]
    fn extracts_class_name() {
        let src = "export class FooPage {}";
        let idx = make_indexer().parse_source(src, "page-objects/foo-page");
        assert!(idx.classes.contains_key("foopage"), "class key missing");
        assert_eq!(idx.classes["foopage"].name, "FooPage");
    }

    #[test]
    fn extracts_extends_clause() {
        let src = "export class FooPage extends BasePage {}";
        let idx = make_indexer().parse_source(src, "page-objects/foo-page");
        let cls = &idx.classes["foopage"];
        assert_eq!(cls.extends.as_deref(), Some("BasePage"));
    }

    #[test]
    fn extracts_abstract_class() {
        let src = "export abstract class AbstractBase {}";
        let idx = make_indexer().parse_source(src, "step-drivers/abstract-base");
        assert!(idx.classes.contains_key("abstractbase"));
    }

    // ── Method extraction ────────────────────────────────────────────────────

    #[test]
    fn extracts_public_async_method() {
        let src = r#"
export class MyPage {
  public async clickButton(): Promise<void> {}
}
"#;
        let idx = make_indexer().parse_source(src, "page-objects/my-page");
        let methods = &idx.classes["mypage"].methods;
        assert_eq!(methods.len(), 1);
        assert_eq!(methods[0].name, "clickButton");
        assert!(methods[0].is_async);
        assert!(methods[0].signature.contains("async"));
        assert!(methods[0].signature.contains("clickButton"));
        assert!(methods[0].signature.contains(": Promise<void>"));
    }

    #[test]
    fn skips_private_methods() {
        let src = r#"
export class MyPage {
  private helper(): void {}
  public visibleMethod(): void {}
}
"#;
        let idx = make_indexer().parse_source(src, "page-objects/my-page");
        let methods = &idx.classes["mypage"].methods;
        assert_eq!(methods.len(), 1, "should only expose public method");
        assert_eq!(methods[0].name, "visibleMethod");
    }

    #[test]
    fn skips_protected_methods() {
        let src = r#"
export class MyPage {
  protected internalHelper(): string { return ""; }
  public publicMethod(): void {}
}
"#;
        let idx = make_indexer().parse_source(src, "page-objects/my-page");
        let methods = &idx.classes["mypage"].methods;
        assert_eq!(methods.len(), 1);
        assert_eq!(methods[0].name, "publicMethod");
    }

    #[test]
    fn skips_constructor() {
        let src = r#"
export class MyPage {
  constructor(private page: any) {}
  public doThing(): void {}
}
"#;
        let idx = make_indexer().parse_source(src, "page-objects/my-page");
        let methods = &idx.classes["mypage"].methods;
        assert!(
            methods.iter().all(|m| m.name != "constructor"),
            "constructor should be filtered out"
        );
    }

    #[test]
    fn extracts_method_parameters() {
        let src = r#"
export class MyPage {
  public async createVm(name: string, namespace: string): Promise<void> {}
}
"#;
        let idx = make_indexer().parse_source(src, "page-objects/my-page");
        let methods = &idx.classes["mypage"].methods;
        assert_eq!(methods.len(), 1);
        assert!(
            methods[0].signature.contains("name: string"),
            "signature should contain param types: {}",
            methods[0].signature
        );
    }

    #[test]
    fn extracts_static_method() {
        let src = r#"
export class MyPage {
  public static create(): MyPage { return new MyPage(); }
}
"#;
        let idx = make_indexer().parse_source(src, "page-objects/my-page");
        let methods = &idx.classes["mypage"].methods;
        assert_eq!(methods.len(), 1);
        assert!(methods[0].is_static);
        assert!(methods[0].signature.contains("static"));
    }

    #[test]
    fn extracts_jsdoc_comment() {
        let src = r#"
export class MyPage {
  /**
   * Navigates to the VM list page.
   */
  public async navigate(): Promise<void> {}
}
"#;
        let idx = make_indexer().parse_source(src, "page-objects/my-page");
        let methods = &idx.classes["mypage"].methods;
        assert_eq!(methods.len(), 1);
        let doc = methods[0].jsdoc.as_deref().unwrap_or("");
        assert!(doc.contains("Navigates"), "jsdoc missing: {:?}", doc);
    }

    #[test]
    fn extracts_arrow_function_property() {
        let src = r#"
export class MyPage {
  public clickCreate = async (): Promise<void> => {};
}
"#;
        let idx = make_indexer().parse_source(src, "page-objects/my-page");
        let methods = &idx.classes["mypage"].methods;
        assert!(
            methods.iter().any(|m| m.name == "clickCreate"),
            "arrow property method not extracted"
        );
    }

    #[test]
    fn extracts_method_line_number() {
        let src = "export class P {\n  public doThing(): void {}\n}";
        let idx = make_indexer().parse_source(src, "page-objects/p");
        let methods = &idx.classes["p"].methods;
        assert_eq!(methods[0].line, 2, "line number should be 1-indexed");
    }

    // ── Selector extraction ──────────────────────────────────────────────────

    #[test]
    fn extracts_data_test_selectors() {
        let src = r#"
export class MyPage {
  get createBtn() {
    return this.page.locator('[data-test="create-vm-btn"]');
  }
}
"#;
        let idx = make_indexer().parse_source(src, "page-objects/my-page");
        let cls = &idx.classes["mypage"];
        assert!(
            cls.selectors.iter().any(|s| s.value == "create-vm-btn"),
            "data-test selector not found: {:?}",
            cls.selectors
        );
    }

    #[test]
    fn extracts_data_test_id_selectors() {
        let src = r#"
export class MyPage {
  get btn() { return this.page.locator('[data-test-id="submit-button"]'); }
}
"#;
        let idx = make_indexer().parse_source(src, "page-objects/my-page");
        let cls = &idx.classes["mypage"];
        assert!(
            cls.selectors.iter().any(|s| s.value == "submit-button" && s.kind == "data-test-id"),
            "data-test-id selector not found"
        );
    }

    // ── find_class partial matching ──────────────────────────────────────────

    #[test]
    fn find_class_exact_match() {
        let src = "export class VirtualMachinesPage {}";
        let idx = make_indexer().parse_source(src, "page-objects/virtual-machines-page");
        let results = idx.find_class("VirtualMachinesPage");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "VirtualMachinesPage");
    }

    #[test]
    fn find_class_partial_match() {
        let src = "export class VirtualMachinesStepDriver {}";
        let idx = make_indexer().parse_source(src, "step-drivers/virtual-machines-step-driver");
        let results = idx.find_class("VirtualMachines");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn find_class_case_insensitive() {
        let src = "export class CatalogPage {}";
        let idx = make_indexer().parse_source(src, "page-objects/catalog-page");
        let results = idx.find_class("catalog");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn find_class_no_match_returns_empty() {
        let src = "export class FooPage {}";
        let idx = make_indexer().parse_source(src, "page-objects/foo-page");
        let results = idx.find_class("BarPage");
        assert!(results.is_empty());
    }

    // ── Top-level export extraction ──────────────────────────────────────────

    #[test]
    fn extracts_exported_const() {
        let src = "export const myHelper = () => {};";
        let idx = make_indexer().parse_source(src, "utils/my-helper");
        assert!(
            idx.exports.contains_key("myhelper"),
            "exported const not indexed"
        );
    }

    #[test]
    fn extracts_exported_function() {
        let src = "export function createVm(name: string): void {}";
        let idx = make_indexer().parse_source(src, "utils/create-vm");
        assert!(
            idx.exports.contains_key("createvm"),
            "exported function not indexed"
        );
    }

    // ── Multiple classes per file ────────────────────────────────────────────

    #[test]
    fn indexes_multiple_classes_in_same_file() {
        let src = r#"
export class PageA {}
export class PageB {}
"#;
        let idx = make_indexer().parse_source(src, "page-objects/multi");
        assert!(idx.classes.contains_key("pagea"));
        assert!(idx.classes.contains_key("pageb"));
    }
}
