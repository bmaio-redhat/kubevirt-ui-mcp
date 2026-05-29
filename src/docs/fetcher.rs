use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use regex::Regex;
use tracing::{debug, warn};

use crate::docs::asciidoc;
use crate::docs::cache::{DocPage, SharedDocsCache};

const RAW_BASE: &str = "https://raw.githubusercontent.com/openshift/openshift-docs/main";
const ATTRS_PATH: &str = "_attributes/common-attributes.adoc";

pub async fn ensure_attributes(
    cache: &SharedDocsCache,
    client: &Arc<reqwest::Client>,
    cache_path: &std::path::Path,
) -> HashMap<String, String> {
    {
        let guard = cache.read().await;
        if !guard.attributes_stale() && !guard.attributes.is_empty() {
            let mut merged = asciidoc::build_default_attributes();
            merged.extend(guard.attributes.clone());
            return merged;
        }
    }

    let url = format!("{}/{}", RAW_BASE, ATTRS_PATH);
    debug!("Fetching attributes from {}", url);

    let attrs = match client.get(&url).send().await {
        Ok(resp) => match resp.text().await {
            Ok(body) => asciidoc::parse_attributes(&body),
            Err(e) => {
                warn!("Failed to read attributes body: {}", e);
                let guard = cache.read().await;
                let mut merged = asciidoc::build_default_attributes();
                merged.extend(guard.attributes.clone());
                return merged;
            }
        },
        Err(e) => {
            warn!("Failed to fetch attributes: {}", e);
            let guard = cache.read().await;
            let mut merged = asciidoc::build_default_attributes();
            merged.extend(guard.attributes.clone());
            return merged;
        }
    };

    {
        let mut guard = cache.write().await;
        guard.attributes = attrs.clone();
        guard.attributes_fetched_at = Some(Utc::now());
        guard.save(cache_path);
    }

    attrs
}

/// Fetch an assembly .adoc file, resolve its include:: directives, and convert to markdown.
pub async fn fetch_and_process(
    dir: &str,
    file: &str,
    section_id: &str,
    client: &Arc<reqwest::Client>,
    cache: &SharedDocsCache,
    cache_path: &std::path::Path,
    force: bool,
) -> Result<DocPage, String> {
    if !force {
        let guard = cache.read().await;
        if let Some(page) = guard.pages.get(section_id) {
            if !page.is_stale() {
                return Ok(page.clone());
            }
        }
    }

    let attrs = ensure_attributes(cache, client, cache_path).await;

    let repo_path = format!("{}/{}", dir, file);
    let url = format!("{}/{}", RAW_BASE, repo_path);
    debug!("Fetching assembly: {}", url);

    let assembly = fetch_raw(client, &url).await?;
    let resolved = resolve_includes(client, &assembly, dir).await;
    let (title, content) = asciidoc::to_markdown(&resolved, &attrs);

    let title = if title.is_empty() {
        section_id.to_string()
    } else {
        title
    };

    let page = DocPage {
        section_id: section_id.to_string(),
        title,
        repo_path,
        content,
        fetched_at: Utc::now(),
    };

    {
        let mut guard = cache.write().await;
        guard.pages.insert(section_id.to_string(), page.clone());
        guard.save(cache_path);
    }

    Ok(page)
}

/// Fetch a raw file from GitHub.
async fn fetch_raw(client: &Arc<reqwest::Client>, url: &str) -> Result<String, String> {
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("HTTP error fetching {}: {}", url, e))?;

    let status = resp.status();
    if !status.is_success() {
        return Err(format!("HTTP {} for {}", status, url));
    }

    resp.text()
        .await
        .map_err(|e| format!("Failed to read body from {}: {}", url, e))
}

/// Resolve `include::modules/...` and `include::snippets/...` directives by fetching module files.
async fn resolve_includes(
    client: &Arc<reqwest::Client>,
    content: &str,
    _dir: &str,
) -> String {
    let include_re = Regex::new(r"^include::(modules/[^\[]+|snippets/[^\[]+)\[([^\]]*)\]")
        .unwrap();
    let leveloffset_re = Regex::new(r"leveloffset=\+?(\d+)").unwrap();

    let mut result = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(caps) = include_re.captures(trimmed) {
            let module_path = &caps[1];
            let opts = &caps[2];
            let offset: i32 = leveloffset_re
                .captures(opts)
                .and_then(|c| c[1].parse().ok())
                .unwrap_or(0);

            let url = format!("{}/{}", RAW_BASE, module_path);
            match fetch_raw(client, &url).await {
                Ok(module_content) => {
                    if offset > 0 {
                        let prefix = "=".repeat(offset as usize);
                        for mline in module_content.lines() {
                            if mline.starts_with('=') && mline.contains(' ') {
                                result.push(format!("{}{}", prefix, mline));
                            } else {
                                result.push(mline.to_string());
                            }
                        }
                    } else {
                        for mline in module_content.lines() {
                            result.push(mline.to_string());
                        }
                    }
                }
                Err(e) => {
                    debug!("Could not resolve include {}: {}", module_path, e);
                }
            }
        } else {
            result.push(line.to_string());
        }
    }

    result.join("\n")
}

/// Fetch an arbitrary repo path (not from the curated index).
pub async fn fetch_arbitrary_path(
    path: &str,
    client: &Arc<reqwest::Client>,
    cache: &SharedDocsCache,
    cache_path: &std::path::Path,
    force: bool,
) -> Result<DocPage, String> {
    let section_id = path
        .trim_start_matches("virt/")
        .replace('/', "-")
        .replace(".adoc", "");

    if !force {
        let guard = cache.read().await;
        if let Some(page) = guard.pages.get(&section_id) {
            if !page.is_stale() {
                return Ok(page.clone());
            }
        }
    }

    let attrs = ensure_attributes(cache, client, cache_path).await;

    let url = format!("{}/{}", RAW_BASE, path);
    debug!("Fetching arbitrary path: {}", url);

    let assembly = fetch_raw(client, &url).await?;

    let dir = path.rsplitn(2, '/').nth(1).unwrap_or("virt");
    let resolved = resolve_includes(client, &assembly, dir).await;
    let (title, content) = asciidoc::to_markdown(&resolved, &attrs);

    let title = if title.is_empty() {
        section_id.clone()
    } else {
        title
    };

    let page = DocPage {
        section_id: section_id.clone(),
        title,
        repo_path: path.to_string(),
        content,
        fetched_at: Utc::now(),
    };

    {
        let mut guard = cache.write().await;
        guard.pages.insert(section_id, page.clone());
        guard.save(cache_path);
    }

    Ok(page)
}
