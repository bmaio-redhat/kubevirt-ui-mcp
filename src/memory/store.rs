use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TicketRecord {
    pub key: String,
    pub summary: String,
    pub status: String,
    pub issue_type: String,
    pub labels: Vec<String>,
    pub components: Vec<String>,
    pub fix_versions: Vec<String>,
    /// First 500 chars of the description plain text
    pub description_excerpt: String,
    /// Commit SHAs in kubevirt-plugin that reference this ticket
    pub commit_shas: Vec<String>,
    pub fetched_at: DateTime<Utc>,
}

impl TicketRecord {
    pub fn is_stale(&self) -> bool {
        let age = Utc::now().signed_duration_since(self.fetched_at);
        age.num_days() >= 7
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Store {
    pub tickets: HashMap<String, TicketRecord>,
}

impl Store {
    pub fn load(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(json) => match serde_json::from_str(&json) {
                Ok(store) => {
                    info!("Loaded store from {}", path.display());
                    store
                }
                Err(e) => {
                    warn!("Failed to parse store.json ({}), starting fresh", e);
                    Self::default()
                }
            },
            Err(_) => {
                info!("No existing store at {}, starting fresh", path.display());
                Self::default()
            }
        }
    }

    pub fn save(&self, path: &Path) {
        if let Some(parent) = path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                warn!("Could not create store dir: {}", e);
                return;
            }
        }
        match serde_json::to_string_pretty(self) {
            Ok(json) => {
                if let Err(e) = std::fs::write(path, json) {
                    warn!("Failed to write store.json: {}", e);
                } else {
                    info!("Store saved to {} ({} tickets)", path.display(), self.tickets.len());
                }
            }
            Err(e) => warn!("Failed to serialize store: {}", e),
        }
    }

    pub fn upsert(&mut self, record: TicketRecord) {
        self.tickets.insert(record.key.clone(), record);
    }

    pub fn get(&self, key: &str) -> Option<&TicketRecord> {
        self.tickets.get(key)
    }

    pub fn len(&self) -> usize {
        self.tickets.len()
    }
}

pub type SharedStore = Arc<RwLock<Store>>;

pub fn shared(store: Store) -> SharedStore {
    Arc::new(RwLock::new(store))
}
