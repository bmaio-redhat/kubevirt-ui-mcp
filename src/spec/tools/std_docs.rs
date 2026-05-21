use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Resolved STD document for a feature area.
#[derive(Debug, Clone)]
pub struct StdDoc {
    #[allow(dead_code)]
    pub path: PathBuf,
    /// Path relative to docs_root
    pub rel_path: String,
    /// Raw markdown content
    pub content: String,
}

/// Find the STD doc(s) most relevant to a spec file relative path.
///
/// Strategy:
///   1. Mirror the path: `tier1/checkups/checkups.spec.ts` → look for
///      `docs_root/tier1/checkups.md` or `docs_root/tier1/checkups/*.md`
///   2. Fall back by stripping sub-directories up to the feature level.
///
/// Returns all matched docs (there may be more than one if a feature has sub-docs).
pub fn find_std_for_spec(docs_root: &str, spec_rel_path: &str) -> Vec<StdDoc> {
    let parts: Vec<&str> = spec_rel_path.split('/').collect();
    // parts: ["tier1", "checkups", "checkups.spec.ts"] or
    //        ["tier1", "virtualmachines", "vm-actions", "vm-lifecycle-actions.spec.ts"]

    let mut candidates: Vec<PathBuf> = Vec::new();

    // Try progressively shorter paths until something matches
    // e.g. tier1/virtualmachines/vm-actions -> tier1/virtualmachines -> tier1
    for depth in (1..parts.len()).rev() {
        let dir_path = parts[..depth].join("/");
        let dir_full = Path::new(docs_root).join(&dir_path);

        // Check for exact feature-named .md file at this level
        // e.g. docs/tier1/checkups.md for spec tier1/checkups/...
        if depth < parts.len() {
            let feature_name = parts[depth]; // the next component after the dir
            let feature_stem = feature_name.trim_end_matches(".spec.ts");
            // Normalise: vm-lifecycle-actions -> vm-actions (strip last -word if needed)
            // Try exact first, then parent feature names
            let candidates_at_level = feature_md_candidates(feature_stem);
            for candidate in &candidates_at_level {
                let md_path = dir_full.join(format!("{}.md", candidate));
                if md_path.exists() {
                    candidates.push(md_path);
                }
            }
        }

        // Scan the directory only when it directly mirrors the spec's own subdirectory
        // (i.e. the spec lives in docs_root/<dir_path>/ and we're looking for docs there).
        // Guard: the directory name must exactly match the spec's immediate parent folder.
        let spec_parent = parts.get(parts.len().saturating_sub(2)).copied().unwrap_or("");
        let dir_name = dir_full.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if dir_name == spec_parent && dir_full.is_dir() && candidates.is_empty() {
            for entry in WalkDir::new(&dir_full)
                .max_depth(1)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                let p = entry.path();
                if p.is_file() && p.extension().map(|e| e == "md").unwrap_or(false) {
                    if p.file_name().map(|n| n != "README.md").unwrap_or(false) {
                        candidates.push(p.to_path_buf());
                    }
                }
            }
        }

        if !candidates.is_empty() {
            break;
        }
    }

    candidates
        .into_iter()
        .filter_map(|path| {
            let content = std::fs::read_to_string(&path).ok()?;
            let rel_path = path
                .strip_prefix(docs_root)
                .ok()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| path.to_string_lossy().to_string());
            Some(StdDoc { path, rel_path, content })
        })
        .collect()
}

