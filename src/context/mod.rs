pub mod indexer;
pub mod watcher;
pub mod tools;

pub use indexer::{Index, Indexer};
pub use watcher::{rebuild_index, spawn_async_watcher};

use std::sync::Arc;
use tokio::sync::RwLock;

pub type SharedIndex = Arc<RwLock<Arc<Index>>>;

pub fn new_shared_index(index: Index) -> SharedIndex {
    Arc::new(RwLock::new(Arc::new(index)))
}
