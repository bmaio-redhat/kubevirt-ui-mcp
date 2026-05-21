#![allow(dead_code)]
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, info, warn};

use crate::context::indexer::{Index, Indexer};

/// Spawns a background task that watches `watch_dirs` for changes and rebuilds
/// the index into `index_slot` whenever TypeScript source files are modified.
pub fn spawn_watcher(
    watch_dirs: Vec<PathBuf>,
    playwright_root: PathBuf,
    index_slot: Arc<RwLock<Arc<Index>>>,
) {
    std::thread::spawn(move || {
        if let Err(e) = run_watcher(watch_dirs, playwright_root, index_slot) {
            warn!("File watcher stopped: {}", e);
        }
    });
}

fn run_watcher(
    watch_dirs: Vec<PathBuf>,
    playwright_root: PathBuf,
    index_slot: Arc<RwLock<Arc<Index>>>,
) -> notify::Result<()> {
    let (tx, rx) = std::sync::mpsc::channel::<notify::Result<Event>>();

    let mut watcher = RecommendedWatcher::new(tx, Config::default().with_poll_interval(Duration::from_secs(2)))?;

    for dir in &watch_dirs {
        if dir.exists() {
            watcher.watch(dir, RecursiveMode::Recursive)?;
            info!("Watching {} for changes", dir.display());
        }
    }

    let mut pending_rebuild = false;
    let debounce = Duration::from_millis(500);
    let mut last_event = std::time::Instant::now();

    loop {
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(Ok(event)) => {
                if is_ts_file_event(&event) {
                    debug!("TypeScript file changed: {:?}", event.paths);
                    pending_rebuild = true;
                    last_event = std::time::Instant::now();
                }
            }
            Ok(Err(e)) => warn!("Watch error: {}", e),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }

        if pending_rebuild && last_event.elapsed() >= debounce {
            pending_rebuild = false;
            info!("Rebuilding index after file change...");
            let indexer = Indexer::new(playwright_root.clone());
            let new_index = Arc::new(indexer.build());
            // Block on the async RwLock from sync context using a fresh runtime
            let rt = tokio::runtime::Handle::try_current();
            match rt {
                Ok(handle) => {
                    handle.block_on(async {
                        let mut guard = index_slot.write().await;
                        *guard = new_index;
                    });
                }
                Err(_) => {
                    // Not in a tokio context — use a one-shot runtime
                    let _ = tokio::runtime::Runtime::new().map(|rt| {
                        rt.block_on(async {
                            let mut guard = index_slot.write().await;
                            *guard = new_index;
                        });
                    });
                }
            }
            info!("Index rebuilt.");
        }
    }

    Ok(())
}

fn is_ts_file_event(event: &Event) -> bool {
    matches!(
        event.kind,
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
    ) && event.paths.iter().any(|p| {
        p.extension().and_then(|e| e.to_str()) == Some("ts")
            && !p
                .to_string_lossy()
                .contains("node_modules")
    })
}

/// Rebuild the index manually (called when agent requests an explicit refresh).
pub async fn rebuild_index(playwright_root: PathBuf, index_slot: Arc<RwLock<Arc<Index>>>) {
    info!("Manual index rebuild requested.");
    let new_index = tokio::task::spawn_blocking(move || {
        let indexer = Indexer::new(playwright_root);
        Arc::new(indexer.build())
    })
    .await
    .unwrap_or_else(|_| Arc::new(Index::default()));

    let mut guard = index_slot.write().await;
    *guard = new_index;
    info!("Index rebuilt. {} classes indexed.", guard.classes.len());
}

/// Tokio channel-based watcher variant that integrates cleanly with the async runtime.
pub fn spawn_async_watcher(
    watch_dirs: Vec<PathBuf>,
    playwright_root: PathBuf,
    index_slot: Arc<RwLock<Arc<Index>>>,
) {
    tokio::spawn(async move {
        let (tx, mut rx) = mpsc::unbounded_channel::<Vec<PathBuf>>();

        // Run the blocking watcher in a dedicated thread
        let tx_clone = tx.clone();
        std::thread::spawn(move || {
            let (notify_tx, notify_rx) = std::sync::mpsc::channel::<notify::Result<Event>>();
            let mut watcher = match RecommendedWatcher::new(
                notify_tx,
                Config::default().with_poll_interval(Duration::from_secs(2)),
            ) {
                Ok(w) => w,
                Err(e) => {
                    warn!("Could not create file watcher: {}", e);
                    return;
                }
            };

            for dir in &watch_dirs {
                if dir.exists() {
                    let _ = watcher.watch(dir, RecursiveMode::Recursive);
                }
            }

            loop {
                match notify_rx.recv_timeout(Duration::from_millis(500)) {
                    Ok(Ok(event)) if is_ts_file_event(&event) => {
                        let _ = tx_clone.send(event.paths.clone());
                    }
                    Ok(_) => {}
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                    Err(_) => {}
                }
            }
        });

        // Debounced rebuild on the async side
        let mut last_rebuild = tokio::time::Instant::now();
        let cooldown = tokio::time::Duration::from_secs(2);

        while let Some(_paths) = rx.recv().await {
            // Drain any pending
            while rx.try_recv().is_ok() {}

            if last_rebuild.elapsed() >= cooldown {
                last_rebuild = tokio::time::Instant::now();
                let root = playwright_root.clone();
                let slot = index_slot.clone();
                tokio::spawn(async move {
                    rebuild_index(root, slot).await;
                });
            }
        }
    });
}
