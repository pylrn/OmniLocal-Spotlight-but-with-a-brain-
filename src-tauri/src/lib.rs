// SmartSearch — Main Tauri Application
// Wires up all modules, manages state, and exposes IPC commands.

mod db;
mod ai;
mod chunker;
mod scanner;
mod search;
mod watcher;
mod context;
pub mod mcp;

use std::sync::{Arc, Mutex};
use std::path::PathBuf;
use rusqlite::Connection;
use tauri::Manager;
use log::info;

use db::{CollectionRow, IndexStats};
use ai::{AiProvider, ProviderStatus};
use search::SearchResult;
use scanner::ScanResult;

/// Shared application state
pub struct AppState {
    db: Arc<Mutex<Connection>>,
    ai_provider: Arc<Mutex<Option<AiProvider>>>,
}

// ═══════════════════════════════════════════════════════════════════════
// Tauri IPC Commands — Collections
// ═══════════════════════════════════════════════════════════════════════

#[tauri::command]
fn add_collection(
    state: tauri::State<'_, AppState>,
    name: String,
    path: String,
    glob_pattern: Option<String>,
    context: Option<String>,
) -> Result<i64, String> {
    let conn = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;
    let pattern = glob_pattern.unwrap_or_else(|| "**/*.md".to_string());

    db::insert_collection(&conn, &name, &path, &pattern, context.as_deref())
        .map_err(|e| format!("Failed to add collection: {}", e))
}

#[tauri::command]
fn list_collections(state: tauri::State<'_, AppState>) -> Result<Vec<CollectionRow>, String> {
    let conn = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;
    db::list_collections(&conn).map_err(|e| format!("Failed to list collections: {}", e))
}

#[tauri::command]
fn remove_collection(state: tauri::State<'_, AppState>, name: String) -> Result<bool, String> {
    let conn = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;
    db::remove_collection(&conn, &name).map_err(|e| format!("Failed to remove collection: {}", e))
}

// ═══════════════════════════════════════════════════════════════════════
// Tauri IPC Commands — Indexing
// ═══════════════════════════════════════════════════════════════════════

#[tauri::command]
async fn add_manual_files(state: tauri::State<'_, AppState>, paths: Vec<String>) -> Result<(), String> {
    let db_arc = state.db.clone();
    
    tauri::async_runtime::spawn_blocking(move || {
        let conn = db_arc.lock().map_err(|e| format!("Lock error: {}", e))?;
        
        let collections = db::list_collections(&conn).unwrap_or_default();
        let manual_col = collections.iter().find(|c| c.name == "Standalone Files");
        let col_id = if let Some(col) = manual_col {
            col.id
        } else {
            db::insert_collection(&conn, "Standalone Files", "/", "**/*", None)
                .map_err(|e| format!("DB Error: {}", e))?
        };

        for path in paths {
            let p = PathBuf::from(&path);
            let _ = scanner::index_manual_file(&conn, &p, col_id);
        }
        
        Ok(())
    }).await.unwrap_or(Err("Runtime error".to_string()))
}

#[tauri::command]
fn list_indexed_files(state: tauri::State<'_, AppState>) -> Result<Vec<db::DocumentRow>, String> {
    let conn = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;
    db::list_documents(&conn).map_err(|e| format!("DB access failed: {}", e))
}

#[tauri::command]
fn remove_manual_file(state: tauri::State<'_, AppState>, document_id: i64) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;
    db::remove_document(&conn, document_id)
        .map_err(|e| format!("Failed to remove document: {}", e))
}

#[tauri::command]
async fn scan_collections(state: tauri::State<'_, AppState>) -> Result<Vec<ScanResult>, String> {
    let db_arc = state.db.clone();
    
    tauri::async_runtime::spawn_blocking(move || {
        let conn = db_arc.lock().map_err(|e| format!("Lock error: {}", e))?;

        let collections = db::list_collections(&conn)
            .map_err(|e| format!("Failed to list collections: {}", e))?;

        let mut results = Vec::new();
        for col in collections {
            let glob = col.glob_pattern.unwrap_or_else(|| "**/*.md".to_string());
            match scanner::scan_collection(&conn, col.id, &col.path, &glob, &col.name) {
                Ok(result) => results.push(result),
                Err(e) => results.push(ScanResult {
                    collection_name: col.name,
                    files_found: 0,
                    files_indexed: 0,
                    files_unchanged: 0,
                    files_removed: 0,
                    errors: vec![e],
                }),
            }
        }

        Ok(results)
    })
    .await
    .map_err(|e| format!("Tokio task panicked: {}", e))?
}

// ═══════════════════════════════════════════════════════════════════════
// Tauri IPC Commands — Search
// ═══════════════════════════════════════════════════════════════════════

#[tauri::command]
fn search_keyword(
    state: tauri::State<'_, AppState>,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<SearchResult>, String> {
    let conn = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;
    let limit = limit.unwrap_or(10);

    // Get foreground app for context boosting
    let foreground_app = context::get_foreground_app();

    let mut results = search::keyword_search(&conn, &query, limit)?;

    // Apply active context boosting
    if let Some(ref app_name) = foreground_app {
        for result in &mut results {
            let boost = search::get_context_boost(app_name, &result.file_type);
            if boost > 0.0 {
                result.score *= 1.0 + boost;
                result.context_boosted = true;
            }
        }
        // Re-sort after boosting
        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    }

    Ok(results)
}

