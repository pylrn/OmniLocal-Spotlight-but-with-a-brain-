// SmartSearch — Main Tauri Application
// Wires up modules, background embedding, live status, and IPC commands.

mod ai;
mod chunker;
mod context;
mod db;
pub mod mcp;
mod scanner;
mod search;
mod watcher;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use ai::{AiProvider, ProviderStatus};
use db::{CollectionRow, DocumentStatusRow, IndexOverview, IndexStats, ProviderHealth};
use log::info;
use rusqlite::Connection;
use scanner::ScanResult;
use search::SearchResult;
use tauri::{Emitter, Manager};
use tokio::sync::mpsc;

#[derive(Debug, Clone, serde::Serialize)]
pub struct EmbeddingRuntimeStatus {
    pub phase: String,
    pub current_title: Option<String>,
    pub current_path: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub message: String,
}

impl Default for EmbeddingRuntimeStatus {
    fn default() -> Self {
        Self {
            phase: "idle".to_string(),
            current_title: None,
            current_path: None,
            provider: None,
            model: None,
            message: "Waiting for indexing work".to_string(),
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    db: Arc<Mutex<Connection>>,
    ai_provider: Arc<Mutex<Option<AiProvider>>>,
    db_path: String,
    watcher_enabled: Arc<Mutex<bool>>,
    embedding_tx: mpsc::UnboundedSender<EmbedCommand>,
    embedding_runtime: Arc<Mutex<EmbeddingRuntimeStatus>>,
}

#[derive(Debug, Clone)]
pub(crate) enum EmbedCommand {
    Kick,
    RetryFailed,
    ReembedAll,
}

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

#[tauri::command]
async fn add_manual_files(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    paths: Vec<String>,
) -> Result<(), String> {
    let db_arc = state.db.clone();
    let added_paths = paths.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let conn = db_arc.lock().map_err(|e| format!("Lock error: {}", e))?;

        let collections = db::list_collections(&conn).unwrap_or_default();
        let manual_collection = collections.iter().find(|collection| collection.name == "Standalone Files");
        let collection_id = if let Some(collection) = manual_collection {
            collection.id
        } else {
            db::insert_collection(&conn, "Standalone Files", "/", "**/*", None)
                .map_err(|e| format!("DB error: {}", e))?
        };

        for path in paths {
            let file_path = PathBuf::from(&path);
            let _ = scanner::index_manual_file(&conn, &file_path, collection_id);
        }

        Ok::<(), String>(())
    })
    .await
    .map_err(|e| format!("Runtime error: {}", e))??;

    kick_embeddings(&state);
    let _ = app.emit(
        "index-progress",
        serde_json::json!({
            "phase": "manual-files",
            "count": added_paths.len(),
            "message": "Standalone files indexed"
        }),
    );
    Ok(())
}

#[tauri::command]
fn list_indexed_files(state: tauri::State<'_, AppState>) -> Result<Vec<db::DocumentRow>, String> {
    let conn = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;
    db::list_documents(&conn).map_err(|e| format!("DB access failed: {}", e))
}

#[tauri::command]
fn remove_manual_file(state: tauri::State<'_, AppState>, document_id: i64) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;
    db::remove_document(&conn, document_id).map_err(|e| format!("Failed to remove document: {}", e))
}

