use serde_json::Value;
use std::fs;

use crate::mcp::protocol::ToolCallResult;
use crate::spec::tools::{
    markdown::{search_results_to_markdown, spec_metadata_only, std_with_spec_metadata},
    parser::{find_spec_files, parse_spec, Suite, TestCase, SpecFile},
    std_docs::{find_all_std_docs, find_std_for_spec},
};

fn resolve_root(params: &Value) -> Result<String, ToolCallResult> {
    if let Some(r) = params.get("root").and_then(|v| v.as_str()) {
        return Ok(r.to_string());
    }
    std::env::var("PLAYWRIGHT_TESTS_ROOT").map_err(|_| {
        ToolCallResult::error(
            "Missing `root` parameter and PLAYWRIGHT_TESTS_ROOT env var is not set",
        )
    })
}

fn resolve_docs_root(params: &Value) -> Option<String> {
    if let Some(r) = params.get("docs_root").and_then(|v| v.as_str()) {
        return Some(r.to_string());
    }
    std::env::var("PLAYWRIGHT_DOCS_ROOT").ok()
}

// ── list_spec_files ───────────────────────────────────────────────────────────

pub fn handle_list_spec_files(params: &Value) -> ToolCallResult {
    let root = match resolve_root(params) {
        Ok(r) => r,
        Err(e) => return e,
    };
    let docs_root = resolve_docs_root(params);

    let files = find_spec_files(&root);
    if files.is_empty() {
        return ToolCallResult::text(format!("No spec files found under `{}`.", root));
    }

    let mut out = format!("# Spec files under `{}`\n\n", root);
    if let Some(ref dr) = docs_root {
        out.push_str(&format!("_STD docs root: `{}`_\n\n", dr));
    }
    out.push_str(&format!("Found **{}** spec file(s):\n\n", files.len()));

    let mut by_tier: std::collections::BTreeMap<String, Vec<String>> = Default::default();
    for f in &files {
        let tier = f.split('/').next().unwrap_or("other").to_string();
        by_tier.entry(tier).or_default().push(f.clone());
    }

    for (tier, paths) in &by_tier {
        out.push_str(&format!("## {}\n\n", tier));
        for p in paths {
            let has_std = docs_root
                .as_deref()
                .map(|dr| !find_std_for_spec(dr, p).is_empty())
                .unwrap_or(false);
            let std_badge = if has_std { " ✅ STD" } else { " ⚠ no STD" };
            out.push_str(&format!("- `{}`{}\n", p, std_badge));
        }
        out.push('\n');
    }

    ToolCallResult::text(out)
}

// ── get_spec_markdown ─────────────────────────────────────────────────────────

pub fn handle_get_spec_markdown(params: &Value) -> ToolCallResult {
    let path = match params.get("path").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => return ToolCallResult::error("Missing required parameter: path"),
    };
    let docs_root = resolve_docs_root(params);

    let source = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => return ToolCallResult::error(format!("Cannot read `{}`: {}", path, e)),
    };

    let rel = std::path::Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string());

    let spec = parse_spec(&source, &rel);

    // Try STD doc
    let md = if let Some(ref dr) = docs_root {
        let stds = find_std_for_spec(dr, &rel);
        if let Some(std_doc) = stds.first() {
            format!(
                "# `{}`\n\n_STD: [`{}`]_\n\n{}\n",
                path,
                std_doc.rel_path,
                std_with_spec_metadata(std_doc, Some(&spec))
            )
        } else {
            format!("# `{}`\n\n{}", path, spec_metadata_only(&spec))
        }
    } else {
        format!("# `{}`\n\n{}", path, spec_metadata_only(&spec))
    };

    ToolCallResult::text(md)
}

// ── get_all_specs_markdown ────────────────────────────────────────────────────

