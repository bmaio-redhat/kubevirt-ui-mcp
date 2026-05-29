use std::sync::Arc;

use serde_json::{json, Value};

use crate::config::Config;
use crate::docs::cache::SharedDocsCache;
use crate::docs::fetcher;
use crate::docs::index::{self, SECTIONS};
use crate::mcp::protocol::ToolCallResult;

pub fn all_tools() -> Vec<Value> {
    vec![
        json!({
            "name": "list_product_doc_sections",
            "description": "List available OpenShift Virtualization documentation sections from the openshift-docs GitHub repository. Shows curated index with cache status and tags.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "filter": {
                        "type": "string",
                        "description": "Optional keyword filter on title, id, or tags (e.g. 'networking', 'storage', 'migration')"
                    }
                }
            }
        }),
        json!({
            "name": "fetch_product_doc",
            "description": "Fetch an OpenShift Virtualization documentation page from the openshift-docs GitHub repository (openshift/openshift-docs main branch, virt/ directory). Converts AsciiDoc to compact markdown with include resolution and attribute substitution. Results are cached locally for 7 days.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "section": {
                        "type": "string",
                        "description": "Section ID from the curated index (e.g. 'about', 'networking-overview', 'storage-overview'). Use list_product_doc_sections to see available IDs."
                    },
                    "path": {
                        "type": "string",
                        "description": "Specific repo path for files not in the curated index (e.g. 'virt/storage/virt-configuring-local-storage-with-hpp.adoc')"
                    },
                    "force_refresh": {
                        "type": "boolean",
                        "description": "Force re-fetch even if cached. Defaults to false."
                    }
                }
            }
        }),
        json!({
            "name": "search_product_docs",
            "description": "Full-text search across all cached OpenShift Virtualization documentation pages. Returns matching pages with surrounding context lines. Fetch pages first using fetch_product_doc if the cache is empty.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search terms (space-separated, all must match)"
                    }
                },
                "required": ["query"]
            }
        }),
    ]
}

pub async fn dispatch(
    name: &str,
    params: &Value,
    cache: &SharedDocsCache,
    client: &Arc<reqwest::Client>,
    cfg: &Config,
) -> ToolCallResult {
    match name {
        "list_product_doc_sections" => handle_list(params, cache).await,
        "fetch_product_doc" => handle_fetch(params, cache, client, cfg).await,
        "search_product_docs" => handle_search(params, cache).await,
        _ => ToolCallResult::error(format!("Unknown docs tool: '{}'", name)),
    }
}

async fn handle_list(params: &Value, cache: &SharedDocsCache) -> ToolCallResult {
    let filter = params.get("filter").and_then(|v| v.as_str()).unwrap_or("");

    let sections: Vec<&index::DocSection> = if filter.is_empty() {
        SECTIONS.iter().collect()
    } else {
        index::search_sections(filter)
    };

    let guard = cache.read().await;

    let mut out = String::new();
    out.push_str(&format!("# OpenShift Virtualization Documentation Index\n"));
    out.push_str(&format!("Source: openshift/openshift-docs (main branch)\n"));
    out.push_str(&format!("Sections: {} (showing {})\n\n", SECTIONS.len(), sections.len()));

    let mut current_dir = "";
    for s in &sections {
        let dir_label = s.dir.trim_start_matches("virt/");
        if dir_label != current_dir {
            current_dir = dir_label;
            out.push_str(&format!("\n## {}\n\n", dir_label));
        }

        let cached = guard.pages.get(s.id);
        let status = match cached {
            Some(page) => {
                if page.is_stale() {
                    "stale"
                } else {
                    "cached"
                }
            }
            None => "not cached",
        };

        out.push_str(&format!(
            "- **{}** (`{}`) — [{}] tags: {}\n",
            s.title,
            s.id,
            status,
            s.tags.join(", ")
        ));
    }

    out.push_str(&format!(
        "\n---\nTotal cached pages: {}\n",
        guard.pages.len()
    ));
    out.push_str("Use `fetch_product_doc` with a section ID to fetch and cache a page.\n");

    ToolCallResult::text(out)
}

async fn handle_fetch(
    params: &Value,
    cache: &SharedDocsCache,
    client: &Arc<reqwest::Client>,
    cfg: &Config,
) -> ToolCallResult {
    let section = params.get("section").and_then(|v| v.as_str());
    let path = params.get("path").and_then(|v| v.as_str());
    let force = params.get("force_refresh").and_then(|v| v.as_bool()).unwrap_or(false);

    if section.is_none() && path.is_none() {
        return ToolCallResult::error(
            "At least one of 'section' or 'path' is required. Use list_product_doc_sections to see available section IDs."
        );
    }

    if let Some(section_id) = section {
        match index::find_section(section_id) {
            Some(s) => {
                match fetcher::fetch_and_process(
                    s.dir, s.file, s.id, client, cache, &cfg.docs_cache_path, force,
                ).await {
                    Ok(page) => {
                        let mut out = String::new();
                        out.push_str(&format!("# {}\n", page.title));
                        out.push_str(&format!("Source: `{}`\n\n", page.repo_path));
                        out.push_str(&page.content);
                        ToolCallResult::text(out)
                    }
                    Err(e) => ToolCallResult::error(format!("Failed to fetch section '{}': {}", section_id, e)),
                }
            }
            None => ToolCallResult::error(format!(
                "Unknown section ID '{}'. Use list_product_doc_sections to see available IDs.",
                section_id
            )),
        }
    } else if let Some(repo_path) = path {
        match fetcher::fetch_arbitrary_path(repo_path, client, cache, &cfg.docs_cache_path, force).await {
            Ok(page) => {
                let mut out = String::new();
                out.push_str(&format!("# {}\n", page.title));
                out.push_str(&format!("Source: `{}`\n\n", page.repo_path));
                out.push_str(&page.content);
                ToolCallResult::text(out)
            }
            Err(e) => ToolCallResult::error(format!("Failed to fetch '{}': {}", repo_path, e)),
        }
    } else {
        ToolCallResult::error("Unexpected state: no section or path")
    }
}

async fn handle_search(params: &Value, cache: &SharedDocsCache) -> ToolCallResult {
    let query = match params.get("query").and_then(|v| v.as_str()) {
        Some(q) => q,
        None => return ToolCallResult::error("Missing required parameter: query"),
    };

    let guard = cache.read().await;

    if guard.pages.is_empty() {
        return ToolCallResult::error(
            "No documentation pages cached yet. Use fetch_product_doc to cache pages first, then search."
        );
    }

    let hits = guard.search(query);

    if hits.is_empty() {
        return ToolCallResult::text(format!(
            "No matches for '{}' across {} cached pages.\n\nTip: fetch more sections with fetch_product_doc to expand the searchable content.",
            query,
            guard.pages.len()
        ));
    }

    let mut out = String::new();
    out.push_str(&format!(
        "# Search: '{}'\nMatches in {} of {} cached pages\n\n",
        query,
        hits.len(),
        guard.pages.len()
    ));

    for hit in &hits {
        out.push_str(&format!("## {} (`{}`)\n\n", hit.title, hit.section_id));
        for snippet in &hit.snippets {
            out.push_str(&format!("```\n{}\n```\n\n", snippet));
        }
    }

    ToolCallResult::text(out)
}