#[tauri::command]
async fn scan_collections(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<ScanResult>, String> {
    let db_arc = state.db.clone();
    let results = tauri::async_runtime::spawn_blocking(move || {
        let conn = db_arc.lock().map_err(|e| format!("Lock error: {}", e))?;
        let collections = db::list_collections(&conn).map_err(|e| format!("Failed to list collections: {}", e))?;

        let mut results = Vec::new();
        for collection in collections {
            let glob = collection.glob_pattern.unwrap_or_else(|| "**/*.md".to_string());
            match scanner::scan_collection(&conn, collection.id, &collection.path, &glob, &collection.name) {
                Ok(result) => results.push(result),
                Err(error) => results.push(ScanResult {
                    collection_name: collection.name,
                    files_found: 0,
                    files_indexed: 0,
                    files_unchanged: 0,
                    files_removed: 0,
                    errors: vec![error],
                }),
            }
        }

        Ok::<Vec<ScanResult>, String>(results)
    })
    .await
    .map_err(|e| format!("Tokio task panicked: {}", e))??;

    kick_embeddings(&state);
    let _ = app.emit(
        "index-progress",
        serde_json::json!({
            "phase": "scan-complete",
            "results": results.clone()
        }),
    );

    Ok(results)
}

#[tauri::command]
async fn search_keyword(
    state: tauri::State<'_, AppState>,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<SearchResult>, String> {
    let limit = limit.unwrap_or(10);
    let foreground_app = context::get_foreground_app();

    let provider = {
        let guard = state.ai_provider.lock().map_err(|e| format!("Lock error: {}", e))?;
        guard.clone()
    };

    let query_embedding = if let Some(provider) = provider {
        provider.embed_query(query.clone()).await.ok()
    } else {
        None
    };

    let conn = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;
    let mut results = search::hybrid_search(&conn, &query, query_embedding.as_deref(), limit)?;

    if let Some(ref app_name) = foreground_app {
        for result in &mut results {
            let boost = search::get_context_boost(app_name, &result.file_type);
            if boost > 0.0 {
                result.score *= 1.0 + boost;
                result.context_boosted = true;
            }
        }

        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    }

    Ok(results)
}

#[tauri::command]
fn get_index_stats(state: tauri::State<'_, AppState>) -> Result<IndexStats, String> {
    let conn = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;
    db::get_index_stats(&conn).map_err(|e| format!("Failed to get stats: {}", e))
}

#[tauri::command]
async fn get_index_overview(state: tauri::State<'_, AppState>) -> Result<IndexOverview, String> {
    let provider = {
        let guard = state.ai_provider.lock().map_err(|e| format!("Lock error: {}", e))?;
        guard.clone()
    };

    let provider_health = match provider {
        Some(provider) => provider_health_from_status(provider.check_status().await),
        None => ProviderHealth {
            provider: "Unavailable".to_string(),
            model_name: String::new(),
            connected: false,
            model_available: false,
            dimensions: None,
            error: Some("AI provider is not configured".to_string()),
        },
    };

    let watcher_enabled = *state
        .watcher_enabled
        .lock()
        .map_err(|e| format!("Lock error: {}", e))?;
    let conn = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;

    db::get_index_overview(&conn, &state.db_path, provider_health, watcher_enabled)
        .map_err(|e| format!("Failed to get overview: {}", e))
}

#[tauri::command]
fn list_document_statuses(state: tauri::State<'_, AppState>) -> Result<Vec<DocumentStatusRow>, String> {
    let conn = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;
    db::list_document_statuses(&conn).map_err(|e| format!("Failed to list document statuses: {}", e))
}

#[tauri::command]
fn get_embedding_runtime(state: tauri::State<'_, AppState>) -> Result<EmbeddingRuntimeStatus, String> {
    let runtime = state
        .embedding_runtime
        .lock()
        .map_err(|e| format!("Lock error: {}", e))?
        .clone();
    Ok(runtime)
}

#[tauri::command]
async fn check_ai_status(state: tauri::State<'_, AppState>) -> Result<ProviderStatus, String> {
    let provider = {
        let guard = state.ai_provider.lock().map_err(|e| format!("Lock error: {}", e))?;
        guard.clone()
    };

    match provider {
        Some(provider) => Ok(provider.check_status().await),
        None => Err("AI provider not configured".to_string()),
    }
}

#[tauri::command]
async fn test_ai_provider(state: tauri::State<'_, AppState>) -> Result<ProviderStatus, String> {
    check_ai_status(state).await
}

#[tauri::command]
fn get_setting(state: tauri::State<'_, AppState>, key: String) -> Result<Option<String>, String> {
    let conn = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;
    db::get_setting(&conn, &key).map_err(|e| format!("Failed to get setting: {}", e))
}

#[tauri::command]
async fn set_setting(
    state: tauri::State<'_, AppState>,
    key: String,
    value: String,
) -> Result<(), String> {
    {
        let conn = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;
        db::set_setting(&conn, &key, &value).map_err(|e| format!("Failed to set setting: {}", e))?;
    }

    if key == "watch_enabled" {
        let mut watcher_enabled = state
            .watcher_enabled
            .lock()
            .map_err(|e| format!("Lock error: {}", e))?;
        *watcher_enabled = value == "true";
    }

    if matches!(
        key.as_str(),
        "ai_provider" | "ollama_base_url" | "lmstudio_base_url" | "gemini_api_key" | "embed_model"
    ) {
        reload_ai_provider(&state)?;
        kick_embeddings(&state);
    }

    if key == "auto_embed_enabled" && value == "true" {
        kick_embeddings(&state);
    }

    Ok(())
}

#[tauri::command]
fn retry_failed_embeddings(state: tauri::State<'_, AppState>) -> Result<(), String> {
    {
        let conn = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;
        db::retry_failed_chunks(&conn).map_err(|e| format!("Failed to retry failed embeddings: {}", e))?;
    }

    state
        .embedding_tx
        .send(EmbedCommand::RetryFailed)
        .map_err(|e| format!("Failed to queue retry: {}", e))?;

    Ok(())
}

#[tauri::command]
fn reembed_all(state: tauri::State<'_, AppState>) -> Result<(), String> {
    {
        let conn = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;
        db::mark_all_chunks_pending(&conn).map_err(|e| format!("Failed to mark chunks pending: {}", e))?;
    }

    state
        .embedding_tx
        .send(EmbedCommand::ReembedAll)
        .map_err(|e| format!("Failed to queue re-embed all: {}", e))?;

    Ok(())
}

#[tauri::command]
fn get_foreground_app() -> Option<String> {
    context::get_foreground_app()
}

#[derive(Debug, serde::Deserialize)]
pub struct ContextSnippet {
    pub snippet: String,
    pub path: String,
}

#[tauri::command]
async fn query_with_context(
    state: tauri::State<'_, AppState>,
    query: String,
    snippets: Vec<ContextSnippet>,
) -> Result<String, String> {
    let (api_key, model) = {
        let conn = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;
        let key = db::get_setting(&conn, "query_api_key")
            .ok()
            .flatten()
            .unwrap_or_default();
        let model = db::get_setting(&conn, "query_model")
            .ok()
            .flatten()
            .unwrap_or_else(|| "gemini-2.0-flash-lite".to_string());
        (key, model)
    };

    let context: Vec<(String, String)> = snippets
        .into_iter()
        .map(|s| (s.snippet, s.path))
        .collect();

    ai::gemini_generate_answer(&api_key, &model, &query, &context).await
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

            spawn_embedding_worker(
                db_arc.clone(),
                ai_provider_arc.clone(),
                embedding_runtime.clone(),
                app.handle().clone(),
                embedding_rx,
            );

            if watch_enabled && !collection_paths.is_empty() {
                match watcher::start_watcher(
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
            add_collection,
            list_collections,
            remove_collection,
            add_manual_files,
            list_indexed_files,
            remove_manual_file,
            scan_collections,
            search_keyword,
            get_index_stats,
            get_index_overview,
            list_document_statuses,
            get_embedding_runtime,
            check_ai_status,
            test_ai_provider,
            get_setting,
            set_setting,
            retry_failed_embeddings,
            reembed_all,
            get_foreground_app,
            query_with_context,
        ])
        .run(tauri::generate_context!())
        .expect("error while running SmartSearch");
}

fn read_provider_from_conn(conn: &Connection) -> AiProvider {
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

fn reload_ai_provider(state: &AppState) -> Result<(), String> {
    let provider = {
        let conn = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;
        read_provider_from_conn(&conn)
    };

    let mut guard = state.ai_provider.lock().map_err(|e| format!("Lock error: {}", e))?;
    *guard = Some(provider);
    Ok(())
}

fn kick_embeddings(state: &AppState) {
    let _ = state.embedding_tx.send(EmbedCommand::Kick);
}

fn provider_health_from_status(status: ProviderStatus) -> ProviderHealth {
    ProviderHealth {
        provider: status.provider.as_str().to_string(),
        model_name: status.model_name,
        connected: status.connected,
        model_available: status.model_available,
        dimensions: status.dimensions,
        error: status.error,
    }
}

fn spawn_embedding_worker(
    db: Arc<Mutex<Connection>>,
    ai_provider: Arc<Mutex<Option<AiProvider>>>,
    runtime: Arc<Mutex<EmbeddingRuntimeStatus>>,
    app: tauri::AppHandle,
    mut rx: mpsc::UnboundedReceiver<EmbedCommand>,
) {
    tauri::async_runtime::spawn(async move {
        while let Some(command) = rx.recv().await {
            match command {
                EmbedCommand::RetryFailed | EmbedCommand::ReembedAll | EmbedCommand::Kick => {
                    if let Err(error) = process_pending_embeddings(&db, &ai_provider, &runtime, &app).await {
                        set_runtime(
                            &runtime,
                            EmbeddingRuntimeStatus {
                                phase: "error".to_string(),
                                current_title: None,
                                current_path: None,
                                provider: None,
                                model: None,
                                message: error.clone(),
                            },
                        );
                        emit_runtime(&app, &db, &runtime);
                    }
                }
            }
        }
    });
}

async fn process_pending_embeddings(
    db: &Arc<Mutex<Connection>>,
    ai_provider: &Arc<Mutex<Option<AiProvider>>>,
    runtime: &Arc<Mutex<EmbeddingRuntimeStatus>>,
    app: &tauri::AppHandle,
) -> Result<(), String> {
    loop {
        let auto_embed_enabled = {
            let conn = db.lock().map_err(|e| format!("Lock error: {}", e))?;
            db::get_setting(&conn, "auto_embed_enabled")
                .map_err(|e| format!("DB error: {}", e))?
                .unwrap_or_else(|| "true".to_string())
                == "true"
        };

        if !auto_embed_enabled {
            set_runtime(
                runtime,
                EmbeddingRuntimeStatus {
                    phase: "paused".to_string(),
                    current_title: None,
                    current_path: None,
                    provider: None,
                    model: None,
                    message: "Auto-embedding is disabled".to_string(),
                },
            );
            emit_runtime(app, db, runtime);
            return Ok(());
        }

        let batch_size = {
            let conn = db.lock().map_err(|e| format!("Lock error: {}", e))?;
            db::get_setting(&conn, "embed_batch_size")
                .ok()
                .flatten()
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(12)
        };

        let pending_chunks = {
            let conn = db.lock().map_err(|e| format!("Lock error: {}", e))?;
            db::list_pending_chunks(&conn, batch_size).map_err(|e| format!("Failed to list pending chunks: {}", e))?
        };

        if pending_chunks.is_empty() {
            set_runtime(runtime, EmbeddingRuntimeStatus::default());
            emit_runtime(app, db, runtime);
            return Ok(());
        }

        let provider = {
            let guard = ai_provider.lock().map_err(|e| format!("Lock error: {}", e))?;
            guard.clone()
        }
        .ok_or_else(|| "AI provider is not configured".to_string())?;

        {
            let conn = db.lock().map_err(|e| format!("Lock error: {}", e))?;
            for chunk in &pending_chunks {
                db::mark_chunk_embedding_in_progress(&conn, chunk.chunk_id)
                    .map_err(|e| format!("Failed to mark chunk processing: {}", e))?;
            }
        }

        let lead = &pending_chunks[0];
        set_runtime(
            runtime,
            EmbeddingRuntimeStatus {
                phase: "embedding".to_string(),
                current_title: lead.document_title.clone(),
                current_path: Some(lead.abs_path.clone()),
                provider: Some(provider.provider_name().to_string()),
                model: Some(provider.model_name().to_string()),
                message: format!("Embedding {} chunk(s)", pending_chunks.len()),
            },
        );
        emit_runtime(app, db, runtime);

        let results = embed_batch_with_fallback(&provider, &pending_chunks).await;
        {
            let conn = db.lock().map_err(|e| format!("Lock error: {}", e))?;
            for (chunk, embedding_result) in pending_chunks.iter().zip(results.into_iter()) {
                match embedding_result {
                    Ok(vector) => db::mark_chunk_embedding_success(
                        &conn,
                        chunk.chunk_id,
                        provider.provider_name(),
                        provider.model_name(),
                        &vector,
                    )
                    .map_err(|e| format!("Failed to store embedding: {}", e))?,
                    Err(error) => db::mark_chunk_embedding_failed(&conn, chunk.chunk_id, &error)
                        .map_err(|e| format!("Failed to store embedding error: {}", e))?,
                }
            }
        }

        emit_runtime(app, db, runtime);
    }
}

async fn embed_batch_with_fallback(
    provider: &AiProvider,
    pending_chunks: &[db::PendingChunk],
) -> Vec<Result<Vec<f32>, String>> {
    let texts = pending_chunks.iter().map(|chunk| chunk.content.clone()).collect::<Vec<_>>();
    match provider.embed_documents(texts).await {
        Ok(embeddings) if embeddings.len() == pending_chunks.len() => embeddings.into_iter().map(Ok).collect(),
        Ok(_) | Err(_) => {
            let mut results = Vec::with_capacity(pending_chunks.len());
            for chunk in pending_chunks {
                match provider.embed_documents(vec![chunk.content.clone()]).await {
                    Ok(mut embeddings) => {
                        if let Some(vector) = embeddings.pop() {
                            results.push(Ok(vector));
                        } else {
                            results.push(Err(format!("No embedding returned for {}", chunk.path)));
                        }
                    }
                    Err(error) => results.push(Err(error)),
                }
            }
            results
        }
    }
}

fn set_runtime(runtime: &Arc<Mutex<EmbeddingRuntimeStatus>>, next: EmbeddingRuntimeStatus) {
    if let Ok(mut guard) = runtime.lock() {
        *guard = next;
    }
}

fn emit_runtime(
    app: &tauri::AppHandle,
    db: &Arc<Mutex<Connection>>,
    runtime: &Arc<Mutex<EmbeddingRuntimeStatus>>,
) {
    let runtime_snapshot = runtime.lock().map(|guard| guard.clone()).unwrap_or_default();
    let stats = db
        .lock()
        .ok()
        .and_then(|conn| db::get_index_stats(&conn).ok())
        .unwrap_or(IndexStats {
            total_collections: 0,
            total_documents: 0,
            total_chunks: 0,
            embedded_chunks: 0,
            pending_chunks: 0,
            failed_chunks: 0,
        });

    let _ = app.emit(
        "embedding-progress",
        serde_json::json!({
            "runtime": runtime_snapshot,
            "stats": stats,
        }),
    );
}