pub fn handle_get_all_specs_markdown(params: &Value) -> ToolCallResult {
    let root = match resolve_root(params) {
        Ok(r) => r,
        Err(e) => return e,
    };
    let docs_root = resolve_docs_root(params);
    let tier_filter = params.get("tier").and_then(|v| v.as_str()).map(str::to_lowercase);
    let feature_filter = params.get("feature").and_then(|v| v.as_str()).map(str::to_lowercase);

    // If both a docs_root is set and no specific spec was asked for, prefer STD docs directly
    if let Some(ref dr) = docs_root {
        let prefix = match (&tier_filter, &feature_filter) {
            (Some(t), Some(f)) => Some(format!("{}/{}", t, f)),
            (Some(t), None) => Some(t.clone()),
            (None, Some(f)) => Some(f.clone()),
            (None, None) => None,
        };

        let stds = find_all_std_docs(dr, prefix.as_deref());
        if !stds.is_empty() {
            let title = match (&tier_filter, &feature_filter) {
                (Some(t), Some(f)) => format!("Playwright Tests — {} / {}", t, f),
                (Some(t), None) => format!("Playwright Tests — {}", t),
                (None, Some(f)) => format!("Playwright Tests — {}", f),
                (None, None) => "Playwright Tests — All STDs".to_string(),
            };

            let mut out = format!("# {}\n\n", title);
            out.push_str(&format!("_Generated from {} STD document(s)_\n\n", stds.len()));

            // TOC
            out.push_str("## Table of Contents\n\n");
            for std_doc in &stds {
                let anchor =
                    std_doc.rel_path.replace('/', "-").replace('.', "-").to_lowercase();
                out.push_str(&format!("- [`{}`](#{anchor})\n", std_doc.rel_path));
            }
            out.push_str("\n---\n\n");

            for std_doc in &stds {
                // Find corresponding spec for metadata augmentation
                let spec = find_spec_for_std(&root, dr, &std_doc.rel_path);
                out.push_str(&format!("## `{}`\n\n", std_doc.rel_path));
                out.push_str(&std_with_spec_metadata(std_doc, spec.as_ref()));
                out.push_str("\n---\n\n");
            }

            return ToolCallResult::text(out);
        }
    }

    // Fallback: spec files only
    let all_files = find_spec_files(&root);
    let filtered: Vec<&str> = all_files.iter().map(|s| s.as_str()).filter(|rel| {
        if let Some(ref t) = tier_filter {
            if !rel.split('/').next().unwrap_or("").to_lowercase().contains(t.as_str()) {
                return false;
            }
        }
        if let Some(ref f) = feature_filter {
            let parts: Vec<&str> = rel.split('/').collect();
            if !parts.get(1).copied().unwrap_or("").to_lowercase().contains(f.as_str()) {
                return false;
            }
        }
        true
    }).collect();

    if filtered.is_empty() {
        return ToolCallResult::text(format!(
            "No spec files found under `{}` matching the given filters.",
            root
        ));
    }

    let mut out = String::from("# Playwright Tests — Spec Metadata\n\n");
    out.push_str("> _No STD docs root configured. Set PLAYWRIGHT_DOCS_ROOT for richer output._\n\n");
    for rel in filtered {
        let full = format!("{}/{}", root.trim_end_matches('/'), rel);
        if let Ok(source) = fs::read_to_string(&full) {
            let spec = parse_spec(&source, rel);
            out.push_str(&spec_metadata_only(&spec));
            out.push_str("---\n\n");
        }
    }

    ToolCallResult::text(out)
}

// ── get_std_doc ───────────────────────────────────────────────────────────────

pub fn handle_get_std_doc(params: &Value) -> ToolCallResult {
    let docs_root = match resolve_docs_root(params) {
        Some(r) => r,
        None => return ToolCallResult::error(
            "Missing docs_root parameter and PLAYWRIGHT_DOCS_ROOT env var is not set",
        ),
    };
    let tests_root = resolve_root(params).ok();

    let rel = match params.get("doc").and_then(|v| v.as_str()) {
        Some(d) => d,
        None => return ToolCallResult::error("Missing required parameter: doc"),
    };

    let full_path = format!("{}/{}", docs_root.trim_end_matches('/'), rel);
    let content = match fs::read_to_string(&full_path) {
        Ok(c) => c,
        Err(e) => return ToolCallResult::error(format!("Cannot read `{}`: {}", full_path, e)),
    };

    use crate::spec::tools::std_docs::StdDoc;
    let std_doc = StdDoc {
        path: std::path::PathBuf::from(&full_path),
        rel_path: rel.to_string(),
        content,
    };

    // Try to pair with spec metadata
    let spec = tests_root
        .as_deref()
        .and_then(|tr| find_spec_for_std(tr, &docs_root, rel));

    ToolCallResult::text(std_with_spec_metadata(&std_doc, spec.as_ref()))
}

// ── search_tests ──────────────────────────────────────────────────────────────

