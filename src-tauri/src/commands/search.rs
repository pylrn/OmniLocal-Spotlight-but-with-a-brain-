use std::path::PathBuf;
use tauri::{AppHandle, Emitter, State};
use crate::AppState;
use crate::core::db;
use crate::core::scanner;
use crate::core::search;
use crate::core::context;
use crate::core::scanner::ScanResult;
use crate::core::search::SearchResult;
use crate::core::ai::{self, EmbeddingProvider};
use crate::workers::embedding;
use tokio::time::{timeout, Duration};

#[tauri::command]
pub async fn add_manual_files(
    app: AppHandle,
    state: State<'_, AppState>,
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

    embedding::kick_embeddings(&state.embedding_tx);
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
pub fn list_indexed_files(state: State<'_, AppState>) -> Result<Vec<db::DocumentRow>, String> {
    let conn = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;
    db::list_documents(&conn).map_err(|e| format!("DB access failed: {}", e))
}

#[tauri::command]
pub fn remove_manual_file(state: State<'_, AppState>, document_id: i64) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;
    db::remove_document(&conn, document_id).map_err(|e| format!("Failed to remove document: {}", e))
}

#[tauri::command]
pub async fn scan_collections(
    app: AppHandle,
    state: State<'_, AppState>,
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

    embedding::kick_embeddings(&state.embedding_tx);
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
pub async fn search_keyword(
    state: State<'_, AppState>,
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
        match timeout(Duration::from_millis(1500), provider.embed_query(query.clone())).await {
            Ok(Ok(embedding)) => Some(embedding),
            _ => None,
        }
    } else {
        None
    };

    let conn = rusqlite::Connection::open(&state.db_path).map_err(|e| format!("DB error: {}", e))?;
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
pub fn get_foreground_app() -> Option<String> {
    context::get_foreground_app()
}

#[derive(Debug, serde::Deserialize)]
pub struct ContextSnippet {
    pub snippet: String,
    pub path: String,
}

#[tauri::command]
pub async fn query_with_context(
    state: State<'_, AppState>,
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
            .unwrap_or_else(|| "gemini-3-flash".to_string());
        (key, model)
    };

    let provider = ai::GeminiClient::new(&api_key, &model);

    let context_text: String = snippets
        .iter()
        .enumerate()
        .map(|(i, s)| format!("[{}] {}\n{}", i + 1, s.path, s.snippet))
        .collect::<Vec<_>>()
        .join("\n\n");

    let prompt = format!(
        "Provide a concise insight (2-4 sentences) that:\n- Directly addresses what the user was looking for\n- Highlights the most relevant finding (mention which result number)\n- Notes any key patterns across results if applicable\n\nBe specific and direct. Do not repeat file paths or source numbers unless citing."
    );
    
    let system_prompt = format!(
        "You are a local file search assistant. The user searched their local files for: \"{}\"\n\nHere are the most relevant excerpts, ranked by relevance:\n\n{}",
        query, context_text
    );

    use crate::core::ai::GenerationProvider;
    provider.generate_answer(&prompt, &system_prompt).await
}