// ═══════════════════════════════════════════════════════════════════════
// Tauri IPC Commands — Status & Settings
// ═══════════════════════════════════════════════════════════════════════

#[tauri::command]
fn get_index_stats(state: tauri::State<'_, AppState>) -> Result<IndexStats, String> {
    let conn = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;
    db::get_index_stats(&conn).map_err(|e| format!("Failed to get stats: {}", e))
}

#[tauri::command]
async fn check_ai_status(state: tauri::State<'_, AppState>) -> Result<ProviderStatus, String> {
    let provider = {
        let guard = state.ai_provider.lock().map_err(|e| format!("Lock error: {}", e))?;
        guard.clone()
    };

    match provider {
        Some(p) => Ok(p.check_status().await),
        None => Err("AI provider not configured".to_string()),
    }
}

#[tauri::command]
fn get_setting(state: tauri::State<'_, AppState>, key: String) -> Result<Option<String>, String> {
    let conn = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;
    db::get_setting(&conn, &key).map_err(|e| format!("Failed to get setting: {}", e))
}

#[tauri::command]
fn set_setting(state: tauri::State<'_, AppState>, key: String, value: String) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;
    db::set_setting(&conn, &key, &value).map_err(|e| format!("Failed to set setting: {}", e))
}

#[tauri::command]
fn get_foreground_app() -> Option<String> {
    context::get_foreground_app()
}

// ═══════════════════════════════════════════════════════════════════════
// Application Setup
// ═══════════════════════════════════════════════════════════════════════

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, shortcut, event| {
                    use tauri_plugin_global_shortcut::{Code, Modifiers, ShortcutState};
                    if event.state() == ShortcutState::Pressed {
                        if shortcut.matches(Modifiers::SUPER | Modifiers::SHIFT, Code::Space) {
                            if let Some(window) = app.get_webview_window("main") {
                                let is_visible = window.is_visible().unwrap_or(false);
                                if is_visible {
                                    let _ = window.hide();
                                } else {
                                    let _ = window.show();
                                    let _ = window.set_focus();
                                }
                            }
                        }
                    }
                })
                .build(),
        )
        .setup(|app| {
            // Register global shortcut
            use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut, GlobalShortcutExt};
            let toggle_shortcut = Shortcut::new(Some(Modifiers::SUPER | Modifiers::SHIFT), Code::Space);
            if let Err(e) = app.global_shortcut().register(toggle_shortcut) {
                log::error!("Failed to register global shortcut: {}", e);
            }
            // Determine database path in app data directory
            #[cfg(debug_assertions)]
            let app_data = std::env::current_dir().unwrap().join(".data");
            #[cfg(not(debug_assertions))]
            let app_data = app
                .path()
                .app_data_dir()
                .expect("Failed to get app data directory");

            std::fs::create_dir_all(&app_data)
                .expect("Failed to create app data directory");

            let db_path = app_data.join("smartsearch.db");
            info!("Database path: {:?}", db_path);

            // Initialize database
            let conn = db::init_db(&db_path)
                .expect("Failed to initialize database");

            // Read AI provider settings
            let ai_provider_type = db::get_setting(&conn, "ai_provider")
                .ok()
                .flatten()
                .unwrap_or_else(|| "ollama".to_string());
            let ollama_url = db::get_setting(&conn, "ollama_base_url")
                .ok()
                .flatten()
                .unwrap_or_else(|| "http://localhost:11434".to_string());
            let lmstudio_url = db::get_setting(&conn, "lmstudio_base_url")
                .ok()
                .flatten()
                .unwrap_or_else(|| "http://localhost:1234".to_string());
            let embed_model = db::get_setting(&conn, "embed_model")
                .ok()
                .flatten()
                .unwrap_or_else(|| "nomic-embed-text".to_string());

            let provider = AiProvider::from_settings(
                &ai_provider_type,
                &ollama_url,
                &lmstudio_url,
                &embed_model,
            );

            // Get collection paths for file watcher
            let collection_paths: Vec<String> = db::list_collections(&conn)
                .unwrap_or_default()
                .iter()
                .map(|c| c.path.clone())
                .collect();

            let db_arc = Arc::new(Mutex::new(conn));

            // Start file watcher if enabled
            let watch_enabled = db::get_setting(
                &db_arc.lock().unwrap(),
                "watch_enabled",
            )
            .ok()
            .flatten()
            .unwrap_or_else(|| "true".to_string());

            if watch_enabled == "true" && !collection_paths.is_empty() {
                let db_clone = Arc::clone(&db_arc);
                let app_handle = app.handle().clone();

                match watcher::start_watcher(db_clone, collection_paths, Some(app_handle)) {
                    Ok(_shutdown) => {
                        info!("File watcher started");
                    }
                    Err(e) => {
                        log::error!("Failed to start file watcher: {}", e);
                    }
                }
            }

            // Store state
            app.manage(AppState {
                db: db_arc,
                ai_provider: Arc::new(Mutex::new(Some(provider))),
            });

            info!("SmartSearch initialized");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            add_collection,
            list_collections,
            remove_collection,
            add_manual_files,
            list_indexed_files,
            remove_manual_file,
            scan_collections,
            search_keyword,
            get_index_stats,
            check_ai_status,
            get_setting,
            set_setting,
            get_foreground_app,
        ])
        .run(tauri::generate_context!())
        .expect("error while running SmartSearch");
}