pub fn handle_search_tests(params: &Value) -> ToolCallResult {
    let root = match resolve_root(params) {
        Ok(r) => r,
        Err(e) => return e,
    };
    let docs_root = resolve_docs_root(params);
    let query = match params.get("query").and_then(|v| v.as_str()) {
        Some(q) => q.to_lowercase(),
        None => return ToolCallResult::error("Missing required parameter: query"),
    };

    let all_files = find_spec_files(&root);
    let mut specs: Vec<SpecFile> = Vec::new();
    for rel in &all_files {
        let full = format!("{}/{}", root.trim_end_matches('/'), rel);
        if let Ok(source) = fs::read_to_string(&full) {
            specs.push(parse_spec(&source, rel));
        }
    }

    struct Flat<'a> {
        spec_idx: usize,
        suite: &'a Suite,
    }
    let mut flat: Vec<Flat> = Vec::new();
    fn collect<'a>(idx: usize, suites: &'a [Suite], out: &mut Vec<Flat<'a>>) {
        for s in suites {
            out.push(Flat { spec_idx: idx, suite: s });
            collect(idx, &s.nested, out);
        }
    }
    for (i, spec) in specs.iter().enumerate() {
        collect(i, &spec.suites, &mut flat);
    }

    let mut matched: Vec<(&SpecFile, &Suite, &TestCase)> = Vec::new();
    for item in &flat {
        let spec = &specs[item.spec_idx];
        for tc in &item.suite.tests {
            let haystack = format!(
                "{} {} {} {} {}",
                tc.name.to_lowercase(),
                tc.jira_id.as_deref().unwrap_or("").to_lowercase(),
                tc.tags.join(" ").to_lowercase(),
                item.suite.name.to_lowercase(),
                spec.rel_path.to_lowercase()
            );
            if haystack.contains(&query) {
                matched.push((spec, item.suite, tc));
            }
        }
    }

    ToolCallResult::text(search_results_to_markdown(&query, &matched, docs_root.as_deref()))
}

// ── list_std_docs ─────────────────────────────────────────────────────────────

pub fn handle_list_std_docs(params: &Value) -> ToolCallResult {
    let docs_root = match resolve_docs_root(params) {
        Some(r) => r,
        None => return ToolCallResult::error(
            "Missing docs_root parameter and PLAYWRIGHT_DOCS_ROOT env var is not set",
        ),
    };
    let filter = params.get("filter").and_then(|v| v.as_str());

    let docs = find_all_std_docs(&docs_root, filter);
    if docs.is_empty() {
        return ToolCallResult::text(format!(
            "No STD docs found under `{}`{}.",
            docs_root,
            filter.map(|f| format!(" matching `{}`", f)).unwrap_or_default()
        ));
    }

    let mut out = format!("# STD Documents under `{}`\n\n", docs_root);
    out.push_str(&format!("Found **{}** document(s):\n\n", docs.len()));

    let mut by_tier: std::collections::BTreeMap<String, Vec<&str>> = Default::default();
    for doc in &docs {
        let tier = doc.rel_path.split('/').next().unwrap_or("other");
        by_tier.entry(tier.to_string()).or_default().push(&doc.rel_path);
    }
    for (tier, paths) in &by_tier {
        out.push_str(&format!("## {}\n\n", tier));
        for p in paths {
            out.push_str(&format!("- `{}`\n", p));
        }
        out.push('\n');
    }

    ToolCallResult::text(out)
}

// ── helpers ───────────────────────────────────────────────────────────────────

/// Given a STD doc relative path (e.g. `tier1/checkups.md`), find the best-matching spec file.
fn find_spec_for_std(tests_root: &str, _docs_root: &str, std_rel: &str) -> Option<SpecFile> {
    // Strip .md, use the path as a prefix to match spec files
    let stem = std_rel.trim_end_matches(".md");
    let all = find_spec_files(tests_root);

    // Find spec files whose path contains the std stem components
    let stem_parts: Vec<&str> = stem.split('/').collect();
    let candidates: Vec<&str> = all
        .iter()
        .map(|s| s.as_str())
        .filter(|rel| {
            // All parts of the stem must appear in the spec path
            stem_parts.iter().all(|p| rel.contains(p))
        })
        .collect();

    // Pick the best match (fewest extra path components)
    let best = candidates.into_iter().min_by_key(|rel| rel.split('/').count())?;
    let full = format!("{}/{}", tests_root.trim_end_matches('/'), best);
    let source = fs::read_to_string(&full).ok()?;
    Some(parse_spec(&source, best))
}
