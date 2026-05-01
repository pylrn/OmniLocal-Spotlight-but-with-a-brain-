// SmartSearch — File System Watcher
// Watches collection directories for changes and triggers re-indexing.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use log::{debug, error, info, warn};
use notify::{Event, EventKind, RecursiveMode, Watcher};
use rusqlite::Connection;
use tauri::Emitter;
use tokio::sync::mpsc;

use crate::{scanner, EmbedCommand};

#[derive(Debug, Clone)]
pub enum FileChange {
    Created(PathBuf),
    Modified(PathBuf),
    Deleted(PathBuf),
}

pub fn start_watcher(
    db: Arc<Mutex<Connection>>,
    collection_paths: Vec<String>,
    event_sender: Option<tauri::AppHandle>,
    embed_sender: Option<mpsc::UnboundedSender<EmbedCommand>>,
) -> Result<mpsc::Sender<()>, String> {
    let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);
    let (change_tx, mut change_rx) = mpsc::channel::<FileChange>(500);

    let change_tx_clone = change_tx.clone();
    let mut watcher = notify::recommended_watcher(move |result: Result<Event, notify::Error>| match result {
        Ok(event) => {
            for path in &event.paths {
                if !is_supported_watch_path(path) {
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
        Err(error) => error!("File watcher error: {:?}", error),
    })
    .map_err(|e| format!("Failed to create watcher: {}", e))?;

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

    std::thread::spawn(move || {
        tauri::async_runtime::block_on(async move {
            let _watcher = watcher;
            let mut pending: HashMap<PathBuf, (FileChange, tokio::time::Instant)> = HashMap::new();
            let debounce_duration = Duration::from_millis(500);
            let mut interval = tokio::time::interval(Duration::from_millis(250));

            loop {
                tokio::select! {
                    Some(change) = change_rx.recv() => {
                        let path = match &change {
                            FileChange::Created(path) | FileChange::Modified(path) | FileChange::Deleted(path) => path.clone(),
                        };
                        pending.insert(path, (change, tokio::time::Instant::now()));
                    }
                    _ = interval.tick() => {
                        let now = tokio::time::Instant::now();
                        let mut to_process = Vec::new();

                        pending.retain(|path, (change, timestamp)| {
                            if now.duration_since(*timestamp) >= debounce_duration {
                                to_process.push((path.clone(), change.clone()));
                                false
                            } else {
                                true
                            }
                        });

                        for (_path, change) in to_process {
                            debug!("Processing file change: {:?}", change);
                            match &change {
                                FileChange::Created(path) | FileChange::Modified(path) => {
                                    let db_lock = db.lock().unwrap();
                                    match scanner::index_single_file(&db_lock, path) {
                                        Ok(true) => {
                                            info!("Re-indexed: {:?}", path);
                                            if let Some(ref handle) = event_sender {
                                                let _ = handle.emit(
                                                    "index-progress",
                                                    serde_json::json!({
                                                        "phase": "watcher-update",
                                                        "path": path.to_string_lossy().to_string(),
                                                        "action": "updated"
                                                    }),
                                                );
                                            }
                                            if let Some(ref embed_tx) = embed_sender {
                                                let _ = embed_tx.send(EmbedCommand::Kick);
                                            }
                                        }
                                        Ok(false) => debug!("File unchanged: {:?}", path),
                                        Err(error) => warn!("Failed to re-index {:?}: {}", path, error),
                                    }
                                }
                                FileChange::Deleted(path) => {
                                    if let Some(ref handle) = event_sender {
                                        let _ = handle.emit(
                                            "index-progress",
                                            serde_json::json!({
                                                "phase": "watcher-update",
                                                "path": path.to_string_lossy().to_string(),
                                                "action": "deleted"
                                            }),
                                        );
                                    }
                                }
                            }
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        info!("File watcher shutting down");
                        break;
                    }
                }
            }
        })
    });

    Ok(shutdown_tx)
}

fn is_supported_watch_path(path: &std::path::Path) -> bool {
    let ext = match path.extension().and_then(|ext| ext.to_str()) {
        Some(ext) => ext.to_lowercase(),
        None => return false,
    };

    matches!(
        ext.as_str(),
        "md"
            | "txt"
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
            | "pdf"
            | "docx"
    )
}
