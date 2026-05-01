// SmartSearch — Main Tauri Application
// Wires up modules, background embedding, live status, and IPC commands.

pub mod core;
pub mod commands;
pub mod workers;

use std::sync::{Arc, Mutex};
use log::info;
use rusqlite::Connection;
use tauri::Manager;
use tokio::sync::mpsc;

use crate::core::ai::AiProvider;
use crate::core::db;
use crate::workers::embedding::{self, EmbeddingRuntimeStatus, EmbedCommand};

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Mutex<Connection>>,
    pub ai_provider: Arc<Mutex<Option<AiProvider>>>,
    pub db_path: String,
    pub watcher_enabled: Arc<Mutex<bool>>,
    pub embedding_tx: mpsc::UnboundedSender<EmbedCommand>,
    pub embedding_runtime: Arc<Mutex<EmbeddingRuntimeStatus>>,
}

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
                    if event.state() == ShortcutState::Pressed
                        && shortcut.matches(Modifiers::SUPER | Modifiers::SHIFT, Code::Space)
                    {
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
                })
                .build(),
        )
        .setup(|app| {
            use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut};

            let toggle_shortcut = Shortcut::new(Some(Modifiers::SUPER | Modifiers::SHIFT), Code::Space);
            if let Err(error) = app.global_shortcut().register(toggle_shortcut) {
                log::error!("Failed to register global shortcut: {}", error);
            }

            #[cfg(debug_assertions)]
            let app_data = std::env::current_dir().unwrap().join(".data");
            #[cfg(not(debug_assertions))]
            let app_data = app.path().app_data_dir().expect("Failed to get app data directory");

            std::fs::create_dir_all(&app_data).expect("Failed to create app data directory");

            let db_path = app_data.join("smartsearch.db");
            info!("Database path: {:?}", db_path);

            let conn = db::init_db(&db_path).expect("Failed to initialize database");
            let provider = read_provider_from_conn(&conn);
            let collection_paths: Vec<String> = db::list_collections(&conn)
                .unwrap_or_default()
                .iter()
                .map(|collection| collection.path.clone())
                .collect();
            let watch_enabled = db::get_setting(&conn, "watch_enabled")
                .ok()
                .flatten()
                .unwrap_or_else(|| "true".to_string())
                == "true";

            let db_arc = Arc::new(Mutex::new(conn));
            let ai_provider_arc = Arc::new(Mutex::new(Some(provider)));
            let watcher_enabled_arc = Arc::new(Mutex::new(watch_enabled));
            let embedding_runtime = Arc::new(Mutex::new(EmbeddingRuntimeStatus::default()));

            let (embedding_tx, embedding_rx) = mpsc::unbounded_channel();

            embedding::spawn_embedding_worker(
                db_path.to_string_lossy().to_string(),
                ai_provider_arc.clone(),
                embedding_runtime.clone(),
                app.handle().clone(),
                embedding_rx,
            );

            if watch_enabled && !collection_paths.is_empty() {
                match crate::core::watcher::start_watcher(
                    db_arc.clone(),
                    collection_paths,
                    Some(app.handle().clone()),
                    Some(embedding_tx.clone()),
                ) {
                    Ok(_shutdown) => info!("File watcher started"),
                    Err(error) => log::error!("Failed to start file watcher: {}", error),
                }
            }

            let _ = embedding_tx.send(EmbedCommand::Kick);

            app.manage(AppState {
                db: db_arc,
                ai_provider: ai_provider_arc,
                db_path: db_path.to_string_lossy().to_string(),
                watcher_enabled: watcher_enabled_arc,
                embedding_tx,
                embedding_runtime,
            });

            info!("SmartSearch initialized");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::collections::add_collection,
            commands::collections::list_collections,
            commands::collections::remove_collection,
            commands::search::add_manual_files,
            commands::search::list_indexed_files,
            commands::search::remove_manual_file,
            commands::search::scan_collections,
            commands::search::search_keyword,
            commands::search::get_foreground_app,
            commands::search::query_with_context,
            commands::settings::get_index_stats,
            commands::settings::get_index_overview,
            commands::settings::list_document_statuses,
            commands::settings::get_embedding_runtime,
            commands::settings::check_ai_status,
            commands::settings::test_ai_provider,
            commands::settings::get_setting,
            commands::settings::set_setting,
            commands::settings::retry_failed_embeddings,
            commands::settings::retry_document_embeddings,
            commands::settings::reembed_all,
        ])
        .run(tauri::generate_context!())
        .expect("error while running SmartSearch");
}

pub fn read_provider_from_conn(conn: &Connection) -> AiProvider {
    let provider_kind = db::get_setting(conn, "ai_provider")
        .ok()
        .flatten()
        .unwrap_or_else(|| "ollama".to_string());
    let ollama_url = db::get_setting(conn, "ollama_base_url")
        .ok()
        .flatten()
        .unwrap_or_else(|| "http://localhost:11434".to_string());
    let lmstudio_url = db::get_setting(conn, "lmstudio_base_url")
        .ok()
        .flatten()
        .unwrap_or_else(|| "http://localhost:1234".to_string());
    let embed_model = db::get_setting(conn, "embed_model")
        .ok()
        .flatten()
        .unwrap_or_else(|| "nomic-embed-text".to_string());
    let gemini_api_key = db::get_setting(conn, "gemini_api_key")
        .ok()
        .flatten()
        .unwrap_or_default();

    AiProvider::from_settings(
        &provider_kind,
        &ollama_url,
        &lmstudio_url,
        &gemini_api_key,
        &embed_model,
    )
}

pub fn reload_ai_provider(state: &AppState) -> Result<(), String> {
    let provider = {
        let conn = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;
        read_provider_from_conn(&conn)
    };

    let mut guard = state.ai_provider.lock().map_err(|e| format!("Lock error: {}", e))?;
    *guard = Some(provider);
    Ok(())
}
