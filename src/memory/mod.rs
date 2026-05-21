pub mod config;
pub mod github;
pub mod jira;
pub mod store;
pub mod tools;

pub use store::{SharedStore, Store};

use std::sync::Arc;
use tracing::{info, warn};

use crate::config::Config;

pub fn new_shared_store(cfg: &Config) -> SharedStore {
    let existing = Store::load(&cfg.store_path);
    store::shared(existing)
}

/// Fetch commits → extract ticket IDs → fetch Jira details → persist.
pub async fn run_refresh(
    client: &reqwest::Client,
    shared: &SharedStore,
    cfg: &Config,
) {
    info!("Starting background store refresh…");

    let ticket_commits =
        github::fetch_ticket_commits(client, &cfg.github_repo, cfg.github_commit_pages).await;

    if ticket_commits.is_empty() {
        warn!("No CNV ticket references found in commit history.");
        return;
    }

    let keys_to_fetch: Vec<String> = {
        let store = shared.read().await;
        ticket_commits
            .keys()
            .filter(|k| store.get(k).map(|r| r.is_stale()).unwrap_or(true))
            .cloned()
            .collect()
    };

    info!(
        "{} tickets to fetch from Jira ({} already cached)",
        keys_to_fetch.len(),
        ticket_commits.len().saturating_sub(keys_to_fetch.len())
    );

    for key in &keys_to_fetch {
        if let Some(mut record) = jira::fetch_ticket(client, &cfg.jira_base_url, key).await {
            record.commit_shas = ticket_commits.get(key).cloned().unwrap_or_default();
            shared.write().await.upsert(record);
        }
    }

    {
        let mut store = shared.write().await;
        for (key, shas) in &ticket_commits {
            if let Some(record) = store.tickets.get_mut(key) {
                for sha in shas {
                    if !record.commit_shas.contains(sha) {
                        record.commit_shas.push(sha.clone());
                    }
                }
            }
        }
    }

    let store = shared.read().await;
    store.save(&cfg.store_path);
    info!("Store refresh complete. {} tickets.", store.len());
}
