use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocPage {
    pub section_id: String,
    pub title: String,
    pub repo_path: String,
    pub content: String,
    pub fetched_at: DateTime<Utc>,
}

impl DocPage {
    pub fn is_stale(&self) -> bool {
        let age = Utc::now().signed_duration_since(self.fetched_at);
        age.num_days() >= 7
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct DocsCache {
    pub pages: HashMap<String, DocPage>,
    pub attributes: HashMap<String, String>,
    pub attributes_fetched_at: Option<DateTime<Utc>>,
}

impl DocsCache {
    pub fn load(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(json) => match serde_json::from_str(&json) {
                Ok(cache) => {
                    info!("Loaded docs cache from {}", path.display());
                    cache
                }
                Err(e) => {
                    warn!("Failed to parse docs-cache.json ({}), starting fresh", e);
                    Self::default()
                }
            },
            Err(_) => {
                info!("No existing docs cache at {}, starting fresh", path.display());
                Self::default()
            }
        }
    }

    pub fn save(&self, path: &Path) {
        if let Some(parent) = path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                warn!("Could not create docs cache dir: {}", e);
                return;
            }
        }
        match serde_json::to_string_pretty(self) {
            Ok(json) => {
                if let Err(e) = std::fs::write(path, json) {
                    warn!("Failed to write docs-cache.json: {}", e);
                } else {
                    info!("Docs cache saved to {} ({} pages)", path.display(), self.pages.len());
                }
            }
            Err(e) => warn!("Failed to serialize docs cache: {}", e),
        }
    }

    pub fn attributes_stale(&self) -> bool {
        match self.attributes_fetched_at {
            Some(ts) => {
                let age = Utc::now().signed_duration_since(ts);
                age.num_days() >= 7
            }
            None => true,
        }
    }

    pub fn search(&self, query: &str) -> Vec<SearchHit> {
        let terms: Vec<String> = query
            .split_whitespace()
            .map(|t| t.to_lowercase())
            .collect();

        if terms.is_empty() {
            return Vec::new();
        }

        let mut hits: Vec<SearchHit> = Vec::new();

        for page in self.pages.values() {
            let content_lower = page.content.to_lowercase();
            let title_lower = page.title.to_lowercase();

            let all_match = terms.iter().all(|t| content_lower.contains(t) || title_lower.contains(t));
            if !all_match {
                continue;
            }

            let lines: Vec<&str> = page.content.lines().collect();
            let mut snippets: Vec<String> = Vec::new();

            for (i, line) in lines.iter().enumerate() {
                let line_lower = line.to_lowercase();
                if terms.iter().any(|t| line_lower.contains(t)) {
                    let start = i.saturating_sub(1);
                    let end = (i + 2).min(lines.len());
                    let snippet: String = lines[start..end].join("\n");
                    snippets.push(snippet);
                    if snippets.len() >= 5 {
                        break;
                    }
                }
            }

            hits.push(SearchHit {
                section_id: page.section_id.clone(),
                title: page.title.clone(),
                snippets,
            });
        }

        hits.sort_by(|a, b| a.section_id.cmp(&b.section_id));
        hits
    }
}

#[derive(Debug)]
pub struct SearchHit {
    pub section_id: String,
    pub title: String,
    pub snippets: Vec<String>,
}

pub type SharedDocsCache = Arc<RwLock<DocsCache>>;

pub fn shared(cache: DocsCache) -> SharedDocsCache {
    Arc::new(RwLock::new(cache))
}