/// Generate candidate .md base names from a spec file stem.
/// `vm-lifecycle-actions` → ["vm-lifecycle-actions", "vm-actions", "vm"]
/// `checkups` → ["checkups"]
fn feature_md_candidates(stem: &str) -> Vec<String> {
    let mut candidates = vec![stem.to_string()];
    // Walk back by removing the last hyphen-segment
    let mut s = stem.to_string();
    while let Some(pos) = s.rfind('-') {
        s.truncate(pos);
        candidates.push(s.clone());
    }
    candidates
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn make_tmp_docs(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tmpdir");
        for (rel, content) in files {
            let full: PathBuf = dir.path().join(rel);
            fs::create_dir_all(full.parent().unwrap()).unwrap();
            fs::write(&full, content).unwrap();
        }
        dir
    }

    #[test]
    fn finds_exact_feature_doc() {
        let dir = make_tmp_docs(&[("tier1/checkups.md", "# Checkups STD")]);
        let docs_root = dir.path().to_str().unwrap();
        let results = find_std_for_spec(docs_root, "tier1/checkups/checkups.spec.ts");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].rel_path, "tier1/checkups.md");
        assert_eq!(results[0].content.trim(), "# Checkups STD");
    }

    #[test]
    fn falls_back_to_parent_feature_name() {
        // vm-lifecycle-actions.spec.ts should match vm-actions.md
        let dir = make_tmp_docs(&[("tier1/virtualmachines/vm-actions.md", "# VM Actions")]);
        let docs_root = dir.path().to_str().unwrap();
        let results = find_std_for_spec(
            docs_root,
            "tier1/virtualmachines/vm-actions/vm-lifecycle-actions.spec.ts",
        );
        assert_eq!(results.len(), 1);
        assert!(results[0].rel_path.contains("vm-actions.md"));
    }

    #[test]
    fn returns_empty_when_no_match() {
        let dir = make_tmp_docs(&[("tier1/other.md", "# Other")]);
        let docs_root = dir.path().to_str().unwrap();
        let results = find_std_for_spec(docs_root, "tier1/checkups/checkups.spec.ts");
        assert!(results.is_empty());
    }

    #[test]
    fn find_all_excludes_readme_and_template() {
        let dir = make_tmp_docs(&[
            ("tier1/checkups.md", "# Checkups"),
            ("tier1/README.md", "# README"),
            ("STD-TEMPLATE.md", "# Template"),
            ("tier2/networking.md", "# Networking"),
        ]);
        let docs_root = dir.path().to_str().unwrap();
        let docs = find_all_std_docs(docs_root, None);
        let names: Vec<&str> = docs.iter().map(|d| d.rel_path.as_str()).collect();
        assert!(names.contains(&"tier1/checkups.md"));
        assert!(names.contains(&"tier2/networking.md"));
        assert!(!names.contains(&"tier1/README.md"));
        assert!(!names.contains(&"STD-TEMPLATE.md"));
    }

    #[test]
    fn find_all_with_filter() {
        let dir = make_tmp_docs(&[
            ("tier1/checkups.md", "# Checkups"),
            ("tier2/networking.md", "# Networking"),
        ]);
        let docs_root = dir.path().to_str().unwrap();
        let docs = find_all_std_docs(docs_root, Some("tier1"));
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].rel_path, "tier1/checkups.md");
    }
}

/// Find all STD docs under a docs root, optionally filtered by tier/feature prefix.
pub fn find_all_std_docs(docs_root: &str, prefix_filter: Option<&str>) -> Vec<StdDoc> {
    let root_path = Path::new(docs_root);
    let mut docs = Vec::new();

    for entry in WalkDir::new(docs_root).follow_links(true).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if path.extension().map(|e| e != "md").unwrap_or(true) {
            continue;
        }
        if path.file_name().map(|n| n == "README.md" || n == "STD-TEMPLATE.md").unwrap_or(false) {
            continue;
        }

        let rel = path
            .strip_prefix(root_path)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();

        if let Some(filter) = prefix_filter {
            if !rel.to_lowercase().contains(&filter.to_lowercase()) {
                continue;
            }
        }

        if let Ok(content) = std::fs::read_to_string(path) {
            docs.push(StdDoc { path: path.to_path_buf(), rel_path: rel, content });
        }
    }

    docs.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    docs
}
