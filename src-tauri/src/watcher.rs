// SmartSearch — File System Watcher
// Uses the `notify` crate to watch collection directories for changes
// and triggers re-indexing via a background queue.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use log::{debug, error, info, warn};
use notify::event::{CreateKind, ModifyKind, RemoveKind};
use notify::{Config, Event, EventKind, RecursiveMode, Watcher};
use rusqlite::Connection;
use tauri::AppHandle;
use tauri::Emitter;
use tokio::sync::mpsc;

use crate::scanner;

/// Represents a file change event
#[derive(Debug, Clone)]
pub enum FileChange {
    Created(PathBuf),
    Modified(PathBuf),
    Deleted(PathBuf),
}

/// Start the file watcher for all collection paths
///
/// Returns a shutdown sender that can be used to stop the watcher.
pub fn start_watcher(
    db: Arc<Mutex<Connection>>,
    collection_paths: Vec<String>,
    event_sender: Option<tauri::AppHandle>,
) -> Result<mpsc::Sender<()>, String> {
    let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);
    let (change_tx, mut change_rx) = mpsc::channel::<FileChange>(500);

    // Create the file system watcher
    let change_tx_clone = change_tx.clone();

    let mut watcher = notify::recommended_watcher(move |result: Result<Event, notify::Error>| {
        match result {
            Ok(event) => {
                for path in &event.paths {
                    // Only watch text-like files
                    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                        let is_text_file = matches!(
                            ext,
                            "md" | "txt"
                                | "rs"
                                | "py"
                                | "ts"
                                | "tsx"
                                | "js"
                                | "jsx"
                                | "go"
                                | "java"
                                | "c"
                                | "cpp"
                                | "h"
                                | "rb"
                                | "swift"
                                | "yaml"
                                | "yml"
                                | "toml"
                                | "json"
                                | "html"
                                | "css"
                                | "scss"
                                | "vue"
                                | "svelte"
                                | "sh"
                                | "bash"
                                | "org"
                                | "tex"
                        );

                        if !is_text_file {
                            continue;
                        }
                    } else {
                        continue;
                    }

                    let change = match event.kind {
                        EventKind::Create(_) => Some(FileChange::Created(path.clone())),
                        EventKind::Modify(_) => Some(FileChange::Modified(path.clone())),
                        EventKind::Remove(_) => Some(FileChange::Deleted(path.clone())),
                        _ => None,
                    };

                    if let Some(change) = change {
                        let _ = change_tx_clone.try_send(change);
                    }
                }
            }
            Err(e) => {
                error!("File watcher error: {:?}", e);
            }
        }
    })
    .map_err(|e| format!("Failed to create watcher: {}", e))?;

    // Watch all collection directories
    for path in &collection_paths {
        let watch_path = PathBuf::from(path);
        if watch_path.exists() {
            watcher
                .watch(&watch_path, RecursiveMode::Recursive)
                .map_err(|e| format!("Failed to watch {}: {}", path, e))?;
            info!("Watching directory: {}", path);
        } else {
            warn!("Collection path does not exist, skipping watch: {}", path);
        }
    }

    // Spawn background task to process changes with debouncing
    tokio::spawn(async move {
        // Keep watcher alive
        let _watcher = watcher;

        // Debounce map: path -> last event time
        let mut pending: HashMap<PathBuf, (FileChange, tokio::time::Instant)> = HashMap::new();
        let debounce_duration = Duration::from_millis(500);

        let mut interval = tokio::time::interval(Duration::from_millis(250));

        loop {
            tokio::select! {
                // Receive new changes
                Some(change) = change_rx.recv() => {
                    let path = match &change {
                        FileChange::Created(p) | FileChange::Modified(p) | FileChange::Deleted(p) => p.clone(),
                    };
                    pending.insert(path, (change, tokio::time::Instant::now()));
                }

                // Process debounced changes
                _ = interval.tick() => {
                    let now = tokio::time::Instant::now();
                    let mut to_process: Vec<(PathBuf, FileChange)> = Vec::new();

                    pending.retain(|path, (change, timestamp)| {
                        if now.duration_since(*timestamp) >= debounce_duration {
                            to_process.push((path.clone(), change.clone()));
                            false // Remove from pending
                        } else {
                            true // Keep waiting
                        }
                    });

                    // Process matured changes
                    for (path, change) in to_process {
                        debug!("Processing file change: {:?}", change);

                        match &change {
                            FileChange::Created(p) | FileChange::Modified(p) => {
                                let db_lock = db.lock().unwrap();
                                match scanner::index_single_file(&db_lock, p) {
                                    Ok(true) => {
                                        info!("Re-indexed: {:?}", p);
                                        // Emit event to frontend
                                        if let Some(ref handle) = event_sender {
                                            let _ = handle.emit("index-updated", serde_json::json!({
                                                "path": p.to_string_lossy().to_string(),
                                                "action": "updated"
                                            }));
                                        }
                                    }
                                    Ok(false) => {
                                        debug!("File unchanged: {:?}", p);
                                    }
                                    Err(e) => {
                                        warn!("Failed to re-index {:?}: {}", p, e);
                                    }
                                }
                            }
                            FileChange::Deleted(p) => {
                                debug!("File deleted: {:?}", p);
                                // Document will be marked inactive on next scan
                                if let Some(ref handle) = event_sender {
                                    let _ = handle.emit("index-updated", serde_json::json!({
                                        "path": p.to_string_lossy().to_string(),
                                        "action": "deleted"
                                    }));
                                }
                            }
                        }
                    }
                }

                // Shutdown signal
                _ = shutdown_rx.recv() => {
                    info!("File watcher shutting down");
                    break;
                }
            }
        }
    });

    Ok(shutdown_tx)
}
